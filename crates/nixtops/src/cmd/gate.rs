//! `nixtops gate`: runs gate 1 (source) and gate 3 (eval), writes `.nix-tops/report.json`, and
//! prints the report. The unbuilt layers are `not-run` unless the config switches them off.

use crate::{
    build_info,
    exit::{Exit, Output},
    gate::{self, Evaluator, Seams, Settings},
    inventory::Tools,
    probe,
    report::{LayerId, Report, render_text},
};
use clap::{Args as ClapArgs, ValueEnum};
use std::path::PathBuf;

/// Which Nix judges.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum EvaluatorArg {
    /// The CppNix pinned in the kit's lock (the default)
    Pinned,
    /// The host's own Nix (the Determinate leg of the CI floor)
    Host,
}

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// The flake directory
    #[arg(default_value = ".")]
    pub path: PathBuf,
    /// Highest tier to run
    #[arg(long, default_value_t = 1)]
    pub max_tier: u8,
    /// Compare against this base (refused until the diff laws are built)
    #[arg(long)]
    pub base: Option<String>,
    /// Keep going after a layer fails
    #[arg(long)]
    pub keep_going: bool,
    /// Print JSON
    #[arg(long)]
    pub json: bool,
    /// Run one layer only
    #[arg(long)]
    pub only: Option<String>,
    /// Evaluate one system only
    #[arg(long)]
    pub system: Option<String>,
    /// Show what would run (refused when CI is set)
    #[arg(long)]
    pub dry_run: bool,
    /// Which Nix judges
    #[arg(long, value_enum, default_value = "pinned")]
    pub evaluator: EvaluatorArg,
}

fn ci_is_set() -> bool {
    std::env::var_os("CI").is_some_and(|value| !value.is_empty())
}

fn settings_of(args: &Args, only: Option<LayerId>, ci: bool) -> Settings {
    Settings {
        root: args.path.clone(),
        keep_going: args.keep_going,
        only,
        system: args.system.clone(),
        evaluator: match args.evaluator {
            EvaluatorArg::Pinned => Evaluator::Pinned,
            EvaluatorArg::Host => Evaluator::Host,
        },
        ci,
        today: gate::run::today(),
        nix: probe::nix_bin(),
        writing_baseline: false,
    }
}

fn plan(settings: &Settings) -> String {
    let mut text = format!("gate (dry run) in {}\n", settings.root.display());
    for id in crate::report::ALL_LAYERS {
        text.push_str(&format!("  {}\n", id.as_str()));
    }
    text
}

fn render(args: &Args, report: &Report) -> String {
    if args.json {
        let mut text = serde_json::to_string_pretty(report).unwrap_or_default();
        text.push('\n');
        return text;
    }
    render_text(&serde_json::to_value(report).unwrap_or_default())
}

/// The one tier this version builds.
const BUILT_TIER: u8 = 1;

/// Writes the report of a run that did not run: every layer `not-run` with `reason`.
fn write_stopped(settings: &Settings, reason: &str) {
    let report = gate::run::stopped_report(settings, None, reason);
    let _ = gate::run::write_report(&settings.root.join(".nix-tops/report.json"), &report);
}

/// A refusal by policy (exit 4): the report is written, so a refusal leaves a trace.
fn refuse(settings: &Settings, reason: &str) -> Output {
    write_stopped(settings, reason);
    Output::fail(Exit::Refused, format!("nixtops gate: {reason}\n"))
}

/// Runs the gate with the given inputs; the caller chooses the date, CI flag and tools.
/// `--only <layer>`: the layer, or the refusal (an unknown name is not a run; no report).
fn parse_only(args: &Args) -> Result<Option<LayerId>, Output> {
    let Some(name) = args.only.as_deref() else {
        return Ok(None);
    };
    LayerId::parse(name).map(Some).ok_or_else(|| {
        let layers: Vec<&str> = crate::report::ALL_LAYERS
            .iter()
            .map(|l| l.as_str())
            .collect();
        let message = format!("nixtops gate: --only takes one of {}\n", layers.join(", "));
        Output::fail(Exit::Refused, message)
    })
}

/// The runs that stop before any layer: a tier that is not built, a base to compare against (no
/// diff law is built), and a dry run.
fn before_any_layer(args: &Args, settings: &Settings, ci: bool) -> Option<Output> {
    if args.max_tier != BUILT_TIER {
        let reason = format!(
            "only tier {BUILT_TIER} is built; --max-tier {} is refused",
            args.max_tier
        );
        return Some(refuse(settings, &reason));
    }
    if args.base.is_some() {
        let reason = "the diff laws that use --base are not built in M1";
        return Some(refuse(settings, reason));
    }
    if args.dry_run && ci {
        return Some(refuse(settings, "--dry-run is refused when CI is set"));
    }
    args.dry_run.then(|| {
        write_stopped(settings, "dry run");
        Output::ok(plan(settings))
    })
}

pub fn run_with(args: &Args, ci: bool, tools: Option<Tools>, seams: &Seams) -> Output {
    let only = match parse_only(args) {
        Ok(only) => only,
        Err(refusal) => return refusal,
    };
    let settings = settings_of(args, only, ci);
    if let Some(output) = before_any_layer(args, &settings, ci) {
        return output;
    }
    let result = gate::run(&settings, tools, seams);
    let stderr: String = result.problems.iter().map(|p| format!("{p}\n")).collect();
    Output {
        exit: result.exit,
        stdout: render(args, &result.report),
        stderr,
    }
}

pub fn run(args: &Args) -> Output {
    let tools = Tools::from_parts(
        build_info::JUDGE_PATH,
        build_info::EVAL_JOBS_PATH,
        build_info::NIXPKGS_REF,
    );
    run_with(args, ci_is_set(), tools, &Seams::default())
}
