use std::{fs, path::Path};

use anyhow::{Context, Result, ensure};
use ultrahdr::{
    ColorAspects, ColorGamut, ColorRange, ColorTransfer, CompressedImage, DecodedOutput, Decoder,
    Encoder, ImageLabel, Preset,
};

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

    if let Some(gamut) = hdr_icc_gamut {
        println!("HDR ICC gamut: {}", gamut_label(gamut));
    }
    if let Some(gamut) = sdr_icc_gamut {
        println!("SDR ICC gamut: {}", gamut_label(gamut));
    }

    // Decode the HDR intent from the UltraHDR JPEG into PQ RGBA1010102 pixels.
    let mut dec = Decoder::new()?;
    dec.set_image(&CompressedImage::with_aspects(
        hdr_bytes.as_slice(),
        ColorAspects::UNSPECIFIED.with_gamut(hdr_icc_gamut.unwrap_or(ColorGamut::DisplayP3)),
    ))?;
    let mut dec = dec.probe_as(DecodedOutput::Pq1010102)?;
    let mut hdr_view = dec.decode()?;

    // Fill in anything the stream did not signal; the encoder needs complete aspects for raw input.
    let mut aspects = hdr_view.aspects();
    aspects.gamut = Some(
        hdr_icc_gamut
            .or(aspects.gamut)
            .unwrap_or(ColorGamut::DisplayP3),
    );
    aspects.transfer = Some(aspects.transfer.unwrap_or(ColorTransfer::Pq));
    aspects.range = Some(ColorRange::Full);
    hdr_view.set_aspects(aspects);

    // Encode with the provided SDR base JPEG.
    let mut enc = Encoder::new()?;
    enc.set_decoded_image(ImageLabel::Hdr, &hdr_view)?;

    let sdr_aspects = ColorAspects::new(
        sdr_icc_gamut.unwrap_or(ColorGamut::DisplayP3),
        ColorTransfer::Srgb,
        ColorRange::Full,
    );
    enc.set_compressed_image(
        ImageLabel::Sdr,
        &CompressedImage::with_aspects(sdr_bytes.as_slice(), sdr_aspects),
    )?;

    enc.set_quality(ImageLabel::Base, args.base_quality)?;
    enc.set_quality(ImageLabel::GainMap, args.gainmap_quality)?;
    enc.set_gainmap_scale_factor(args.gainmap_scale)?;
    enc.set_multi_channel_gainmap(args.multichannel_gainmap)?;
    enc.set_gainmap_gamma(1.0)?;
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
    enc.set_target_display_peak_brightness(target_peak)?;
    enc.set_preset(Preset::BestQuality)?;
    enc.encode()?;

    let out_view = enc
        .encoded_stream()
        .context("Encode returned null output")?;
    fs::write(out_path, out_view.bytes())
        .with_context(|| format!("Failed to write output {}", out_path.display()))?;

    println!("Wrote {}", out_path.display());
    Ok(())
}
