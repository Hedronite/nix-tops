//! `nixtops report`: renders the last report (`.nix-tops/report.json`), as text or as JSON.

use crate::{
    exit::{Exit, Output},
    report::render_text,
};
use clap::{Args as ClapArgs, ValueEnum};
use std::{fs, path::PathBuf};

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
    /// The flake directory
    #[arg(default_value = ".")]
    pub path: PathBuf,
}

/// Renders the report file at `path`. A missing or unreadable report is an environment error: there
/// is nothing to render until a gate has run.
pub fn render_file(path: &std::path::Path, format: Format) -> Output {
    let Ok(text) = fs::read_to_string(path) else {
        return Output::fail(
            Exit::Environment,
            format!(
                "nixtops report: no report at {}; run `nixtops gate` first\n",
                path.display()
            ),
        );
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Output::fail(
            Exit::Environment,
            format!("nixtops report: {} is not JSON\n", path.display()),
        );
    };
    match format {
        Format::Json => Output::ok(text),
        Format::Text => Output::ok(render_text(&value)),
    }
}

pub fn run(args: &Args) -> Output {
    render_file(&args.path.join(".nix-tops/report.json"), args.format)
}
