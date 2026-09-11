use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum, builder::ValueHint};
use ultrahdr::Codec;

/// Container written by `ultrahdr-bake bake`.
#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// JPEG UltraHDR; always available.
    Jpeg,
    /// AVIF (AV1); needs the `heif` feature plus libaom, either from the host or
    /// cross-compiled with the `wasm-avif` feature.
    Avif,
    /// HEIF/HEIC (HEVC); needs the `heif` feature and a host HEVC encoder (x265).
    Heif,
}

impl OutputFormat {
    /// The libultrahdr output codec behind this format.
    pub fn codec(self) -> Codec {
        match self {
            Self::Jpeg => Codec::Jpeg,
            Self::Avif => Codec::Avif,
            Self::Heif => Codec::Heif,
        }
    }

    /// File extension used when `--out` is omitted.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Avif => "avif",
            Self::Heif => "heic",
        }
    }
}

/// Command-line arguments for ultrahdr-bake.
#[derive(Parser, Debug, Clone)]
#[command(
    name = "ultrahdr-bake",
    about = "Bake an UltraHDR JPEG or assemble a Motion Photo (JPEG + MP4).",
    author,
    version,
    arg_required_else_help = true,
    args_conflicts_with_subcommands = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[command(flatten)]
    pub bake: BakeArgs,
}

impl Cli {
    pub fn into_command(self) -> Command {
        self.command.unwrap_or(Command::Bake(self.bake))
    }
}

#[derive(Subcommand, Debug, Clone)]
pub enum Command {
    /// Bake an UltraHDR JPEG from an HDR gain map JPEG and an SDR base JPEG.
    Bake(BakeArgs),
    /// Merge a JPEG + MP4 into a Motion Photo with Motion Photo metadata.
    Motion(MotionArgs),
}

#[derive(Args, Debug, Clone)]
pub struct BakeArgs {
    /// Two JPEGs; autodetect which is HDR (ISO 21496 gain map) vs SDR
    #[arg(value_name = "FILE", value_hint = ValueHint::FilePath, num_args = 0..=2)]
    pub inputs: Vec<PathBuf>,

    /// UltraHDR JPEG containing the HDR intent and gain map
    #[arg(long, value_hint = ValueHint::FilePath, value_name = "FILE")]
    pub hdr: Option<PathBuf>,

    /// SDR base JPEG to embed into the UltraHDR output
    #[arg(long, short = 's', value_hint = ValueHint::FilePath, value_name = "FILE")]
    pub sdr: Option<PathBuf>,

    /// Output UltraHDR JPEG path
    #[arg(
        long,
        short = 'o',
        value_hint = ValueHint::FilePath,
        value_name = "FILE",
        help = "Defaults to <SDR-filename>-merge.<ext> when omitted"
    )]
    pub out: Option<PathBuf>,

    /// Output container format
    #[arg(long, value_enum, default_value = "jpeg")]
    pub format: OutputFormat,

    /// JPEG quality for the SDR base image (1-100)
    #[arg(
        long = "base-q",
        default_value_t = 95,
        value_parser = clap::value_parser!(u8).range(1..=100)
    )]
    pub base_quality: u8,

    /// JPEG quality for the gain map (1-100)
    #[arg(
        long = "gm-q",
        alias = "gainmap-q",
        default_value_t = 100,
        value_parser = clap::value_parser!(u8).range(1..=100)
    )]
    pub gainmap_quality: u8,

    /// Gain map scale factor
    #[arg(
        long = "scale",
        default_value_t = 1,
        value_parser = clap::value_parser!(i32).range(1..)
    )]
    pub gainmap_scale: i32,

    /// Use RGB gain maps; pass --multichannel=false for a single-channel gain map
    #[arg(
        long = "multichannel",
        short = 'm',
        alias = "mc",
        default_value_t = true,
        action = clap::ArgAction::Set,
        num_args = 0..=1,
        default_missing_value = "true",
        require_equals = true
    )]
    pub multichannel_gainmap: bool,

    /// Override target peak brightness in nits (falls back to metadata or 1600 nits)
    #[arg(long = "target-peak", value_name = "NITS")]
    pub target_peak_nits: Option<f32>,
}

#[derive(Args, Debug, Clone)]
pub struct MotionArgs {
    /// Two inputs; auto-detect which is JPEG photo vs MP4 video
    #[arg(value_name = "FILE", value_hint = ValueHint::FilePath, num_args = 0..=2)]
    pub inputs: Vec<PathBuf>,

    /// Still JPEG to act as the Motion Photo primary image
    #[arg(long, short = 'p', value_hint = ValueHint::FilePath, value_name = "JPEG")]
    pub photo: Option<PathBuf>,

    /// MP4 clip to append to the Motion Photo
    #[arg(long, short = 'v', value_hint = ValueHint::FilePath, value_name = "MP4")]
    pub video: Option<PathBuf>,

    /// Output Motion Photo JPEG path
    #[arg(
        long,
        short = 'o',
        value_hint = ValueHint::FilePath,
        value_name = "FILE",
        help = "Defaults to <photo-filename>-motion.<ext> when omitted"
    )]
    pub out: Option<PathBuf>,

    /// Presentation timestamp (microseconds) for the still frame within the motion clip
    #[arg(long = "timestamp-us", default_value_t = 0)]
    pub presentation_timestamp_us: u64,
}
