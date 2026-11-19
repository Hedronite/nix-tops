//! The gate run: config first, then the layers in order, then the report.

use super::{
    diag, eval, layer, ledger,
    source::{self, Input as SourceInput},
};
use crate::{
    build_info,
    cmd::check,
    config::{self, semantic},
    exit::Exit,
    host,
    inventory::{self, Backend, Session, Tools},
    report::{
        ALL_LAYERS, AckEntry, BaselineIdentity, Layer, LayerId, OutputEntry, Parts, Report,
        Verdict, WarningEntry,
    },
};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Which Nix judges: the kit's pinned CppNix, or the host's own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Evaluator {
    Pinned,
    Host,
}

/// What a run needs besides the flake.
pub struct Settings {
    pub root: PathBuf,
    pub keep_going: bool,
    pub only: Option<LayerId>,
    pub system: Option<String>,
    pub evaluator: Evaluator,
    pub ci: bool,
    /// The UTC date, `YYYY-MM-DD`.
    pub today: String,
    /// The host's `nix`, for the daemon floor and for `--evaluator host`.
    pub nix: String,
}

/// A test-only argument. A normal run has none: the diagnostics table is `laws/diagnostics.toml`
/// and nothing else. Tests pass entries here to show how an entry's `effect` is applied (the
/// embedded table has only `report` entries).
#[derive(Default)]
pub struct Seams {
    pub extra_diagnostics: Vec<diag::Entry>,
}

/// The result of a run.
pub struct RunResult {
    pub report: Report,
    pub exit: Exit,
    /// Lines for stderr: what stopped the run before any layer.
    pub problems: Vec<String>,
}

const DEFAULT_REPORT: &str = ".nix-tops/report.json";

fn report_path(root: &Path, doc: Option<&Value>) -> PathBuf {
    let configured = doc
        .and_then(|d| d.pointer("/report/path"))
        .and_then(Value::as_str);
    root.join(configured.unwrap_or(DEFAULT_REPORT))
}

/// Writes the report, creating its directory.
pub fn write_report(path: &Path, report: &Report) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut text = serde_json::to_string_pretty(report).unwrap_or_default();
    text.push('\n');
    fs::write(path, text)
}

fn identity_of(settings: &Settings) -> (String, String) {
    match settings.evaluator {
        Evaluator::Pinned => (
            "cppnix".to_string(),
            build_info::judge_version(build_info::TOOLS)
                .unwrap_or("unknown")
                .to_string(),
        ),
        Evaluator::Host => {
            let line = crate::probe::version_line(&settings.nix);
            match line.as_deref().and_then(host::parse_version_line) {
                Some(h) => (
                    h.implementation
                        .name()
                        .to_lowercase()
                        .replace(' ', "-")
                        .replace("-nix", ""),
                    h.version,
                ),
                None => ("unknown".to_string(), "unknown".to_string()),
            }
        }
    }
}

fn unknown_identity(settings: &Settings) -> BaselineIdentity {
    let (implementation, version) = identity_of(settings);
    BaselineIdentity {
        implementation,
        version,
        system: "unknown".into(),
        lock_hash: "none".into(),
    }
}

fn make_report(
    enforce: String,
    baseline_identity: BaselineIdentity,
    facets: Vec<String>,
    body: Collected,
) -> Report {
    Report::finish(Parts {
        protocol: build_info::PROTOCOL.to_string(),
        enforce,
        baseline_identity,
        facets,
        layers: body.layers,
        outputs: body.outputs,
        warnings: body.warnings,
        acks: body.acks,
    })
}

/// A report for a run that stopped before any layer: every layer `not-run` with the reason.
pub fn stopped_report(settings: &Settings, doc: Option<&Value>, reason: &str) -> Report {
    let layers = ALL_LAYERS
        .iter()
        .map(|id| Layer::new(*id, Verdict::NotRun, Some(reason.to_string())))
        .collect();
    let facets = doc.map(facets_of).unwrap_or_default();
    let body = Collected {
        layers,
        outputs: Vec::new(),
        warnings: Vec::new(),
        acks: Vec::new(),
    };
    make_report(
        settings_enforce(doc),
        unknown_identity(settings),
        facets,
        body,
    )
}

fn settings_enforce(doc: Option<&Value>) -> String {
    let mode = doc.and_then(|d| d.get("enforce")).and_then(Value::as_str);
    mode.unwrap_or("hard").to_string()
}

