//! `nixtops report`: not built yet.

use crate::{cmd::not_built, exit::Output};
use clap::Args as ClapArgs;
use clap::ValueEnum;

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Format {
    Text,
    Json,
}

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Output format
    #[arg(long, value_enum, default_value = "text")]
    pub format: Format,
}

pub fn run(_args: &Args) -> Output {
    not_built("report")
}
