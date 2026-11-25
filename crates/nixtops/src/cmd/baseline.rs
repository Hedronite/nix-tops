//! `nixtops baseline`: runs the gate and, when every layer passed, writes the baseline. It is the
//! only writer. It refuses to write over a baseline that is not the committed one, to start again
//! after a baseline was deleted, and to record anything looser than the baseline it replaces.

use crate::{
    baseline::{compare, file, file::Baseline, history},
    build_info,
    cmd::gate::EvaluatorArg,
    config::{self, baseline::real_git, semantic},
    exit::{Exit, Output},
    gate::{self, Evaluator, Seams, Settings},
    inventory::Tools,
    probe,
    report::render_text,
};
use clap::Args as ClapArgs;
use serde_json::Value;
use std::{fs, path::PathBuf};

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// The flake directory
    #[arg(default_value = ".")]
    pub path: PathBuf,
    /// Which Nix judges
    #[arg(long, value_enum, default_value = "pinned")]
    pub evaluator: EvaluatorArg,
}

fn fail(exit: Exit, why: &str) -> Output {
    Output::fail(exit, format!("nixtops baseline: {why}\n"))
}

fn config_of(settings: &Settings) -> Option<Value> {
    let text = fs::read_to_string(settings.root.join("nix-tops.toml")).ok()?;
    config::load(&text).ok()
}

/// The previous baseline, after git agreed that writing is allowed.
fn previous(settings: &Settings, path: &str) -> Result<Option<Baseline>, Output> {
    let file = settings.root.join(path);
    let git = real_git(&settings.root);
    history::may_write(&git, path, file.exists()).map_err(|why| fail(Exit::Refused, &why))?;
    file::load(&file).map_err(|why| fail(Exit::GateFailed, &why))
}

/// The gate's own failure, passed on with its exit code and its report.
fn not_written(result: &gate::run::RunResult) -> Output {
    let report = serde_json::to_value(&result.report).unwrap_or_default();
    let exit = match result.exit {
        Exit::Pass | Exit::GateFailed => Exit::GateFailed,
        other => other,
    };
    let mut stderr: String = result.problems.iter().map(|p| format!("{p}\n")).collect();
    stderr.push_str("nixtops baseline: not written: the gate did not pass\n");
    Output {
        exit,
        stdout: render_text(&report),
        stderr,
    }
}

fn written(path: &str, new: &Baseline) -> Output {
    Output::ok(format!(
        "wrote {path}: {} finding(s) from {} lint tool(s), {} ack(s)\ncommit it with the change\n",
        new.total(),
        new.lint.len(),
        new.acks
    ))
}

/// What `old` refuses in `new`, in every enforcement mode.
fn looser(doc: &Value, old: Option<&Baseline>, new: &Baseline) -> Result<(), Output> {
    let found = old.map(|old| compare::judge(doc, old, new));
    match found.filter(|f| !f.failures.is_empty()) {
        Some(found) => {
            let why = format!("not written, looser than the baseline: {}", found.all());
            Err(fail(Exit::GateFailed, &why))
        }
        None => Ok(()),
    }
}

fn record(
    settings: &Settings,
    doc: &Value,
    tools: Option<Tools>,
    seams: &Seams,
) -> Result<Output, Output> {
    let path = semantic::baseline_path(doc);
    let old = previous(settings, path)?;
    let result = gate::run(settings, tools, seams);
    if !result.report.passed {
        return Err(not_written(&result));
    }
    let new = Baseline::of(&settings.root, doc, &result.report);
    looser(doc, old.as_ref(), &new)?;
    file::write(&settings.root.join(path), &new)
        .map_err(|e| fail(Exit::Environment, &format!("cannot write {path}: {e}")))?;
    Ok(written(path, &new))
}

/// Runs with the settings and tools given. A config that does not load goes straight to the gate,
/// which names the problem.
pub fn run_with(settings: &Settings, tools: Option<Tools>, seams: &Seams) -> Output {
    match config_of(settings) {
        Some(doc) => record(settings, &doc, tools, seams).unwrap_or_else(|refusal| refusal),
        None => not_written(&gate::run(settings, tools, seams)),
    }
}

fn settings_of(args: &Args) -> Settings {
    Settings {
        root: args.path.clone(),
        keep_going: false,
        only: None,
        system: None,
        evaluator: match args.evaluator {
            EvaluatorArg::Pinned => Evaluator::Pinned,
            EvaluatorArg::Host => Evaluator::Host,
        },
        ci: std::env::var_os("CI").is_some_and(|value| !value.is_empty()),
        today: gate::run::today(),
        nix: probe::nix_bin(),
        writing_baseline: true,
    }
}

pub fn run(args: &Args) -> Output {
    let tools = Tools::from_parts(
        build_info::JUDGE_PATH,
        build_info::EVAL_JOBS_PATH,
        build_info::NIXPKGS_REF,
    );
    run_with(&settings_of(args), tools, &Seams::default())
}
