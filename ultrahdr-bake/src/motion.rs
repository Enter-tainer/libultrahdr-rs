use std::fs;

use anyhow::{anyhow, Context, Result};
use bytes::{Bytes, BytesMut};
use img_parts::jpeg::{markers, Jpeg, JpegSegment};
use quick_xml::{
    events::{BytesEnd, BytesStart, Event},
    Reader, Writer,
};

use crate::cli::MotionArgs;

const XMP_PREFIX: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";

pub fn run_motion(args: &MotionArgs) -> Result<()> {
    let photo_bytes = fs::read(&args.photo)
        .with_context(|| format!("Failed to read photo {}", args.photo.display()))?;
    let video_bytes = fs::read(&args.video)
        .with_context(|| format!("Failed to read video {}", args.video.display()))?;

    let mut base_jpeg = Jpeg::from_bytes(Bytes::copy_from_slice(&photo_bytes))
        .with_context(|| format!("Failed to parse JPEG {}", args.photo.display()))?;
    let existing_xmp = take_existing_xmp(base_jpeg.segments_mut());

    // Iteratively rebuild until the embedded offset stabilizes.
    let mut encoded = base_jpeg.clone().encoder().bytes();
    for _ in 0..4 {
        let jpeg_len = encoded.len();
        let xmp = build_motion_xmp(
            existing_xmp.as_deref(),
            jpeg_len,
            video_bytes.len(),
            args.presentation_timestamp_us,
        )?;
        let mut working = base_jpeg.clone();
        upsert_xmp(working.segments_mut(), xmp);
        let new_encoded = working.encoder().bytes();
        if new_encoded.len() == jpeg_len {
            encoded = new_encoded;
            break;
        }
        encoded = new_encoded;
    }

    let jpeg_len = encoded.len();
    let mut out = Vec::with_capacity(jpeg_len + video_bytes.len());
    out.extend_from_slice(&encoded);
    out.extend_from_slice(&video_bytes);

    fs::write(&args.out, &out)
        .with_context(|| format!("Failed to write {}", args.out.display()))?;
    println!(
        "Wrote Motion Photo {} (JPEG {} bytes, video {} bytes, offset {})",
        args.out.display(),
        jpeg_len,
        video_bytes.len(),
        jpeg_len
    );
    Ok(())
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
    jpeg_len: usize,
    video_len: usize,
    presentation_timestamp_us: u64,
}

fn build_motion_xmp(
    existing: Option<&[u8]>,
    jpeg_len: usize,
    video_len: usize,
    presentation_timestamp_us: u64,
) -> Result<Vec<u8>> {
    let meta = MotionMeta {
        jpeg_len,
        video_len,
        presentation_timestamp_us,
    };
    if let Some(existing) = existing {
        if let Ok(merged) = merge_into_existing_xmp(existing, &meta) {
            return Ok(merged);
        }
    }
    Ok(build_fresh_xmp(&meta))
}

fn merge_into_existing_xmp(existing: &[u8], meta: &MotionMeta) -> Result<Vec<u8>> {
    let mut reader = Reader::from_reader(existing);
    reader.config_mut().trim_text(false);
    let mut writer = Writer::new(Vec::with_capacity(existing.len() + 512));
    let mut buf = Vec::new();
    let mut injected = false;
    let mut found_directory = false;
    let mut in_directory = false;
    let mut current_dir_has_motion_item = false;
    let mut inserted_motion_item = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) if e.name().as_ref() == b"Container:Directory" => {
                in_directory = true;
                current_dir_has_motion_item = false;
                found_directory = true;
                writer.write_event(Event::Start(e.to_owned()))?;
            }
            Ok(Event::Empty(ref e)) if in_directory && e.name().as_ref() == b"Container:Item" => {
                if has_motion_semantic(e)? {
                    current_dir_has_motion_item = true;
                    inserted_motion_item = true;
                }
                writer.write_event(Event::Empty(e.to_owned()))?;
            }
            Ok(Event::End(ref e))
                if in_directory && e.name().as_ref() == b"Container:Directory" =>
            {
                if !inserted_motion_item && !current_dir_has_motion_item {
                    write_container_item(&mut writer, "video/mp4", "MotionPhoto", meta.video_len)?;
                    inserted_motion_item = true;
                }
                writer.write_event(Event::End(e.to_owned()))?;
                in_directory = false;
            }
            Ok(Event::End(e)) if e.name().as_ref() == b"rdf:RDF" => {
                if !injected {
                    let include_container = !found_directory;
                    write_motion_description(&mut writer, meta, include_container)?;
                    injected = true;
                }
                writer.write_event(Event::End(e))?;
            }
            Ok(Event::Eof) => break,
            Ok(ev) => writer.write_event(ev)?,
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

    write_container_item(writer, "image/jpeg", "Primary", meta.jpeg_len)?;
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

fn has_motion_semantic(e: &BytesStart<'_>) -> Result<bool> {
    for attr in e.attributes() {
        let attr = attr?;
        if attr.key.as_ref() == b"Item:Semantic" && attr.value.as_ref() == b"MotionPhoto" {
            return Ok(true);
        }
    }
    Ok(false)
}
