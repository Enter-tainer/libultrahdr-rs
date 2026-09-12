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

    let mut filename = stem.to_os_string();
    filename.push("-merge");
    filename.push(".jpg");

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

#[cfg(test)]
mod tests {
    use super::*;

    fn bake_args(argv: &[&str]) -> cli::BakeArgs {
        match cli::Cli::parse_from(argv).into_command() {
            cli::Command::Bake(args) => args,
            other => panic!("expected bake args, got {other:?}"),
        }
    }

    #[test]
    fn default_output_name_follows_the_sdr_input() {
        let args = bake_args(&["ultrahdr-bake", "a.jpg", "b.jpg"]);

        assert_eq!(args.out, None);
        assert_eq!(
            default_out_for_sdr(Path::new("dir/base.jpg")),
            Path::new("dir/base-merge.jpg")
        );
    }

    #[test]
    fn explicit_out_wins_over_the_default_name() {
        let args = bake_args(&["ultrahdr-bake", "--out", "custom.bin", "a.jpg", "b.jpg"]);

        assert_eq!(args.out.as_deref(), Some(Path::new("custom.bin")));
    }
}
