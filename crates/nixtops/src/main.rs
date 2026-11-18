use clap::Parser;
use nixtops::{cmd, exit::Output};
use std::process::ExitCode;

/// Nix-TOPS: gate a Nix flake against the protocol.
#[derive(Debug, Parser)]
#[command(name = "nixtops", disable_version_flag = true)]
struct Cli {
    #[command(subcommand)]
    command: cmd::Command,
}

fn emit(output: &Output) -> ExitCode {
    print!("{}", output.stdout);
    eprint!("{}", output.stderr);
    ExitCode::from(output.exit.code())
}

fn main() -> ExitCode {
    emit(&cmd::run(&Cli::parse().command))
}
