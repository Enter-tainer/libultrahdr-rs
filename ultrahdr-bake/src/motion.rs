use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow, bail, ensure};
use bytes::{Bytes, BytesMut};
use img_parts::jpeg::{Jpeg, JpegSegment, markers};
use quick_xml::{
    Reader, Writer,
    events::{BytesEnd, BytesStart, Event},
};

use crate::cli::MotionArgs;
use crate::detect::probe_gainmap_metadata;

const XMP_PREFIX: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";

#[derive(Debug, Clone)]
pub struct MotionInputPair {
    pub photo: PathBuf,
    pub video: PathBuf,
}

pub fn resolve_inputs(args: &MotionArgs) -> Result<MotionInputPair> {
    if args.photo.is_some() || args.video.is_some() {
        ensure!(
            args.photo.is_some() && args.video.is_some(),
            "Provide both --photo and --video together (or omit both to auto-detect)"
        );
        ensure!(
            args.inputs.is_empty(),
            "Provide --photo/--video without positional inputs, or two positional inputs without flags"
        );
        return Ok(MotionInputPair {
            photo: args.photo.clone().expect("photo is_some checked"),
            video: args.video.clone().expect("video is_some checked"),
        });
    }

    ensure!(
        args.inputs.len() == 2,
        "Provide --photo and --video, or two positional inputs for auto-detection"
    );
    auto_detect_motion_pair(&args.inputs[0], &args.inputs[1])
}

pub fn run_motion(args: &MotionArgs, inputs: &MotionInputPair, out_path: &Path) -> Result<()> {
    let photo_bytes = fs::read(&inputs.photo)
        .with_context(|| format!("Failed to read photo {}", inputs.photo.display()))?;
    let video_bytes = fs::read(&inputs.video)
        .with_context(|| format!("Failed to read video {}", inputs.video.display()))?;

    let mut base_jpeg = Jpeg::from_bytes(Bytes::copy_from_slice(&photo_bytes))
        .with_context(|| format!("Failed to parse JPEG {}", inputs.photo.display()))?;
    let existing_xmp = take_existing_xmp(base_jpeg.segments_mut());

    let mut probe_buf = photo_bytes.clone();
    let gainmap_present = probe_gainmap_metadata(&mut probe_buf)?.is_some();

    let (primary_bytes, meta) = if gainmap_present {
        build_ultrahdr_motion(
            &photo_bytes,
            &video_bytes,
            &base_jpeg,
            existing_xmp.as_deref(),
            args.presentation_timestamp_us,
        )?
    } else {
        build_plain_motion(
            &video_bytes,
            &base_jpeg,
            existing_xmp.as_deref(),
            args.presentation_timestamp_us,
        )?
    };

    let mut out = Vec::with_capacity(primary_bytes.len() + video_bytes.len());
    out.extend_from_slice(&primary_bytes);
    out.extend_from_slice(&video_bytes);

    fs::write(out_path, &out).with_context(|| format!("Failed to write {}", out_path.display()))?;
    println!(
        "Wrote Motion Photo {} (JPEG {} bytes{}video {} bytes, offset {})",
        out_path.display(),
        primary_bytes.len(),
        meta.gainmap_len
            .map(|n| format!(", gain map {} bytes, ", n))
            .unwrap_or_else(|| ", ".to_string()),
        video_bytes.len(),
        primary_bytes.len()
    );
    Ok(())
}

