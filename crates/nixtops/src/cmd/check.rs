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

/// What the checks found, without printing or exiting: the exit code the caller should use
/// (`Pass`, `ConfigInvalid` for any config failure, `Environment` below the daemon floor), the
/// failures with their keys, and the notes. `nixtops gate` calls [`evaluate`] first and stops
/// before any layer when `exit` is not `Pass`.
#[derive(Debug, PartialEq)]
pub struct Verdict {
    pub exit: Exit,
    pub problems: Vec<Problem>,
    pub notes: Vec<String>,
}

impl Verdict {
    fn invalid(problems: Vec<Problem>, notes: Vec<String>) -> Self {
        Self {
            exit: Exit::ConfigInvalid,
            problems,
            notes,
        }
    }

    /// The text and exit code of `nixtops check`.
    pub fn render(&self) -> Output {
        let mut stdout: String = self.notes.iter().map(|n| format!("{n}\n")).collect();
        match self.exit {
            Exit::Pass => {
                stdout.push_str("nix-tops.toml: ok\n");
                Output::ok(stdout)
            }
            exit => Output {
                exit,
                stdout,
                stderr: render(&self.problems),
            },
        }
    }
}

/// The checks, with the date, the nix binary and the config path given by the caller.
pub fn evaluate(config: &Path, today: &str, nix: &str) -> Verdict {
    let Ok(text) = fs::read_to_string(config) else {
        let problem = Problem::new("(file)".into(), format!("cannot read {}", config.display()));
        return Verdict::invalid(vec![problem], Vec::new());
    };
    let doc = match config::load(&text) {
        Ok(doc) => doc,
        Err(problems) => return Verdict::invalid(problems, Vec::new()),
    };
    let dir = config.parent().unwrap_or(Path::new("."));
    let lock = fs::read_to_string(dir.join("flake.lock"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok());
    let ctx = semantic::Context {
        today,
        lock: lock.as_ref(),
        baseline_exists: dir.join(semantic::baseline_path(&doc)).is_file(),
    };
    let found = semantic::check(&doc, &ctx);
    let mut notes = found.notes;
    let floor = floor_problem(nix);
    notes.push(floor.clone().unwrap_or_else(|line| line));
    if !found.problems.is_empty() {
        return Verdict::invalid(found.problems, notes);
    }
    let exit = if floor.is_ok() {
        Exit::Pass
    } else {
        Exit::Environment
    };
    Verdict {
        exit,
        problems: Vec::new(),
        notes,
    }
}

pub fn check_at(config: &Path, today: &str, nix: &str) -> Output {
    evaluate(config, today, nix).render()
}

pub fn run(args: &Args) -> Output {
    check_at(&args.config, &semantic::today_utc(), &probe::nix_bin())
}
