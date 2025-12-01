use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{bail, ensure, Context, Result};
use clap::{builder::ValueHint, Parser};
use ultrahdr::{sys, CompressedImage, Decoder, Encoder, Error, GainMapMetadata, ImgLabel};

#[derive(Parser, Debug)]
#[command(
    name = "ultrahdr-bake",
    about = "Bake an UltraHDR JPEG from an HDR gain map JPEG and an SDR base JPEG.",
    author,
    version,
    arg_required_else_help = true
)]
struct Cli {
    /// Two JPEGs; autodetect which is HDR (ISO 21496 gain map) vs SDR
    #[arg(value_name = "FILE", value_hint = ValueHint::FilePath, num_args = 0..=2)]
    inputs: Vec<PathBuf>,

    /// UltraHDR JPEG containing the HDR intent and gain map
    #[arg(long, value_hint = ValueHint::FilePath, value_name = "FILE")]
    hdr: Option<PathBuf>,

    /// SDR base JPEG to embed into the UltraHDR output
    #[arg(long, short = 's', value_hint = ValueHint::FilePath, value_name = "FILE")]
    sdr: Option<PathBuf>,

    /// Output UltraHDR JPEG path
    #[arg(long, short = 'o', value_hint = ValueHint::FilePath, value_name = "FILE", default_value = "ultrahdr_bake_out.jpg")]
    out: PathBuf,

    /// JPEG quality for the SDR base image (1-100)
    #[arg(long = "base-q", default_value_t = 95, value_parser = clap::value_parser!(i32).range(1..=100))]
    base_quality: i32,

    /// JPEG quality for the gain map (1-100)
    #[arg(long = "gm-q", alias = "gainmap-q", default_value_t = 95, value_parser = clap::value_parser!(i32).range(1..=100))]
    gainmap_quality: i32,

    /// Gain map scale factor
    #[arg(long = "scale", default_value_t = 1, value_parser = clap::value_parser!(i32).range(1..))]
    gainmap_scale: i32,

    /// Use multi-channel gain maps (--mc works too)
    #[arg(long = "multichannel", short = 'm', alias = "mc")]
    multichannel_gainmap: bool,

    /// Override target peak brightness in nits (falls back to metadata or 1600 nits)
    #[arg(long = "target-peak", value_name = "NITS")]
    target_peak_nits: Option<f32>,
}

fn main() -> Result<()> {
    let args = Cli::parse();
    run(args)
}

fn run(args: Cli) -> Result<()> {
    ensure!(
        args.inputs.is_empty() || (args.hdr.is_none() && args.sdr.is_none()),
        "Provide either two positional JPEGs for auto-detection or --hdr/--sdr, not both"
    );

    if let Some(target_peak) = args.target_peak_nits.as_ref() {
        ensure!(
            *target_peak > 0.0,
            "Target peak brightness must be greater than zero nits"
        );
    }

    let inputs = resolve_inputs(&args)?;

    let mut hdr_bytes = fs::read(&inputs.hdr)
        .with_context(|| format!("Failed to read HDR UltraHDR file {}", inputs.hdr.display()))?;
    let mut sdr_bytes = fs::read(&inputs.sdr)
        .with_context(|| format!("Failed to read SDR JPEG file {}", inputs.sdr.display()))?;
    let gainmap_meta = probe_gainmap_metadata(&mut hdr_bytes)?;

    // Decode HDR intent from UltraHDR JPEG.
    let mut dec = Decoder::new()?;
    let mut hdr_comp = CompressedImage::from_bytes(
        &mut hdr_bytes,
        sys::uhdr_color_gamut::UHDR_CG_UNSPECIFIED,
        sys::uhdr_color_transfer::UHDR_CT_UNSPECIFIED,
        sys::uhdr_color_range::UHDR_CR_UNSPECIFIED,
    );
    dec.set_image(&mut hdr_comp)?;
    let mut hdr_view = dec.decode_packed_view(
        sys::uhdr_img_fmt::UHDR_IMG_FMT_32bppRGBA1010102,
        sys::uhdr_color_transfer::UHDR_CT_PQ,
    )?;
    if hdr_view.meta().0 == sys::uhdr_color_gamut::UHDR_CG_UNSPECIFIED {
        hdr_view.set_color_gamut(sys::uhdr_color_gamut::UHDR_CG_DISPLAY_P3);
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
        sys::uhdr_color_gamut::UHDR_CG_DISPLAY_P3,
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

#[derive(Debug)]
struct InputPair {
    hdr: PathBuf,
    sdr: PathBuf,
}

#[derive(Debug, Clone, Copy)]
enum HdrDetection {
    ProbeGainMapMetadata,
}

impl HdrDetection {
    fn as_str(&self) -> &'static str {
        "libuhdr probe found gain map metadata"
    }
}

fn resolve_inputs(args: &Cli) -> Result<InputPair> {
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

    ensure!(
        args.inputs.len() == 2,
        "Provide --hdr and --sdr, or exactly two positional JPEGs for auto-detection"
    );
    let a = &args.inputs[0];
    let b = &args.inputs[1];
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
                hdr: a.clone(),
                sdr: b.clone(),
            })
        }
        (None, Some(reason)) => {
            println!(
                "Auto-detected HDR input: {} ({})",
                b.display(),
                reason.as_str()
            );
            Ok(InputPair {
                hdr: b.clone(),
                sdr: a.clone(),
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

fn probe_gainmap_metadata(buf: &mut [u8]) -> Result<Option<GainMapMetadata>> {
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
        Err(e)
            if matches!(
                e,
                Error {
                    code: sys::uhdr_codec_err_t::UHDR_CODEC_INVALID_PARAM,
                    ..
                }
            ) =>
        {
            // Not an UltraHDR/gain map JPEG.
            Ok(None)
        }
        Err(e) => Err(e.into()),
    }
}
