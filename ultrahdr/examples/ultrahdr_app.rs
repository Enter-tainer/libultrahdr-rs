use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use ultrahdr::{sys, CompressedImage, Decoder, Encoder, ImgFormat, ImgLabel, RawImage};

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
        #[arg(long, default_value_t = 95)]
        base_q: i32,
        /// Gain map JPEG quality
        #[arg(long, default_value_t = 95)]
        gm_q: i32,
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

#[derive(Debug, Clone, ValueEnum)]
enum RawFmt {
    Rgba8888,
    Rgba1010102,
    RgbaF16,
}

impl RawFmt {
    fn to_img_fmt(&self) -> (ImgFormat, usize) {
        match self {
            RawFmt::Rgba8888 => (sys::uhdr_img_fmt::UHDR_IMG_FMT_32bppRGBA8888, 4),
            RawFmt::Rgba1010102 => (sys::uhdr_img_fmt::UHDR_IMG_FMT_32bppRGBA1010102, 4),
            RawFmt::RgbaF16 => (sys::uhdr_img_fmt::UHDR_IMG_FMT_64bppRGBAHalfFloat, 8),
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
    fn to_ct(&self) -> sys::uhdr_color_transfer {
        match self {
            Transfer::Pq => sys::uhdr_color_transfer::UHDR_CT_PQ,
            Transfer::Hlg => sys::uhdr_color_transfer::UHDR_CT_HLG,
            Transfer::Srgb => sys::uhdr_color_transfer::UHDR_CT_SRGB,
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
        } => encode(hdr_raw, hdr_fmt, sdr_jpeg, out, width, height, base_q, gm_q, scale, mc),
        Command::Decode {
            uhdr,
            out_raw,
            fmt,
            transfer,
        } => decode(uhdr, out_raw, fmt, transfer),
    }
}

fn encode(
    hdr_raw_path: PathBuf,
    hdr_fmt: RawFmt,
    sdr_jpeg_path: PathBuf,
    out_path: PathBuf,
    width: u32,
    height: u32,
    base_q: i32,
    gm_q: i32,
    scale: i32,
    mc: bool,
) -> Result<()> {
    let mut hdr_bytes = fs::read(&hdr_raw_path)
        .with_context(|| format!("Failed to read HDR raw {}", hdr_raw_path.display()))?;
    let mut sdr_bytes = fs::read(&sdr_jpeg_path)
        .with_context(|| format!("Failed to read SDR JPEG {}", sdr_jpeg_path.display()))?;

    let (fmt, bpp) = hdr_fmt.to_img_fmt();
    let mut hdr_raw = RawImage::packed(
        fmt,
        width,
        height,
        bpp,
        &mut hdr_bytes,
        sys::uhdr_color_gamut::UHDR_CG_DISPLAY_P3,
        sys::uhdr_color_transfer::UHDR_CT_PQ,
        sys::uhdr_color_range::UHDR_CR_FULL_RANGE,
    )?;

    let mut enc = Encoder::new()?;
    enc.set_raw_image(&mut hdr_raw, ImgLabel::UHDR_HDR_IMG)?;

    let mut sdr_comp = CompressedImage::from_bytes(
        &mut sdr_bytes,
        sys::uhdr_color_gamut::UHDR_CG_DISPLAY_P3,
        sys::uhdr_color_transfer::UHDR_CT_SRGB,
        sys::uhdr_color_range::UHDR_CR_FULL_RANGE,
    );
    enc.set_compressed_image(&mut sdr_comp, ImgLabel::UHDR_SDR_IMG)?;

    enc.set_quality(base_q, ImgLabel::UHDR_BASE_IMG)?;
    enc.set_quality(gm_q, ImgLabel::UHDR_GAIN_MAP_IMG)?;
    enc.set_gainmap_scale_factor(scale)?;
    enc.set_using_multi_channel_gainmap(mc)?;
    enc.set_gainmap_gamma(1.0)?;
    enc.set_target_display_peak_brightness(10000.0)?;
    enc.set_output_format(sys::uhdr_codec::UHDR_CODEC_JPG)?;
    enc.set_preset(sys::uhdr_enc_preset::UHDR_USAGE_BEST_QUALITY)?;
    enc.encode()?;

    let out_img = enc
        .encoded_stream()
        .context("Encode returned null output")?;
    fs::write(&out_path, out_img.bytes()?)
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
    let mut uhdr_bytes =
        fs::read(&uhdr_path).with_context(|| format!("Failed to read {}", uhdr_path.display()))?;
    let mut dec = Decoder::new()?;
    let mut comp = CompressedImage::from_bytes(
        &mut uhdr_bytes,
        sys::uhdr_color_gamut::UHDR_CG_UNSPECIFIED,
        sys::uhdr_color_transfer::UHDR_CT_UNSPECIFIED,
        sys::uhdr_color_range::UHDR_CR_UNSPECIFIED,
    );
    dec.set_image(&mut comp)?;

    let (img_fmt, _) = fmt.to_img_fmt();
    let decoded = dec.decode_packed_view(img_fmt, transfer.to_ct())?;
    let mut file =
        File::create(&out_raw_path).with_context(|| format!("Failed to write {}", out_raw_path.display()))?;
    for y in 0..decoded.height() as usize {
        let row = decoded.row(y)?;
        file.write_all(row)
            .with_context(|| format!("Failed to write {}", out_raw_path.display()))?;
    }
    println!(
        "Decoded {} -> {} ({}x{}, {:?} {:?})",
        uhdr_path.display(),
        out_raw_path.display(),
        decoded.width(),
        decoded.height(),
        img_fmt,
        transfer.to_ct()
    );
    Ok(())
}
