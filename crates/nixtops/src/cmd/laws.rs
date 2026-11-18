//! `nixtops laws`: not built yet.

use crate::{cmd::not_built, exit::Output};
use clap::{Args as ClapArgs, Subcommand};

#[derive(Debug, ClapArgs)]
pub struct Args {
    #[command(subcommand)]
    pub action: Option<Action>,
}

#[derive(Debug, Subcommand)]
pub enum Action {
    /// The catalog with readers
    List,
    /// Run the laws alone
    Run,
    /// Explain one law
    Explain {
        /// A law id such as N-04
        id: String,
    },
}

pub fn run(_args: &Args) -> Output {
    not_built("laws")
}