fn auto_detect_motion_pair(a: &Path, b: &Path) -> Result<MotionInputPair> {
    let a_kind = detect_media_kind(a)?;
    let b_kind = detect_media_kind(b)?;

    match (a_kind, b_kind) {
        (MediaKind::Jpeg, MediaKind::Mp4) => Ok(MotionInputPair {
            photo: a.to_path_buf(),
            video: b.to_path_buf(),
        }),
        (MediaKind::Mp4, MediaKind::Jpeg) => Ok(MotionInputPair {
            photo: b.to_path_buf(),
            video: a.to_path_buf(),
        }),
        (MediaKind::Jpeg, MediaKind::Jpeg) => {
            bail!("Both inputs look like JPEGs; please specify --video for the MP4 explicitly")
        }
        (MediaKind::Mp4, MediaKind::Mp4) => {
            bail!("Both inputs look like MP4s; please specify --photo for the JPEG explicitly")
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MediaKind {
    Jpeg,
    Mp4,
}

fn detect_media_kind(path: &Path) -> Result<MediaKind> {
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let lower = ext.to_ascii_lowercase();
        if lower == "jpg" || lower == "jpeg" {
            return Ok(MediaKind::Jpeg);
        }
        if lower == "mp4" || lower == "m4v" || lower == "mov" || lower == "qt" {
            return Ok(MediaKind::Mp4);
        }
    }

    let mut file = fs::File::open(path)
        .with_context(|| format!("Failed to open {} for type detection", path.display()))?;
    let mut buf = [0u8; 12];
    let n = file
        .read(&mut buf)
        .with_context(|| format!("Failed to read {} for type detection", path.display()))?;
    if n >= 3 && buf[0] == 0xFF && buf[1] == 0xD8 && buf[2] == 0xFF {
        return Ok(MediaKind::Jpeg);
    }
    if n >= 8 && &buf[4..8] == b"ftyp" {
        return Ok(MediaKind::Mp4);
    }

    bail!("Unrecognized media type for {}", path.display())
}

fn build_plain_motion(
    video_bytes: &[u8],
    base_jpeg: &Jpeg,
    existing_xmp: Option<&[u8]>,
    presentation_timestamp_us: u64,
) -> Result<(Vec<u8>, MotionMeta)> {
    // Iteratively rebuild until the embedded offset stabilizes.
    let mut encoded = base_jpeg.clone().encoder().bytes();
    for _ in 0..4 {
        let meta = MotionMeta {
            primary_len: encoded.len(),
            gainmap_len: None,
            video_len: video_bytes.len(),
            presentation_timestamp_us,
        };
        let xmp = build_motion_xmp(existing_xmp, &meta)?;
        let mut working = base_jpeg.clone();
        upsert_xmp(working.segments_mut(), xmp);
        let new_encoded = working.encoder().bytes();
        if new_encoded.len() == meta.primary_len {
            encoded = new_encoded;
            break;
        }
        encoded = new_encoded;
    }
    let meta = MotionMeta {
        primary_len: encoded.len(),
        gainmap_len: None,
        video_len: video_bytes.len(),
        presentation_timestamp_us,
    };
    Ok((encoded.to_vec(), meta))
}

fn build_ultrahdr_motion(
    photo_bytes: &[u8],
    video_bytes: &[u8],
    base_jpeg: &Jpeg,
    existing_xmp: Option<&[u8]>,
    presentation_timestamp_us: u64,
) -> Result<(Vec<u8>, MotionMeta)> {
    let mut mpf_probe =
        Jpeg::from_bytes(Bytes::copy_from_slice(photo_bytes)).context("Parse JPEG for MPF")?;
    let mpf_info = find_mpf_segment(mpf_probe.segments_mut())?;
    let mut working = base_jpeg.clone();
    let mut meta = MotionMeta {
        primary_len: mpf_info.primary_size,
        gainmap_len: Some(mpf_info.secondary_size),
        video_len: video_bytes.len(),
        presentation_timestamp_us,
    };

    // Encode once using MPF-reported sizes.
    let mut xmp = build_motion_xmp(existing_xmp, &meta)?;
    upsert_xmp(working.segments_mut(), xmp);
    let mut encoded = working.clone().encoder().bytes();
    let mut primary_len = encoded
        .len()
        .checked_sub(mpf_info.secondary_size)
        .ok_or_else(|| anyhow!("Primary length underflow after encode"))?;

    // If XMP length guess was off, rebuild once with the measured primary length.
    if primary_len != meta.primary_len {
        meta.primary_len = primary_len;
        xmp = build_motion_xmp(existing_xmp, &meta)?;
        working = base_jpeg.clone();
        upsert_xmp(working.segments_mut(), xmp);
        encoded = working.clone().encoder().bytes();
        primary_len = encoded
            .len()
            .checked_sub(mpf_info.secondary_size)
            .ok_or_else(|| anyhow!("Primary length underflow after re-encode"))?;
        meta.primary_len = primary_len;
    }

    // Patch MPF using the measured primary length.
    let tiff_base = find_mpf_offset_bytes(&encoded)
        .map(|idx| idx + 4)
        .ok_or_else(|| anyhow!("Failed to find MPF payload start"))?;
    let mpf_current =
        find_mpf_segment(working.segments()).context("Locate MPF after XMP insert")?;
    let gainmap_offset = primary_len
        .checked_sub(tiff_base)
        .ok_or_else(|| anyhow!("Gain map offset underflow"))?;
    let new_mpf = build_mpf_payload(primary_len, mpf_info.secondary_size, gainmap_offset)?;
    replace_mpf_segment(working.segments_mut(), &mpf_current, new_mpf);

    // Final bytes after MPF rewrite.
    let primary_bytes = working.encoder().bytes().to_vec();
    meta.primary_len = primary_bytes
        .len()
        .checked_sub(mpf_info.secondary_size)
        .ok_or_else(|| anyhow!("Primary length underflow after MPF rewrite"))?;

    Ok((primary_bytes, meta))
}

fn take_existing_xmp(segments: &mut Vec<JpegSegment>) -> Option<Vec<u8>> {
    let mut found = None;
    let mut i = 0;
    while i < segments.len() {
        if segments[i].marker() == markers::APP1 && segments[i].contents().starts_with(XMP_PREFIX) {
            if found.is_none() {
                found = Some(segments[i].contents().slice(XMP_PREFIX.len()..).to_vec());
            }
            segments.remove(i);
        } else {
            i += 1;
        }
    }
    found
}

fn upsert_xmp(segments: &mut Vec<JpegSegment>, xmp_body: Vec<u8>) {
    let mut contents = BytesMut::with_capacity(XMP_PREFIX.len() + xmp_body.len());
    contents.extend_from_slice(XMP_PREFIX);
    contents.extend_from_slice(&xmp_body);
    let segment = JpegSegment::new_with_contents(markers::APP1, contents.freeze());

    let insert_at = segments
        .iter()
        .position(|s| {
            let m = s.marker();
            m != markers::APP0 && m != markers::APP1
        })
        .unwrap_or(segments.len());
    segments.insert(insert_at, segment);
}

struct MotionMeta {
    primary_len: usize,
    gainmap_len: Option<usize>,
    video_len: usize,
    presentation_timestamp_us: u64,
}

#[derive(Debug, Clone)]
struct MpfInfo {
    segment_index: usize,
    primary_size: usize,
    secondary_size: usize,
}

fn find_mpf_segment(segments: &[JpegSegment]) -> Result<MpfInfo> {
    let (idx, seg) = segments
        .iter()
        .enumerate()
        .find(|(_, s)| s.marker() == markers::APP2 && s.contents().starts_with(b"MPF\0"))
        .ok_or_else(|| anyhow!("MPF APP2 segment not found; UltraHDR layout missing"))?;
    parse_mpf_payload(seg.contents()).map(|mut info| {
        info.segment_index = idx;
        info
    })
}

fn parse_mpf_payload(payload: &[u8]) -> Result<MpfInfo> {
    if payload.len() < 12 {
        return Err(anyhow!("MPF payload too short"));
    }
    if &payload[..4] != b"MPF\0" {
        return Err(anyhow!("MPF payload missing signature"));
    }
    let endian = &payload[4..8];
    let be = match endian {
        [0x4D, 0x4D, 0x00, 0x2A] => true,
        [0x49, 0x49, 0x2A, 0x00] => false,
        _ => return Err(anyhow!("MPF payload has unknown endianness")),
    };
    let read_u16 = |buf: &[u8], offset: usize, be: bool| -> Result<u16> {
        let slice = buf
            .get(offset..offset + 2)
            .ok_or_else(|| anyhow!("MPF read_u16 out of bounds"))?;
        Ok(if be {
            u16::from_be_bytes([slice[0], slice[1]])
        } else {
            u16::from_le_bytes([slice[0], slice[1]])
        })
    };
    let read_u32 = |buf: &[u8], offset: usize, be: bool| -> Result<u32> {
        let slice = buf
            .get(offset..offset + 4)
            .ok_or_else(|| anyhow!("MPF read_u32 out of bounds"))?;
        Ok(if be {
            u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]])
        } else {
            u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]])
        })
    };

    let tiff_base = 4usize; // offsets are relative to the TIFF header (after signature)
    let ifd_offset = read_u32(payload, 8, be)? as usize;
    let ifd_pos = tiff_base
        .checked_add(ifd_offset)
        .ok_or_else(|| anyhow!("MPF IFD offset overflow"))?;
    let entry_count = read_u16(payload, ifd_pos, be)? as usize;

    let mut mp_entry_offset: Option<usize> = None;
    let mut mp_entry_bytes_len: Option<usize> = None;
    let mut num_images: Option<usize> = None;
    for i in 0..entry_count {
        let base = ifd_pos + 2 + i * 12;
        let tag = read_u16(payload, base, be)?;
        let typ = read_u16(payload, base + 2, be)?;
        let count = read_u32(payload, base + 4, be)? as usize;
        let value_or_offset = read_u32(payload, base + 8, be)? as usize;

        match tag {
            0xB001 => {
                num_images = Some(value_or_offset as usize);
            }
            0xB002 => {
                // Type undefined, count is total bytes; value_or_offset points at the data if > 4 bytes.
                if typ != 0x7 {
                    return Err(anyhow!("MPF MPEntry tag has unexpected type"));
                }
                mp_entry_bytes_len = Some(count);
                mp_entry_offset = if count > 4 {
                    Some(
                        tiff_base
                            .checked_add(value_or_offset)
                            .ok_or_else(|| anyhow!("MPF MPEntry offset overflow"))?,
                    )
                } else {
                    Some(base + 8)
                };
            }
            _ => {}
        }
    }

    let entries = num_images.ok_or_else(|| anyhow!("MPF missing image count tag"))?;
    let entry_offset = mp_entry_offset.ok_or_else(|| anyhow!("MPF missing MPEntry offset"))?;
    let entry_bytes_len = mp_entry_bytes_len.unwrap_or(entries * 16);
    let entry_bytes = payload
        .get(entry_offset..entry_offset + entry_bytes_len)
        .ok_or_else(|| anyhow!("MPF entries out of bounds"))?;
    if entry_bytes.len() < entries * 16 {
        return Err(anyhow!("MPF entries too short"));
    }

    let read_entry = |idx: usize| -> Result<(u32, u32, u32)> {
        let start = idx
            .checked_mul(16)
            .ok_or_else(|| anyhow!("MPF entry index overflow"))?;
        let slice = entry_bytes
            .get(start..start + 16)
            .ok_or_else(|| anyhow!("MPF entry slice out of bounds"))?;
        let attr = if be {
            u32::from_be_bytes(slice[0..4].try_into().unwrap())
        } else {
            u32::from_le_bytes(slice[0..4].try_into().unwrap())
        };
        let size = if be {
            u32::from_be_bytes(slice[4..8].try_into().unwrap())
        } else {
            u32::from_le_bytes(slice[4..8].try_into().unwrap())
        };
        let offset = if be {
            u32::from_be_bytes(slice[8..12].try_into().unwrap())
        } else {
            u32::from_le_bytes(slice[8..12].try_into().unwrap())
        };
        Ok((attr, size, offset))
    };

    if entries < 2 {
        return Err(anyhow!("MPF entries missing secondary image"));
    }

    let (_, primary_size, _) = read_entry(0)?;
    let (_, secondary_size, _) = read_entry(1)?;

    Ok(MpfInfo {
        segment_index: 0,
        primary_size: primary_size as usize,
        secondary_size: secondary_size as usize,
    })
}

