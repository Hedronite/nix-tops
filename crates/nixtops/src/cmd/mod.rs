//! The command table of protocol section 12. One module per command; `run` dispatches.

use crate::exit::{Exit, Output};
use clap::Subcommand;

pub mod baseline;
pub mod check;
pub mod doctor;
pub mod gate;
pub mod init;
pub mod laws;
pub mod outputs;
pub mod report;
pub mod version;

/// The commands, in two flattened groups so no dispatch function grows past a few arms.
#[derive(Debug, Subcommand)]
pub enum Command {
    #[command(flatten)]
    Setup(Setup),
    #[command(flatten)]
    Gating(Gating),
}

/// Commands that prepare or describe a project and its host.
#[derive(Debug, Subcommand)]
pub enum Setup {
    /// Write nix-tops.toml, the CI workflow and the AGENTS.md section
    Init(init::Args),
    /// Validate nix-tops.toml
    Check(check::Args),
    /// Report the host's Nix
    Doctor(doctor::Args),
    /// Protocol, package and tool versions
    Version(version::Args),
}

/// Commands that run the gate or read its results.
#[derive(Debug, Subcommand)]
pub enum Gating {
    /// Run the enabled layers
    Gate(gate::Args),
    /// Write .nix-tops/baseline.json after a passing run
    Baseline(baseline::Args),
    /// The law catalog
    Laws(laws::Args),
    /// List every output with its coverage level
    Outputs(outputs::Args),
    /// Render the last report
    Report(report::Args),
}

pub fn run(command: &Command) -> Output {
    match command {
        Command::Setup(setup) => run_setup(setup),
        Command::Gating(gating) => run_gating(gating),
    }
}

fn run_setup(command: &Setup) -> Output {
    match command {
        Setup::Init(args) => init::run(args),
        Setup::Check(args) => check::run(args),
        Setup::Doctor(args) => doctor::run(args),
        Setup::Version(args) => version::run(args),
    }
}

fn run_gating(command: &Gating) -> Output {
    match command {
        Gating::Gate(args) => gate::run(args),
        Gating::Baseline(args) => baseline::run(args),
        Gating::Laws(args) => laws::run(args),
        Gating::Outputs(args) => outputs::run(args),
        Gating::Report(args) => report::run(args),
    }
}

/// What a command that is not built yet returns: a message and exit 4, never 0.
pub fn not_built(name: &str) -> Output {
    Output::fail(Exit::Refused, format!("nixtops {name}: not built yet\n"))
}
