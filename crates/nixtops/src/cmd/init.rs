//! `nixtops init`: not built yet.

use crate::{cmd::not_built, exit::Output};
use clap::Args as ClapArgs;

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Facet to write (repeatable); narrows the facets the outputs show
    #[arg(long = "facet")]
    pub facets: Vec<String>,
    /// Overwrite existing files
    #[arg(long)]
    pub force: bool,
}

pub fn run(_args: &Args) -> Output {
    not_built("init")
}
