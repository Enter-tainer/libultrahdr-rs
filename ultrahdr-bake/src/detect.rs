use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail, ensure};
use memchr::memmem;
use ultrahdr::{CompressedImage, Decoder, Error, GainMapMetadata, sys};

// Tunable knobs for XMP scanning. Bump these if your XMP lives deeper in the file.
const XMP_SCAN_LIMIT_BYTES: usize = 256 * 1024;
const XMP_CHUNK_SIZE: usize = 8192;
const XMP_EXTRA_TAIL_CHUNK: usize = 4096;
const XMP_EXTRA_TAIL_READS: usize = 4;

#[derive(Debug, Clone, Copy)]
pub enum HdrDetection {
    ProbeGainMapMetadata,
}

impl HdrDetection {
    pub fn as_str(&self) -> &'static str {
        "libuhdr probe found gain map metadata"
    }
}

#[derive(Debug)]
pub struct InputPair {
    pub hdr: PathBuf,
    pub sdr: PathBuf,
}

pub fn resolve_inputs(args: &crate::cli::BakeArgs) -> Result<InputPair> {
    if args.hdr.is_some() || args.sdr.is_some() {
        ensure!(
            args.hdr.is_some() && args.sdr.is_some(),
            "Provide both --hdr and --sdr together (or omit both to auto-detect)"
        );
        return Ok(InputPair {
            hdr: args.hdr.clone().expect("hdr is_some checked"),
            sdr: args.sdr.clone().expect("sdr is_some checked"),
        });
    }

    if args.inputs.len() == 1 {
        return resolve_by_original_id(&args.inputs[0]);
    }

    ensure!(
        args.inputs.len() == 2,
        "Provide --hdr and --sdr, or 1-2 positional JPEGs for auto-detection"
    );
    auto_detect_pair(&args.inputs[0], &args.inputs[1])
}

fn resolve_by_original_id(seed: &Path) -> Result<InputPair> {
    let seed_doc_id = original_document_id(seed)?.ok_or_else(|| {
        anyhow::anyhow!(
            "Input {} missing XMP OriginalDocumentID; cannot find pair",
            seed.display()
        )
    })?;
    let ext = seed
        .extension()
        .and_then(|e| e.to_str())
        .ok_or_else(|| anyhow::anyhow!("Input {} has no extension", seed.display()))?;
    let dir = seed
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    let mut matches = Vec::new();
    for entry in
        fs::read_dir(&dir).with_context(|| format!("Failed to list directory {}", dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path == seed || !path.is_file() {
            continue;
        }
        let same_ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case(ext))
            .unwrap_or(false);
        if !same_ext {
            continue;
        }
        if let Some(doc_id) = original_document_id(&path)?
            && doc_id == seed_doc_id
        {
            matches.push(path);
        }
    }

    match matches.len() {
        0 => bail!(
            "No sibling with matching OriginalDocumentID ({}) found for {}",
            seed_doc_id,
            seed.display()
        ),
        1 => {
            let mate = matches.pop().expect("len==1");
            println!(
                "Found matching OriginalDocumentID ({}) between {} and {}",
                seed_doc_id,
                seed.display(),
                mate.display()
            );
            auto_detect_pair(seed, &mate)
        }
        _ => {
            let mut siblings: Vec<String> = matches
                .into_iter()
                .map(|p| p.display().to_string())
                .collect();
            siblings.sort();
            siblings.dedup();
            bail!(
                "Multiple siblings share OriginalDocumentID ({}) with {}:\n{}\nPlease specify --hdr/--sdr explicitly.",
                seed_doc_id,
                seed.display(),
                siblings.join("\n")
            )
        }
    }
}

fn auto_detect_pair(a: &Path, b: &Path) -> Result<InputPair> {
    let a_det = detect_hdr_candidate(a)?;
    let b_det = detect_hdr_candidate(b)?;

    match (a_det, b_det) {
        (Some(reason), None) => {
            println!(
                "Auto-detected HDR input: {} ({})",
                a.display(),
                reason.as_str()
            );
            Ok(InputPair {
                hdr: a.to_path_buf(),
                sdr: b.to_path_buf(),
            })
        }
        (None, Some(reason)) => {
            println!(
                "Auto-detected HDR input: {} ({})",
                b.display(),
                reason.as_str()
            );
            Ok(InputPair {
                hdr: b.to_path_buf(),
                sdr: a.to_path_buf(),
            })
        }
        (Some(_), Some(_)) => bail!(
            "Both inputs look like UltraHDR (ISO 21496 gain map metadata). Please specify --hdr and --sdr explicitly."
        ),
        (None, None) => bail!(
            "Could not find ISO 21496 gain map metadata in either input. Specify --hdr and --sdr explicitly."
        ),
    }
}

