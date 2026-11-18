//! `nixtops gate`: not built yet.

use crate::{cmd::not_built, exit::Output};
use clap::Args as ClapArgs;

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Highest tier to run
    #[arg(long, default_value_t = 1)]
    pub max_tier: u8,
    /// Compare against this base
    #[arg(long)]
    pub base: Option<String>,
    /// Keep going after a layer fails
    #[arg(long)]
    pub keep_going: bool,
    /// Print JSON
    #[arg(long)]
    pub json: bool,
    /// Run one layer only
    #[arg(long)]
    pub only: Option<String>,
    /// Evaluate one system only
    #[arg(long)]
    pub system: Option<String>,
    /// Show what would run (refused when CI is set)
    #[arg(long)]
    pub dry_run: bool,
}

pub fn run(_args: &Args) -> Output {
    not_built("gate")
}
