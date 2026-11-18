//! `nixtops baseline`: not built yet.

use crate::{cmd::not_built, exit::Output};
use clap::Args as ClapArgs;

#[derive(Debug, ClapArgs)]
pub struct Args {}

pub fn run(_args: &Args) -> Output {
    not_built("baseline")
}
