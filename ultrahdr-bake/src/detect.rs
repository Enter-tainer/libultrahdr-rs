use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{bail, ensure, Context, Result};
use ultrahdr::{sys, CompressedImage, Decoder, Error, GainMapMetadata};
use xmpkit::{ns, XmpFile};

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

pub fn resolve_inputs(args: &crate::cli::Cli) -> Result<InputPair> {
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
        if let Some(doc_id) = original_document_id(&path)? {
            if doc_id == seed_doc_id {
                matches.push(path);
            }
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
        _ => bail!(
            "Multiple siblings share OriginalDocumentID ({}) with {}. Please specify --hdr/--sdr explicitly.",
            seed_doc_id,
            seed.display()
        ),
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
    let mut file = XmpFile::new();
    file.open(path)
        .with_context(|| format!("Failed to open {} for XMP", path.display()))?;
    let xmp = match file.get_xmp() {
        Some(meta) => meta,
        None => return Ok(None),
    };
    Ok(xmp
        .get_property(ns::XMP_MM, "OriginalDocumentID")
        .and_then(|v| v.as_str().map(str::to_owned)))
}
