use std::{fs, path::Path};

use anyhow::{Context, Result, ensure};
#[cfg(feature = "heif")]
use ultrahdr::RawImage;
use ultrahdr::{
    ColorAspects, ColorGamut, ColorRange, ColorTransfer, CompressedImage, Decoder, Encoder,
    ImageLabel, PixelFormat, Preset,
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

    // A wasm build bundles the AV1 codec only (HEVC has no WASI port), so there is no HEVC encoder
    // to write a HEIC/HEIF container with. Fail with the reason instead of letting libheif report
    // its opaque "Unsupported file-type".
    #[cfg(target_arch = "wasm32")]
    if args.format == crate::cli::OutputFormat::Heif {
        anyhow::bail!(
            "--format heif needs an HEVC encoder, and wasm builds bundle the AV1 codec only \
             (x265/libde265 have no WASI port); use --format avif or --format jpeg"
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
    let mut hdr_view = dec.decode_as(PixelFormat::Rgba1010102, ColorTransfer::Pq)?;

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
    if args.format == crate::cli::OutputFormat::Jpeg {
        enc.set_compressed_image(
            ImageLabel::Sdr,
            &CompressedImage::with_aspects(sdr_bytes.as_slice(), sdr_aspects),
        )?;
    } else if cfg!(feature = "heif") {
        // A HEIF/AVIF file carries the base image inside its own container, so upstream rejects a
        // compressed base for those formats ("heif/avif encoding is supported only with raw
        // intents") and the SDR intent has to be decoded to raw pixels first.
        #[cfg(feature = "heif")]
        {
            let sdr = decode_sdr_to_rgba(sdr_bytes.as_slice(), sdr_aspects)?;
            enc.set_raw_image(ImageLabel::Sdr, &sdr)?;
        }
    } else {
        anyhow::bail!(
            "--format {} needs a build with the `heif` feature",
            args.format.extension()
        );
    }

    enc.set_quality(ImageLabel::Base, args.base_quality)?;
    enc.set_quality(ImageLabel::GainMap, args.gainmap_quality)?;
    enc.set_gainmap_scale_factor(args.gainmap_scale)?;
    // Upstream overruns the gain map buffer when a HEIF/AVIF stream carries a multi-channel gain
    // map with an odd height (the safe wrapper rejects that combination), so fall back to a
    // single-channel gain map instead of failing the whole bake.
    let mut multichannel_gainmap = args.multichannel_gainmap;
    if args.format != crate::cli::OutputFormat::Jpeg && multichannel_gainmap {
        let map_height = hdr_view.height() / args.gainmap_scale.max(1) as u32;
        if map_height % 2 == 1 {
            eprintln!(
                "Warning: libultrahdr cannot write a {map_height}-row multi-channel gain map into \
                 the {} format; using a single-channel gain map instead",
                args.format.extension()
            );
            multichannel_gainmap = false;
        }
    }
    enc.set_multi_channel_gainmap(multichannel_gainmap)?;
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
    enc.set_output_format(args.format.codec())?;
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

/// Decode the SDR base JPEG to packed `Rgba8888` pixels.
///
/// Only used for HEIF/AVIF output, which upstream encodes from raw intents; `Rgba8888` is the SDR
/// format it pairs with the `Rgba1010102` HDR intent. libultrahdr's own decoder only accepts JPEGs
/// that carry a gain map, so a plain SDR JPEG is decoded here instead.
#[cfg(feature = "heif")]
fn decode_sdr_to_rgba(bytes: &[u8], aspects: ColorAspects) -> Result<RawImage> {
    use zune_jpeg::{
        JpegDecoder,
        zune_core::{bytestream::ZCursor, colorspace::ColorSpace, options::DecoderOptions},
    };

    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGBA);
    let mut decoder = JpegDecoder::new_with_options(ZCursor::new(bytes), options);
    let pixels = decoder
        .decode()
        .map_err(|error| anyhow::anyhow!("failed to decode the SDR JPEG: {error}"))?;
    let info = decoder.info().context("the SDR JPEG has no frame header")?;
    let (width, height) = (u32::from(info.width), u32::from(info.height));
    let expected = width as usize * height as usize * 4;
    ensure!(
        pixels.len() >= expected,
        "the SDR JPEG decoded to {} bytes, expected at least {expected}",
        pixels.len()
    );
    RawImage::from_packed(PixelFormat::Rgba8888, width, height, pixels, aspects).map_err(Into::into)
}
