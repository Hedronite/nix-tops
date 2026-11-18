//! `nixtops check`: validates `nix-tops.toml` against the embedded schema, then the checks of
//! protocol section 12 that need no outputs, then the host daemon floor. Exit 2 for any config
//! failure (each names its key), 3 when only the daemon is below the floor.

use crate::{
    config::{self, Problem, semantic},
    exit::{Exit, Output},
    host::{self, Floor},
    probe,
};
use clap::Args as ClapArgs;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// The configuration file
    #[arg(long, default_value = "nix-tops.toml")]
    pub config: PathBuf,
}

fn floor_problem(nix: &str) -> Result<String, String> {
    let parsed = probe::version_line(nix)
        .as_deref()
        .and_then(host::parse_version_line);
    let Some(h) = parsed else {
        return Err(format!("host: cannot read the version of `{nix}`"));
    };
    match host::check_floor(&h) {
        Floor::Met => Ok(format!(
            "host: {} {} meets the daemon floor",
            h.implementation.name(),
            h.version
        )),
        Floor::Below(why) => Err(format!("host: daemon floor not met: {why}")),
    }
}

fn render(problems: &[Problem]) -> String {
    problems
        .iter()
        .map(|p| format!("nix-tops.toml: {}: {}\n", p.key, p.message))
        .collect()
}

/// The checks, with the date, the nix binary and the directory given by the caller.
pub fn check_at(config: &Path, today: &str, nix: &str) -> Output {
    let Ok(text) = fs::read_to_string(config) else {
        return Output::fail(
            Exit::ConfigInvalid,
            format!("nix-tops.toml: (file): cannot read {}\n", config.display()),
        );
    };
    let doc = match config::load(&text) {
        Ok(doc) => doc,
        Err(problems) => return Output::fail(Exit::ConfigInvalid, render(&problems)),
    };
    let dir = config
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let lock = fs::read_to_string(dir.join("flake.lock"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok());
    let ctx = semantic::Context {
        today,
        lock: lock.as_ref(),
        baseline_exists: dir.join(semantic::baseline_path(&doc)).is_file(),
    };
    let found = semantic::check(&doc, &ctx);
    let mut lines = found.notes;
    let floor = floor_problem(nix);
    let floor_failed = match &floor {
        Ok(line) => {
            lines.push(line.clone());
            false
        }
        Err(line) => {
            lines.push(line.clone());
            true
        }
    };
    let mut stdout = lines.join("\n") + "\n";
    if !found.problems.is_empty() {
        return Output {
            exit: Exit::ConfigInvalid,
            stdout,
            stderr: render(&found.problems),
        };
    }
    if floor_failed {
        return Output {
            exit: Exit::Environment,
            stdout: std::mem::take(&mut stdout),
            stderr: String::new(),
        };
    }
    stdout.push_str("nix-tops.toml: ok\n");
    Output::ok(stdout)
}

pub fn run(args: &Args) -> Output {
    check_at(&args.config, &semantic::today_utc(), &probe::nix_bin())
}
