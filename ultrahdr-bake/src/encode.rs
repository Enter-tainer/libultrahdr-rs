use std::{fs, path::Path};

use anyhow::{Context, Result, ensure};
use ultrahdr::{ColorGamut, ColorTransfer, PixelFormat};

use crate::color::{detect_icc_color_gamut, gamut_label};
use crate::detect::probe_gainmap_metadata;

pub fn run_encoding(
    args: &crate::cli::BakeArgs,
    inputs: &crate::detect::InputPair,
    out_path: &Path,
) -> Result<()> {
    if let Some(target_peak) = args.target_peak_nits.as_ref() {
        ensure!(
            *target_peak > 0.0,
            "Target peak brightness must be greater than zero nits"
        );
    }

    let hdr_bytes = fs::read(&inputs.hdr)
        .with_context(|| format!("Failed to read HDR UltraHDR file {}", inputs.hdr.display()))?;
    let sdr_bytes = fs::read(&inputs.sdr)
        .with_context(|| format!("Failed to read SDR JPEG file {}", inputs.sdr.display()))?;
    let hdr_icc_gamut = detect_icc_color_gamut(&hdr_bytes);
    let sdr_icc_gamut = detect_icc_color_gamut(&sdr_bytes);
    let gainmap_meta = probe_gainmap_metadata(&hdr_bytes)?;

    if let Some(cg) = hdr_icc_gamut {
        println!("HDR ICC gamut: {}", gamut_label(cg));
    }
    if let Some(cg) = sdr_icc_gamut {
        println!("SDR ICC gamut: {}", gamut_label(cg));
    }

    // Decode HDR intent from UltraHDR JPEG.
    let hdr_gamut = hdr_icc_gamut.unwrap_or(ColorGamut::DisplayP3);
    let decoded = ultrahdr::decoder::Decoder::new(&hdr_bytes)
        .output_format(PixelFormat::Rgba1010102)
        .output_transfer(ColorTransfer::Pq)
        .decode()
        .context("Failed to decode HDR UltraHDR JPEG")?;

    // Encode with provided SDR base JPEG.
    let sdr_gamut = sdr_icc_gamut.unwrap_or(ColorGamut::DisplayP3);

    let target_peak = args
        .target_peak_nits
        .or_else(|| gainmap_meta.as_ref().map(|m| m.target_display_peak_nits()))
        .unwrap_or(1600.0);
    if let Some(meta) = &gainmap_meta {
        println!(
            "Source gain map target peak: {:.1} nits (hdr_capacity_max={:.3})",
            meta.target_display_peak_nits(),
            meta.hdr_capacity_max
        );
    }
    println!("Using target peak brightness: {:.1} nits", target_peak);

    let out_bytes = ultrahdr::encoder::Encoder::new()
        .hdr_raw(
            &decoded.data,
            decoded.width,
            decoded.height,
            PixelFormat::Rgba1010102,
            hdr_gamut,
            ColorTransfer::Pq,
        )
        .sdr_compressed(&sdr_bytes, sdr_gamut)
        .quality(args.base_quality as u8)
        .gainmap_quality(args.gainmap_quality as u8)
        .gainmap_scale(args.gainmap_scale as u32)
        .multichannel_gainmap(args.multichannel_gainmap)
        .target_display_peak_nits(target_peak)
        .encode()
        .context("Failed to encode UltraHDR JPEG")?;

    fs::write(out_path, out_bytes)
        .with_context(|| format!("Failed to write output {}", out_path.display()))?;

    println!("Wrote {}", out_path.display());
    Ok(())
}