fn build_mpf_payload(
    primary_size: usize,
    gainmap_size: usize,
    gainmap_offset_from_tiff: usize,
) -> Result<Vec<u8>> {
    let p_size = u32::try_from(primary_size).map_err(|_| anyhow!("Primary size too large"))?;
    let g_size = u32::try_from(gainmap_size).map_err(|_| anyhow!("Gain map size too large"))?;
    let g_off = u32::try_from(gainmap_offset_from_tiff)
        .map_err(|_| anyhow!("Gain map offset too large"))?;

    let mut buf = Vec::with_capacity(4 + 4 + 4 + 2 + 3 * 12 + 4 + 2 * 16);
    buf.extend_from_slice(b"MPF\0");
    buf.extend_from_slice(&[0x4D, 0x4D, 0x00, 0x2A]); // big endian
    buf.extend_from_slice(&(8u32.to_be_bytes())); // IFD offset from TIFF base (4 bytes into payload)
    buf.extend_from_slice(&(3u16.to_be_bytes())); // tag count

    // Version tag
    buf.extend_from_slice(&0xB000u16.to_be_bytes());
    buf.extend_from_slice(&0x0007u16.to_be_bytes());
    buf.extend_from_slice(&4u32.to_be_bytes());
    buf.extend_from_slice(b"0100");

    // Number of images tag
    buf.extend_from_slice(&0xB001u16.to_be_bytes());
    buf.extend_from_slice(&0x0004u16.to_be_bytes());
    buf.extend_from_slice(&1u32.to_be_bytes());
    buf.extend_from_slice(&2u32.to_be_bytes());

    // MP entry tag (placeholder offset)
    buf.extend_from_slice(&0xB002u16.to_be_bytes());
    buf.extend_from_slice(&0x0007u16.to_be_bytes());
    buf.extend_from_slice(&(32u32.to_be_bytes()));
    let offset_pos = buf.len();
    buf.extend_from_slice(&0u32.to_be_bytes()); // filled later

    // Attribute IFD offset (unused)
    buf.extend_from_slice(&0u32.to_be_bytes());

    let mp_entries_start = buf.len();
    // Primary entry
    buf.extend_from_slice(&0x00030000u32.to_be_bytes());
    buf.extend_from_slice(&p_size.to_be_bytes());
    buf.extend_from_slice(&0u32.to_be_bytes());
    buf.extend_from_slice(&0u16.to_be_bytes());
    buf.extend_from_slice(&0u16.to_be_bytes());

    // Gain map entry
    buf.extend_from_slice(&0x00000000u32.to_be_bytes());
    buf.extend_from_slice(&g_size.to_be_bytes());
    buf.extend_from_slice(&g_off.to_be_bytes());
    buf.extend_from_slice(&0u16.to_be_bytes());
    buf.extend_from_slice(&0u16.to_be_bytes());

    let mp_entry_offset = mp_entries_start
        .checked_sub(4)
        .ok_or_else(|| anyhow!("MPF entry offset underflow"))? as u32; // relative to TIFF base (payload offset 4)
    buf[offset_pos..offset_pos + 4].copy_from_slice(&mp_entry_offset.to_be_bytes());

    Ok(buf)
}

