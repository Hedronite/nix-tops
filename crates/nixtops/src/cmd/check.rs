//! `nixtops check`: validates `nix-tops.toml` against the embedded schema, then the checks of
//! protocol section 12 that need no outputs, then the host daemon floor. Exit 2 for any config
//! failure (each names its key), 3 when only the daemon is below the floor.

use crate::{
    build_info,
    config::{
        self, Outcome, Problem,
        baseline::{self, Git},
        outputs, semantic,
    },
    exit::{Exit, Output},
    host::{self, Floor},
    inventory::{Structure, Tools},
    probe,
};
use clap::Args as ClapArgs;
use serde_json::Value;
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
/// (`Pass`; `ConfigInvalid` for any config failure; `Environment` when the host Nix is below the
/// floor, git is missing, or the outputs cannot be read), the failures with their keys, and the
/// notes. `nixtops gate` calls [`evaluate_with_outputs`] first and stops before any layer when
/// `exit` is not `Pass`.
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

/// What the checks that read the world call: the flake's outputs and git. Tests pass stand-ins.
pub struct Probes<'a> {
    pub outputs: &'a dyn Fn() -> Result<Structure, String>,
    pub git: Git<'a>,
}

fn problem_of_read(message: String) -> Problem {
    Problem::new("outputs".into(), message)
}

/// The checks that need the world: the outputs, and under `ratchet` the baseline in git.
fn world(doc: &Value, baseline_exists: bool, probes: &Probes<'_>) -> Outcome {
    let mut out = match (probes.outputs)() {
        Ok(structure) => outputs::check(doc, &structure),
        Err(message) => Outcome {
            environment: vec![problem_of_read(message)],
            ..Outcome::default()
        },
    };
    let ratchet = semantic::enforce_of(doc) == "ratchet";
    if ratchet && baseline_exists {
        let path = semantic::baseline_path(doc);
        let found = baseline::findings(probes.git, path);
        out.problems.extend(found.problems);
        out.environment.extend(found.environment);
        out.notes.extend(found.notes);
    }
    out
}

fn assemble(
    config_problems: Vec<Problem>,
    environment: Vec<Problem>,
    notes: Vec<String>,
) -> Verdict {
    let exit = match (config_problems.is_empty(), environment.is_empty()) {
        (false, _) => Exit::ConfigInvalid,
        (true, false) => Exit::Environment,
        (true, true) => Exit::Pass,
    };
    let mut problems = config_problems;
    problems.extend(environment);
    Verdict {
        exit,
        problems,
        notes,
    }
}

fn evaluate_inner(
    config: &Path,
    today: &str,
    nix: &str,
    probes: Option<&Probes<'_>>,
    writing_baseline: bool,
) -> Verdict {
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
    let baseline_exists = dir.join(semantic::baseline_path(&doc)).is_file();
    let ctx = semantic::Context {
        today,
        lock: lock.as_ref(),
        baseline_exists,
        writing_baseline,
    };
    let found = semantic::check(&doc, &ctx);
    let mut notes = found.notes;
    let mut environment = Vec::new();
    let mut problems = found.problems;
    if let Some(probes) = probes {
        let more = world(&doc, baseline_exists, probes);
        problems.extend(more.problems);
        environment.extend(more.environment);
        notes.extend(more.notes);
    }
    match floor_problem(nix) {
        Ok(line) => notes.push(line),
        Err(line) => {
            notes.push(line.clone());
            environment.push(Problem::new("host".into(), line));
        }
    }
    assemble(problems, environment, notes)
}

/// The checks, with the date, the nix binary and the config path given by the caller. `probes`
/// is `None` for the checks that need no outputs and no git.
pub fn evaluate_with(
    config: &Path,
    today: &str,
    nix: &str,
    probes: Option<&Probes<'_>>,
) -> Verdict {
    evaluate_inner(config, today, nix, probes, false)
}

/// The checks that need no outputs (for unit use).
pub fn evaluate(config: &Path, today: &str, nix: &str) -> Verdict {
    evaluate_inner(config, today, nix, None, false)
}

/// Gate entry point without outputs, skipping the missing-baseline error when writing the baseline.
pub fn evaluate_gate(config: &Path, today: &str, nix: &str, writing_baseline: bool) -> Verdict {
    evaluate_inner(config, today, nix, None, writing_baseline)
}

/// All the checks of section 12 with the outputs already read: the entry point `nixtops check`
/// and `nixtops gate` call. Git is the real git, run in the config's directory.
pub fn evaluate_with_outputs(
    config: &Path,
    today: &str,
    nix: &str,
    structure: &Structure,
) -> Verdict {
    evaluate_gate_with_outputs(config, today, nix, structure, false)
}

/// Gate entry point with outputs, allowing `writing_baseline` bootstrap.
pub fn evaluate_gate_with_outputs(
    config: &Path,
    today: &str,
    nix: &str,
    structure: &Structure,
    writing_baseline: bool,
) -> Verdict {
    let dir = config.parent().unwrap_or(Path::new("."));
    let git = baseline::real_git(dir);
    let outputs = || Ok(structure.clone());
    let probes = Probes {
        outputs: &outputs,
        git: &git,
    };
    evaluate_inner(config, today, nix, Some(&probes), writing_baseline)
}

pub fn check_at(config: &Path, today: &str, nix: &str) -> Output {
    evaluate(config, today, nix).render()
}

/// `nixtops check` with the tools chosen by the caller and the date and nix binary given.
pub fn run_with(args: &Args, today: &str, nix: &str, tools: Option<Tools>) -> Output {
    let dir = args.config.parent().unwrap_or(Path::new("."));
    let git = baseline::real_git(dir);
    let outputs = || outputs::found(outputs::read_with(tools.clone(), dir));
    let probes = Probes {
        outputs: &outputs,
        git: &git,
    };
    evaluate_with(&args.config, today, nix, Some(&probes)).render()
}

pub fn run(args: &Args) -> Output {
    let tools = Tools::from_parts(
        build_info::JUDGE_PATH,
        build_info::EVAL_JOBS_PATH,
        build_info::NIXPKGS_REF,
    );
    run_with(args, &semantic::today_utc(), &probe::nix_bin(), tools)
}
