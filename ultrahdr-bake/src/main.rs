use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

use anyhow::{Result, ensure};
use clap::Parser;

mod cli;
mod color;
mod detect;
mod encode;
mod motion;

fn main() -> Result<()> {
    let args = cli::Cli::parse();
    run(args.into_command())
}

fn run(cmd: cli::Command) -> Result<()> {
    match cmd {
        cli::Command::Bake(args) => {
            ensure!(
                args.inputs.is_empty() || (args.hdr.is_none() && args.sdr.is_none()),
                "Provide either two positional JPEGs for auto-detection or --hdr/--sdr, not both"
            );

            let inputs = detect::resolve_inputs(&args)?;
            let out_path = resolve_out_path(&args, &inputs);
            encode::run_encoding(&args, &inputs, &out_path)
        }
        cli::Command::Motion(args) => {
            ensure!(
                args.inputs.is_empty() || (args.photo.is_none() && args.video.is_none()),
                "Provide either two positional inputs for auto-detection or --photo/--video, not both"
            );

            let inputs = motion::resolve_inputs(&args)?;
            let out_path = resolve_motion_out_path(&args, &inputs);
            motion::run_motion(&args, &inputs, &out_path)
        }
    }
}

fn resolve_out_path(args: &cli::BakeArgs, inputs: &detect::InputPair) -> PathBuf {
    args.out
        .clone()
        .unwrap_or_else(|| default_out_for_sdr(&inputs.sdr))
}

fn default_out_for_sdr(sdr_path: &Path) -> PathBuf {
    let parent = sdr_path.parent().unwrap_or_else(|| Path::new("."));
    let stem = sdr_path.file_stem().unwrap_or_else(|| OsStr::new("sdr"));
    let ext = sdr_path.extension().unwrap_or_else(|| OsStr::new("jpg"));

    let mut filename = stem.to_os_string();
    filename.push("-merge");
    filename.push(".");
    filename.push(ext);

    let mut out = parent.to_path_buf();
    out.push(filename);
    out
}

fn resolve_motion_out_path(args: &cli::MotionArgs, inputs: &motion::MotionInputPair) -> PathBuf {
    args.out
        .clone()
        .unwrap_or_else(|| default_motion_out(&inputs.photo))
}

fn default_motion_out(photo_path: &Path) -> PathBuf {
    let parent = photo_path.parent().unwrap_or_else(|| Path::new("."));
    let stem = photo_path
        .file_stem()
        .unwrap_or_else(|| OsStr::new("photo"));
    let ext = photo_path.extension().unwrap_or_else(|| OsStr::new("jpg"));

    let mut filename = stem.to_os_string();
    filename.push("-motion");
    filename.push(".");
    filename.push(ext);

    let mut out = parent.to_path_buf();
    out.push(filename);
    out
}