fn facets_of(doc: &Value) -> Vec<String> {
    let list = doc.get("facets").and_then(Value::as_array);
    list.into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

fn lock_hash(session: &Session, root: &Path) -> String {
    let lock = root.join("flake.lock");
    let run = session.judge(&["hash", "file"], &[&lock.display().to_string()]);
    let hash = run.stdout.trim().to_string();
    if run.ok && !hash.is_empty() {
        hash
    } else {
        "none".to_string()
    }
}

fn system_of(session: &Session) -> String {
    let run = session.judge(&["config", "show", "system"], &[]);
    let system = run.stdout.trim().to_string();
    if run.ok && !system.is_empty() {
        system
    } else {
        "unknown".to_string()
    }
}

/// The reason a layer is switched off in the config (`[layers] <id> = { enabled = false, reason }`).
fn config_switch(doc: &Value, id: LayerId) -> Option<String> {
    let entry = doc.pointer(&format!("/layers/{}", id.as_str()))?;
    let off = entry.get("enabled").and_then(Value::as_bool) == Some(false);
    let reason = entry.get("reason").and_then(Value::as_str);
    off.then(|| reason.unwrap_or("switched off").to_string())
}

/// What the layers produced.
struct Collected {
    layers: Vec<Layer>,
    outputs: Vec<OutputEntry>,
    warnings: Vec<WarningEntry>,
    acks: Vec<AckEntry>,
}

fn skipped(settings: &Settings, id: LayerId) -> Option<Layer> {
    let other = settings.only.filter(|only| *only != id)?;
    Some(Layer::new(
        id,
        Verdict::NotRun,
        Some(format!("not selected: --only {}", other.as_str())),
    ))
}

fn run_layers(
    session: &mut Session,
    settings: &Settings,
    doc: &Value,
    seams: &Seams,
    flake_ref: &str,
) -> Collected {
    let mut table = diag::embedded().unwrap_or_default();
    table.extend(seams.extra_diagnostics.iter().cloned());
    let entries = ledger::entries(doc);
    let (implementation, _) = identity_of(settings);
    let mut got = Collected {
        layers: Vec::new(),
        outputs: Vec::new(),
        warnings: Vec::new(),
        acks: Vec::new(),
    };
    let mut stopped_by: Option<LayerId> = None;
    for id in ALL_LAYERS {
        let fixed = skipped(settings, id)
            .or_else(|| layer::fixed_state(id, config_switch(doc, id).as_deref()));
        if let Some(layer) = fixed {
            got.layers.push(layer);
            continue;
        }
        if let Some(by) = stopped_by {
            let reason = format!("stopped: {} failed; use --keep-going", by.as_str());
            got.layers
                .push(Layer::new(id, Verdict::NotRun, Some(reason)));
            continue;
        }
        let layer = match id {
            LayerId::Source => {
                let input = SourceInput {
                    root: &settings.root,
                    flake_ref,
                    ci: settings.ci,
                };
                source::layer(&source::findings(session, &input))
            }
            _ => {
                let input = eval::Input {
                    flake_ref,
                    today: &settings.today,
                    ledger: &entries,
                    diagnostics: &table,
                    implementation: &implementation,
                    system: settings.system.as_deref(),
                };
                let outcome = eval::run(session, &input);
                got.outputs = outcome.outputs;
                got.warnings = outcome.warnings;
                got.acks = outcome.acks;
                outcome.layer
            }
        };
        if layer.verdict.fails() && !settings.keep_going {
            stopped_by = Some(id);
        }
        got.layers.push(layer);
    }
    got
}

fn session_for(settings: &Settings, tools: Tools) -> std::io::Result<Session> {
    let tools = match settings.evaluator {
        Evaluator::Pinned => tools,
        Evaluator::Host => Tools {
            judge: PathBuf::from(&settings.nix),
            ..tools
        },
    };
    let mut session = Session::new(tools)?;
    if settings.evaluator == Evaluator::Host {
        session.set_backend(Backend::PerAttribute);
        session.set_determinate(identity_of(settings).0 == "determinate");
    }
    Ok(session)
}

fn stopped(
    settings: &Settings,
    doc: Option<&Value>,
    exit: Exit,
    problems: Vec<String>,
) -> RunResult {
    let reason = if problems.is_empty() {
        "the run stopped before any layer".to_string()
    } else {
        problems.join("\n")
    };
    let report = stopped_report(settings, doc, &reason);
    let _ = write_report(&report_path(&settings.root, doc), &report);
    RunResult {
        report,
        exit,
        problems,
    }
}

/// What a check that did not pass says on stderr: each problem with its key; a host below the
/// daemon floor is not a config problem, so its reason is the `host:` note.
fn stop_problems(verdict: &check::Verdict) -> Vec<String> {
    let problems: Vec<String> = verdict
        .problems
        .iter()
        .map(|p| format!("nix-tops.toml: {}: {}", p.key, p.message))
        .collect();
    if !problems.is_empty() {
        return problems;
    }
    let host = verdict.notes.iter().filter(|n| n.starts_with("host:"));
    host.cloned().collect()
}

/// Runs the gate in `settings.root`: config first, then the layers.
pub fn run(settings: &Settings, tools: Option<Tools>, seams: &Seams) -> RunResult {
    let config_path = settings.root.join("nix-tops.toml");
    let verdict = check::evaluate(&config_path, &settings.today, &settings.nix);
    if verdict.exit != Exit::Pass {
        let problems = stop_problems(&verdict);
        return stopped(settings, None, verdict.exit, problems);
    }
    let doc = std::fs::read_to_string(&config_path)
        .ok()
        .and_then(|t| config::load(&t).ok());
    let (Some(doc), Some(tools)) = (doc, tools) else {
        return stopped(
            settings,
            None,
            Exit::Environment,
            vec!["this build has no pinned judge (built outside the kit's flake)".to_string()],
        );
    };
    let Ok(mut session) = session_for(settings, tools) else {
        return stopped(
            settings,
            Some(&doc),
            Exit::Environment,
            vec!["cannot create a scratch directory".to_string()],
        );
    };
    let flake_ref = settings
        .root
        .canonicalize()
        .unwrap_or_else(|_| settings.root.clone())
        .display()
        .to_string();
    // The checks that read the outputs (facets, exported systems) come before any layer. When
    // the outputs cannot be read at all the layers say why (gate 1 for a stale lock, gate 3 for
    // an evaluation error), so that run goes on without those checks.
    if let Ok(structure) = inventory::structure_of(&session, &flake_ref) {
        let full =
            check::evaluate_with_outputs(&config_path, &settings.today, &settings.nix, &structure);
        if full.exit != Exit::Pass {
            return stopped(settings, Some(&doc), full.exit, stop_problems(&full));
        }
    }
    let got = run_layers(&mut session, settings, &doc, seams, &flake_ref);
    let (implementation, version) = identity_of(settings);
    let identity = BaselineIdentity {
        implementation,
        version,
        system: system_of(&session),
        lock_hash: lock_hash(&session, &settings.root),
    };
    let report = make_report(settings_enforce(Some(&doc)), identity, facets_of(&doc), got);
    let _ = write_report(&report_path(&settings.root, Some(&doc)), &report);
    let exit = if report.passed {
        Exit::Pass
    } else {
        Exit::GateFailed
    };
    RunResult {
        report,
        exit,
        problems: Vec::new(),
    }
}

/// The date used when none is given.
pub fn today() -> String {
    semantic::today_utc()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_layer_switch_in_the_config_needs_enabled_false_and_names_its_reason() {
        let doc = json!({"layers": {
            "lint": {"enabled": false, "reason": "r"},
            "laws": {"enabled": false},
            "coverage": {"enabled": true, "reason": "ignored"}
        }});
        assert_eq!(config_switch(&doc, LayerId::Lint).as_deref(), Some("r"));
        assert_eq!(
            config_switch(&doc, LayerId::Laws).as_deref(),
            Some("switched off")
        );
        assert_eq!(config_switch(&doc, LayerId::Coverage), None);
        assert_eq!(config_switch(&doc, LayerId::BuildTest), None);
        assert_eq!(config_switch(&json!({}), LayerId::Lint), None);
    }

    #[test]
    fn today_is_a_utc_date() {
        let t = today();
        assert_eq!((t.len(), &t[4..5], &t[7..8]), (10, "-", "-"));
        assert!(t.starts_with("20"));
    }

    #[test]
    fn the_report_path_defaults_and_follows_the_config() {
        let root = Path::new("/r");
        assert_eq!(
            report_path(root, None),
            PathBuf::from("/r/.nix-tops/report.json")
        );
        assert_eq!(
            report_path(root, Some(&json!({"report": {"path": "o/x.json"}}))),
            PathBuf::from("/r/o/x.json")
        );
    }

    #[test]
    fn the_enforce_mode_defaults_to_hard_when_nothing_was_read() {
        assert_eq!(settings_enforce(None), "hard");
        assert_eq!(
            settings_enforce(Some(&json!({"enforce": "ratchet"}))),
            "ratchet"
        );
        assert_eq!(facets_of(&json!({"facets": ["a", 1, "b"]})), ["a", "b"]);
    }
}
