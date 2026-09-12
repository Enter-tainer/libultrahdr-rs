use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use ultrahdr::{
    Codec, ColorAspects, ColorGamut, ColorRange, ColorTransfer, CompressedImage, Decoder, Encoder,
    ImageLabel, PixelFormat, Preset, RawImage,
};

#[derive(Debug, Parser)]
#[command(about = "Rust port of ultrahdr_app: encode/decode UltraHDR streams")]
struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Encode UltraHDR from HDR raw + SDR compressed/raw
    Encode {
        /// HDR raw image path
        #[arg(long)]
        hdr_raw: PathBuf,
        /// HDR format (rgba1010102 or rgba8888)
        #[arg(long, value_enum, default_value = "rgba1010102")]
        hdr_fmt: RawFmt,
        /// SDR JPEG path (base image)
        #[arg(long)]
        sdr_jpeg: PathBuf,
        /// Output UltraHDR JPEG
        #[arg(long)]
        out: PathBuf,
        /// Width in pixels
        #[arg(long)]
        width: u32,
        /// Height in pixels
        #[arg(long)]
        height: u32,
        /// Base JPEG quality
        #[arg(long, default_value_t = 95, value_parser = clap::value_parser!(u8).range(1..=100))]
        base_q: u8,
        /// Gain map JPEG quality
        #[arg(long, default_value_t = 95, value_parser = clap::value_parser!(u8).range(1..=100))]
        gm_q: u8,
        /// Gain map downscale factor
        #[arg(long, default_value_t = 1)]
        scale: i32,
        /// Enable multichannel gain map
        #[arg(long, default_value_t = false)]
        mc: bool,
    },
    /// Decode UltraHDR to raw RGB
    Decode {
        /// UltraHDR JPEG path
        #[arg(long)]
        uhdr: PathBuf,
        /// Output raw file
        #[arg(long)]
        out_raw: PathBuf,
        /// Output format
        #[arg(long, value_enum, default_value = "rgba1010102")]
        fmt: RawFmt,
        /// Output transfer
        #[arg(long, value_enum, default_value = "pq")]
        transfer: Transfer,
    },
}

#[derive(Debug)]
struct EncodeArgs {
    hdr_raw: PathBuf,
    hdr_fmt: RawFmt,
    sdr_jpeg: PathBuf,
    out: PathBuf,
    width: u32,
    height: u32,
    base_q: u8,
    gm_q: u8,
    scale: i32,
    mc: bool,
}

#[derive(Debug, Clone, ValueEnum)]
enum RawFmt {
    Rgba8888,
    Rgba1010102,
    RgbaF16,
}

impl RawFmt {
    fn to_pixel_format(&self) -> PixelFormat {
        match self {
            RawFmt::Rgba8888 => PixelFormat::Rgba8888,
            RawFmt::Rgba1010102 => PixelFormat::Rgba1010102,
            RawFmt::RgbaF16 => PixelFormat::RgbaHalfFloat,
        }
    }
}

#[derive(Debug, Clone, ValueEnum)]
enum Transfer {
    Pq,
    Hlg,
    Srgb,
}

impl Transfer {
    fn to_color_transfer(&self) -> ColorTransfer {
        match self {
            Transfer::Pq => ColorTransfer::Pq,
            Transfer::Hlg => ColorTransfer::Hlg,
            Transfer::Srgb => ColorTransfer::Srgb,
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Command::Encode {
            hdr_raw,
            hdr_fmt,
            sdr_jpeg,
            out,
            width,
            height,
            base_q,
            gm_q,
            scale,
            mc,
        } => encode(EncodeArgs {
            hdr_raw,
            hdr_fmt,
            sdr_jpeg,
            out,
            width,
            height,
            base_q,
            gm_q,
            scale,
            mc,
        }),
        Command::Decode {
            uhdr,
            out_raw,
            fmt,
            transfer,
        } => decode(uhdr, out_raw, fmt, transfer),
    }
}

fn encode(args: EncodeArgs) -> Result<()> {
    let EncodeArgs {
        hdr_raw: hdr_raw_path,
        hdr_fmt,
        sdr_jpeg: sdr_jpeg_path,
        out: out_path,
        width,
        height,
        base_q,
        gm_q,
        scale,
        mc,
    } = args;

    let hdr_bytes = fs::read(&hdr_raw_path)
        .with_context(|| format!("Failed to read HDR raw {}", hdr_raw_path.display()))?;
    let sdr_bytes = fs::read(&sdr_jpeg_path)
        .with_context(|| format!("Failed to read SDR JPEG {}", sdr_jpeg_path.display()))?;

    let aspects = ColorAspects::new(ColorGamut::DisplayP3, ColorTransfer::Pq, ColorRange::Full);
    let hdr_raw =
        RawImage::from_packed(hdr_fmt.to_pixel_format(), width, height, hdr_bytes, aspects)?;

    let mut enc = Encoder::new()?;
    enc.set_raw_image(ImageLabel::Hdr, &hdr_raw)?;
    enc.set_compressed_image(
        ImageLabel::Sdr,
        &CompressedImage::with_aspects(
            sdr_bytes,
            ColorAspects::new(ColorGamut::DisplayP3, ColorTransfer::Srgb, ColorRange::Full),
        ),
    )?;

    enc.set_quality(ImageLabel::Base, base_q)?;
    enc.set_quality(ImageLabel::GainMap, gm_q)?;
    enc.set_gainmap_scale_factor(scale)?;
    enc.set_multi_channel_gainmap(mc)?;
    enc.set_gainmap_gamma(1.0)?;
    enc.set_target_display_peak_brightness(10000.0)?;
    enc.set_output_format(Codec::Jpeg)?;
    enc.set_preset(Preset::BestQuality)?;
    enc.encode()?;

    let out_img = enc
        .encoded_stream()
        .context("Encode returned null output")?;
    fs::write(&out_path, out_img.bytes())
        .with_context(|| format!("Failed to write output {}", out_path.display()))?;
    println!("Wrote {}", out_path.display());
    Ok(())
}

fn decode(
    uhdr_path: PathBuf,
    out_raw_path: PathBuf,
    fmt: RawFmt,
    transfer: Transfer,
) -> Result<()> {
    let uhdr_bytes =
        fs::read(&uhdr_path).with_context(|| format!("Failed to read {}", uhdr_path.display()))?;
    let mut dec = Decoder::new()?;
    dec.set_image(&CompressedImage::new(uhdr_bytes))?;

    let img_fmt = fmt.to_pixel_format();
    let color_transfer = transfer.to_color_transfer();
    let decoded = dec.decode_as(img_fmt, color_transfer)?;
    let mut file = File::create(&out_raw_path)
        .with_context(|| format!("Failed to write {}", out_raw_path.display()))?;
    for row in decoded.rows() {
        file.write_all(row?)
            .with_context(|| format!("Failed to write {}", out_raw_path.display()))?;
    }
    println!(
        "Decoded {} -> {} ({}x{}, {:?} {:?})",
        uhdr_path.display(),
        out_raw_path.display(),
        decoded.width(),
        decoded.height(),
        img_fmt,
        color_transfer
    );
    Ok(())
}