fn replace_mpf_segment(segments: &mut [JpegSegment], info: &MpfInfo, payload: Vec<u8>) {
    segments[info.segment_index] =
        JpegSegment::new_with_contents(markers::APP2, Bytes::from(payload));
}

fn find_mpf_offset_bytes(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|w| w == b"MPF\0")
}
fn build_motion_xmp(existing: Option<&[u8]>, meta: &MotionMeta) -> Result<Vec<u8>> {
    if let Some(existing) = existing
        && let Ok(merged) = merge_into_existing_xmp(existing, meta)
    {
        return Ok(merged);
    }
    Ok(build_fresh_xmp(meta))
}

fn merge_into_existing_xmp(existing: &[u8], meta: &MotionMeta) -> Result<Vec<u8>> {
    let mut reader = Reader::from_reader(existing);
    reader.config_mut().trim_text(false);
    let mut writer = Writer::new(Vec::with_capacity(existing.len() + 512));
    let mut buf = Vec::new();
    let mut injected = false;
    let mut dropping = 0usize;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) if e.name().as_ref() == b"Container:Directory" => {
                dropping = 1;
            }
            Ok(Event::Start(_)) if dropping > 0 => {
                dropping += 1;
            }
            Ok(Event::End(_)) if dropping > 0 => {
                dropping = dropping.saturating_sub(1);
            }
            Ok(Event::Empty(_)) if dropping > 0 => {}
            Ok(Event::End(ref e)) if e.name().as_ref() == b"rdf:RDF" => {
                if !injected {
                    write_motion_description(&mut writer, meta, true)?;
                    injected = true;
                }
                writer.write_event(Event::End(e.to_owned()))?;
            }
            Ok(Event::Eof) => break,
            Ok(ev) => writer.write_event(ev.to_owned())?,
            Err(e) => return Err(e.into()),
        }
        buf.clear();
    }

    if injected {
        Ok(writer.into_inner())
    } else {
        Err(anyhow!("Existing XMP missing rdf:RDF; cannot merge"))
    }
}

