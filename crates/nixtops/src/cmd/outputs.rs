//! `nixtops outputs`: not built yet.

use crate::{cmd::not_built, exit::Output};
use clap::Args as ClapArgs;

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Print JSON
    #[arg(long)]
    pub json: bool,
}

pub fn run(_args: &Args) -> Output {
    not_built("outputs")
}
