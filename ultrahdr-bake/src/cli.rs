use std::path::PathBuf;

use clap::{builder::ValueHint, Parser};

/// Command-line arguments for ultrahdr-bake.
#[derive(Parser, Debug)]
#[command(
    name = "ultrahdr-bake",
    about = "Bake an UltraHDR JPEG from an HDR gain map JPEG and an SDR base JPEG.",
    author,
    version,
    arg_required_else_help = true
)]
pub struct Cli {
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
        default_value = "ultrahdr_bake_out.jpg"
    )]
    pub out: PathBuf,

    /// JPEG quality for the SDR base image (1-100)
    #[arg(
        long = "base-q",
        default_value_t = 95,
        value_parser = clap::value_parser!(i32).range(1..=100)
    )]
    pub base_quality: i32,

    /// JPEG quality for the gain map (1-100)
    #[arg(
        long = "gm-q",
        alias = "gainmap-q",
        default_value_t = 95,
        value_parser = clap::value_parser!(i32).range(1..=100)
    )]
    pub gainmap_quality: i32,

    /// Gain map scale factor
    #[arg(
        long = "scale",
        default_value_t = 1,
        value_parser = clap::value_parser!(i32).range(1..)
    )]
    pub gainmap_scale: i32,

    /// Use multi-channel gain maps (--mc works too)
    #[arg(long = "multichannel", short = 'm', alias = "mc")]
    pub multichannel_gainmap: bool,

    /// Override target peak brightness in nits (falls back to metadata or 1600 nits)
    #[arg(long = "target-peak", value_name = "NITS")]
    pub target_peak_nits: Option<f32>,
}