fn build_fresh_xmp(meta: &MotionMeta) -> Vec<u8> {
    let mut writer = Writer::new(Vec::with_capacity(512));

    let mut xmp = BytesStart::new("x:xmpmeta");
    xmp.push_attribute(("xmlns:x", "adobe:ns:meta/"));
    writer
        .write_event(Event::Start(xmp))
        .expect("write xmpmeta");

    let mut rdf = BytesStart::new("rdf:RDF");
    rdf.push_attribute(("xmlns:rdf", "http://www.w3.org/1999/02/22-rdf-syntax-ns#"));
    writer
        .write_event(Event::Start(rdf))
        .expect("write rdf start");

    write_motion_description(&mut writer, meta, true).expect("write motion description");

    writer
        .write_event(Event::End(BytesEnd::new("rdf:RDF")))
        .expect("end rdf");
    writer
        .write_event(Event::End(BytesEnd::new("x:xmpmeta")))
        .expect("end xmpmeta");

    writer.into_inner()
}

fn write_motion_description(
    writer: &mut Writer<Vec<u8>>,
    meta: &MotionMeta,
    include_container: bool,
) -> quick_xml::Result<()> {
    let ts_str = meta.presentation_timestamp_us.to_string();

    let mut desc = BytesStart::new("rdf:Description");
    desc.push_attribute(("xmlns:GCamera", "http://ns.google.com/photos/1.0/camera/"));
    desc.push_attribute((
        "xmlns:Container",
        "http://ns.google.com/photos/1.0/container/",
    ));
    desc.push_attribute((
        "xmlns:Item",
        "http://ns.google.com/photos/1.0/container/item/",
    ));
    desc.push_attribute(("GCamera:MotionPhoto", "1"));
    desc.push_attribute(("GCamera:MotionPhotoVersion", "1"));
    desc.push_attribute((
        "GCamera:MotionPhotoPresentationTimestampUs",
        ts_str.as_str(),
    ));
    writer.write_event(Event::Start(desc))?;

    if include_container {
        write_container_directory(writer, meta)?;
    }

    writer.write_event(Event::End(BytesEnd::new("rdf:Description")))?;
    Ok(())
}

