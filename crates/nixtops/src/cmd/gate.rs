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
    /// Compare against this base
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

/// Runs the gate with the given inputs; the caller chooses the date, CI flag and tools.
pub fn run_with(args: &Args, ci: bool, tools: Option<Tools>, seams: &Seams) -> Output {
    let only = match args.only.as_deref().map(LayerId::parse) {
        Some(None) => {
            let layers: Vec<&str> = crate::report::ALL_LAYERS
                .iter()
                .map(|l| l.as_str())
                .collect();
            let message = format!("nixtops gate: --only takes one of {}\n", layers.join(", "));
            return Output::fail(Exit::Refused, message);
        }
        Some(layer) => layer,
        None => None,
    };
    let settings = settings_of(args, only, ci);
    if args.dry_run && ci {
        let reason = "--dry-run is refused when CI is set";
        let report = gate::run::stopped_report(&settings, None, reason);
        let _ = gate::run::write_report(&settings.root.join(".nix-tops/report.json"), &report);
        return Output::fail(Exit::Refused, format!("nixtops gate: {reason}\n"));
    }
    if args.dry_run {
        return Output::ok(plan(&settings));
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