fn detect_hdr_candidate(path: &Path) -> Result<Option<HdrDetection>> {
    let mut bytes =
        fs::read(path).with_context(|| format!("Failed to read input {}", path.display()))?;
    let meta = probe_gainmap_metadata(&mut bytes)?;
    Ok(meta.map(|_| HdrDetection::ProbeGainMapMetadata))
}

pub fn probe_gainmap_metadata(buf: &mut [u8]) -> Result<Option<GainMapMetadata>> {
    let mut dec = Decoder::new()?;
    let mut comp = CompressedImage::from_bytes(
        buf,
        sys::uhdr_color_gamut::UHDR_CG_UNSPECIFIED,
        sys::uhdr_color_transfer::UHDR_CT_UNSPECIFIED,
        sys::uhdr_color_range::UHDR_CR_UNSPECIFIED,
    );
    dec.set_image(&mut comp)?;
    match dec.gainmap_metadata() {
        Ok(meta) => Ok(meta),
        Err(Error {
            code: sys::uhdr_codec_err_t::UHDR_CODEC_INVALID_PARAM,
            ..
        }) => {
            // Not an UltraHDR/gain map JPEG.
            Ok(None)
        }
        Err(e) => Err(e.into()),
    }
}

fn original_document_id(path: &Path) -> Result<Option<String>> {
    let file = fs::File::open(path).with_context(|| {
        format!(
            "Failed to read {} for OriginalDocumentID scan",
            path.display()
        )
    })?;
    let marker = b"OriginalDocumentID";
    let step = XMP_CHUNK_SIZE;
    let mut offset: usize = 0;
    let mut buf = vec![0u8; step + marker.len()];

    while offset < XMP_SCAN_LIMIT_BYTES {
        let mut f = file.try_clone()?;
        f.seek(SeekFrom::Start(offset as u64))?;
        let to_read = buf.len().min(XMP_SCAN_LIMIT_BYTES.saturating_sub(offset));
        let n = read_exact_allow_short(&mut f, &mut buf[..to_read])?;
        if n == 0 {
            break;
        }
        let slice = &buf[..n];
        if let Some(pos) = memmem::find(slice, marker) {
            let tail_start = offset + pos + marker.len();
            let mut tail = vec![0u8; XMP_EXTRA_TAIL_CHUNK * XMP_EXTRA_TAIL_READS];
            let mut tail_file = file.try_clone()?;
            tail_file.seek(SeekFrom::Start(tail_start as u64))?;
            let m = read_exact_allow_short(&mut tail_file, &mut tail)?;
            return Ok(extract_original_document_id(&tail[..m]));
        }
        offset = offset.saturating_add(step);
    }

    Ok(None)
}

fn extract_original_document_id(after_marker: &[u8]) -> Option<String> {
    // Try element form: <xmpMM:OriginalDocumentID>VALUE</xmpMM:OriginalDocumentID>
    if let Some(gt_pos) = after_marker.iter().position(|&b| b == b'>') {
        let value_start = gt_pos + 1;
        if let Some(end_pos) = after_marker[value_start..].iter().position(|&b| b == b'<') {
            let raw = &after_marker[value_start..value_start + end_pos];
            if let Ok(s) = String::from_utf8(raw.to_vec()) {
                let trimmed = s.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
    }

    // Fallback: attribute form OriginalDocumentID="VALUE"
    if let Some(eq_pos) = after_marker.iter().position(|&b| b == b'=') {
        let rest = &after_marker[eq_pos + 1..];
        if let Some(first_quote) = rest.iter().position(|&b| b == b'"') {
            let rest_after_quote = &rest[first_quote + 1..];
            if let Some(second_quote) = rest_after_quote.iter().position(|&b| b == b'"') {
                let raw = &rest_after_quote[..second_quote];
                if let Ok(s) = String::from_utf8(raw.to_vec()) {
                    let trimmed = s.trim();
                    if !trimmed.is_empty() {
                        return Some(trimmed.to_string());
                    }
                }
            }
        }
    }

    None
}

fn read_exact_allow_short<R: Read>(r: &mut R, buf: &mut [u8]) -> Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match r.read(&mut buf[filled..])? {
            0 => break,
            n => filled += n,
        }
    }
    Ok(filled)
}