fn write_container_directory(
    writer: &mut Writer<Vec<u8>>,
    meta: &MotionMeta,
) -> quick_xml::Result<()> {
    writer.write_event(Event::Start(BytesStart::new("Container:Directory")))?;
    writer.write_event(Event::Start(BytesStart::new("rdf:Seq")))?;

    write_container_item(writer, "image/jpeg", "Primary", meta.primary_len)?;
    if let Some(gainmap_len) = meta.gainmap_len {
        write_container_item(writer, "image/jpeg", "GainMap", gainmap_len)?;
    }
    write_container_item(writer, "video/mp4", "MotionPhoto", meta.video_len)?;

    writer.write_event(Event::End(BytesEnd::new("rdf:Seq")))?;
    writer.write_event(Event::End(BytesEnd::new("Container:Directory")))?;
    Ok(())
}

fn write_container_item(
    writer: &mut Writer<Vec<u8>>,
    mime: &str,
    semantic: &str,
    len: usize,
) -> quick_xml::Result<()> {
    let len_str = len.to_string();
    let mut li = BytesStart::new("rdf:li");
    li.push_attribute(("rdf:parseType", "Resource"));
    writer.write_event(Event::Start(li))?;
    let mut item = BytesStart::new("Container:Item");
    item.push_attribute(("Item:Mime", mime));
    item.push_attribute(("Item:Semantic", semantic));
    item.push_attribute(("Item:Length", len_str.as_str()));
    item.push_attribute(("Item:Padding", "0"));
    writer.write_event(Event::Empty(item))?;
    writer.write_event(Event::End(BytesEnd::new("rdf:li")))?;
    Ok(())
}
