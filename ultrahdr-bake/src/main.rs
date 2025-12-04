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
            encode::run_encoding(&args, &inputs)
        }
        cli::Command::Motion(args) => motion::run_motion(&args),
    }
}
