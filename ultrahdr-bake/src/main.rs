use anyhow::{ensure, Result};
use clap::Parser;

mod cli;
mod detect;
mod encode;

fn main() -> Result<()> {
    let args = cli::Cli::parse();
    run(args)
}

fn run(args: cli::Cli) -> Result<()> {
    ensure!(
        args.inputs.is_empty() || (args.hdr.is_none() && args.sdr.is_none()),
        "Provide either two positional JPEGs for auto-detection or --hdr/--sdr, not both"
    );

    let inputs = detect::resolve_inputs(&args)?;
    encode::run_encoding(&args, &inputs)
}
