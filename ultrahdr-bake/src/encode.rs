use std::fs;

use anyhow::{ensure, Context, Result};
use ultrahdr::{sys, CompressedImage, Decoder, Encoder, ImgLabel};

use crate::color::{detect_icc_color_gamut, gamut_label};
use crate::detect::probe_gainmap_metadata;

pub fn run_encoding(args: &crate::cli::Cli, inputs: &crate::detect::InputPair) -> Result<()> {
    if let Some(target_peak) = args.target_peak_nits.as_ref() {
        ensure!(
            *target_peak > 0.0,
            "Target peak brightness must be greater than zero nits"
        );
    }

    let mut hdr_bytes = fs::read(&inputs.hdr)
        .with_context(|| format!("Failed to read HDR UltraHDR file {}", inputs.hdr.display()))?;
    let mut sdr_bytes = fs::read(&inputs.sdr)
        .with_context(|| format!("Failed to read SDR JPEG file {}", inputs.sdr.display()))?;
    let hdr_icc_gamut = detect_icc_color_gamut(&hdr_bytes);
    let sdr_icc_gamut = detect_icc_color_gamut(&sdr_bytes);
    let gainmap_meta = probe_gainmap_metadata(&mut hdr_bytes)?;

    if let Some(cg) = hdr_icc_gamut {
        println!("HDR ICC gamut: {}", gamut_label(cg));
    }
    if let Some(cg) = sdr_icc_gamut {
        println!("SDR ICC gamut: {}", gamut_label(cg));
    }

    // Decode HDR intent from UltraHDR JPEG.
    let mut dec = Decoder::new()?;
    let mut hdr_comp = CompressedImage::from_bytes(
        &mut hdr_bytes,
        hdr_icc_gamut.unwrap_or(sys::uhdr_color_gamut::UHDR_CG_UNSPECIFIED),
        sys::uhdr_color_transfer::UHDR_CT_UNSPECIFIED,
        sys::uhdr_color_range::UHDR_CR_UNSPECIFIED,
    );
    dec.set_image(&mut hdr_comp)?;
    let mut hdr_view = dec.decode_packed_view(
        sys::uhdr_img_fmt::UHDR_IMG_FMT_32bppRGBA1010102,
        sys::uhdr_color_transfer::UHDR_CT_PQ,
    )?;
    if hdr_view.meta().0 == sys::uhdr_color_gamut::UHDR_CG_UNSPECIFIED {
        hdr_view
            .set_color_gamut(hdr_icc_gamut.unwrap_or(sys::uhdr_color_gamut::UHDR_CG_DISPLAY_P3));
    }
    if hdr_view.meta().1 == sys::uhdr_color_transfer::UHDR_CT_UNSPECIFIED {
        hdr_view.set_color_transfer(sys::uhdr_color_transfer::UHDR_CT_PQ);
    }
    hdr_view.set_color_range(sys::uhdr_color_range::UHDR_CR_FULL_RANGE);

    // Encode with provided SDR base JPEG.
    let mut enc = Encoder::new()?;
    enc.set_raw_image_view(&mut hdr_view, ImgLabel::UHDR_HDR_IMG)?;

    let mut sdr_comp = CompressedImage::from_bytes(
        &mut sdr_bytes,
        sdr_icc_gamut.unwrap_or(sys::uhdr_color_gamut::UHDR_CG_DISPLAY_P3),
        sys::uhdr_color_transfer::UHDR_CT_SRGB,
        sys::uhdr_color_range::UHDR_CR_FULL_RANGE,
    );
    enc.set_compressed_image(&mut sdr_comp, ImgLabel::UHDR_SDR_IMG)?;

    enc.set_quality(args.base_quality, ImgLabel::UHDR_BASE_IMG)?;
    enc.set_quality(args.gainmap_quality, ImgLabel::UHDR_GAIN_MAP_IMG)?;
    enc.set_gainmap_scale_factor(args.gainmap_scale)?;
    enc.set_using_multi_channel_gainmap(args.multichannel_gainmap)?;
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
    enc.set_output_format(sys::uhdr_codec::UHDR_CODEC_JPG)?;
    enc.set_preset(sys::uhdr_enc_preset::UHDR_USAGE_BEST_QUALITY)?;
    enc.encode()?;

    let out_view = enc
        .encoded_stream()
        .context("Encode returned null output")?;
    let out_bytes = out_view.bytes()?;
    fs::write(&args.out, out_bytes)
        .with_context(|| format!("Failed to write output {}", args.out.display()))?;

    println!("Wrote {}", args.out.display());
    Ok(())
}
