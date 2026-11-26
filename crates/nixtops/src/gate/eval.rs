//! Gate 3, eval (protocol section 3.1): `nix flake check --no-build --all-systems` under
//! `abort-on-warn` with import from derivation off, the inventory of every output (so one error
//! does not hide the others), the configuration build roots, `config.warnings` read as data, the
//! `nix` command's own diagnostics classified by `laws/diagnostics.toml`, and the ledger.

use super::{
    diag::{self, Effect},
    ledger::{self, Entry},
    system::{check_failed_for_system, only_system, record_concerns_system},
};
use crate::{
    inventory::{self, Inventory, Kind, Level, Record, Session},
    report::{AckEntry, Layer, LayerId, OutputEntry, Verdict, WarningEntry, WarningSource},
};
use serde_json::Value;

/// The text `nix` prints when `abort-on-warn` stops an evaluation.
const ABORT_MARKER: &str = "abort-on-warn is set";
const WARNING_PREFIX: &str = "evaluation warning: ";
const IFD_MARKER: &str = "import-from-derivation";

/// What gate 3 reads besides the flake.
pub struct Input<'a> {
    pub flake_ref: &'a str,
    pub today: &'a str,
    pub ledger: &'a [Entry],
    pub diagnostics: &'a [diag::Entry],
    /// `cppnix` or `determinate`: which entries of the diagnostics table apply.
    pub implementation: &'a str,
    /// Evaluate one system only (`--system`); `None` is `--all-systems`.
    pub system: Option<&'a str>,
}

/// Gate 3's result: the layer, and what the report lists.
pub struct Outcome {
    pub layer: Layer,
    pub warnings: Vec<WarningEntry>,
    pub outputs: Vec<OutputEntry>,
    pub acks: Vec<AckEntry>,
}

/// One evaluation: `flake check` and the inventory, under one setting of `abort-on-warn`.
struct Pass {
    check_ok: bool,
    check_text: String,
    inventory: Result<Inventory, String>,
    warning_scopes: Option<Vec<(String, bool)>>,
}

fn record_warnings(record: &Record) -> Vec<String> {
    record
        .error
        .as_deref()
        .map(evaluation_warnings)
        .unwrap_or_default()
}

fn warning_scopes_from_inventory(
    inventory: &Result<Inventory, String>,
    system: Option<&str>,
) -> Option<Vec<(String, bool)>> {
    let inv = inventory.as_ref().ok()?;
    let mut scopes = Vec::new();
    for record in &inv.records {
        let warnings = record_warnings(record);
        if warnings.is_empty() {
            continue;
        }
        let concerns = record_concerns_system(record, system);
        for warning in warnings {
            scopes.push((warning, concerns));
        }
    }
    Some(scopes)
}

/// Whether a warning is about the selected system using structured `strict` inventory records:
/// - with no selection (`None`) or when inventory failed to collect, every warning counts;
/// - when one or more `abort-on-warn` records carried the warning, counts iff any carrying record
///   concerns `system` (via `record_concerns_system`, never by message text);
/// - falls back to counting when no record carries the warning.
pub fn warning_concerns_system(
    message: &str,
    scopes: Option<&[(String, bool)]>,
    system: Option<&str>,
) -> bool {
    if system.is_none() {
        return true;
    }
    let Some(scopes) = scopes else {
        return true;
    };
    let mut matched = scopes.iter().filter(|(msg, _)| msg == message).peekable();
    matched.peek().is_none() || matched.any(|(_, concerns)| *concerns)
}

fn run_pass(session: &mut Session, input: &Input, abort_on_warn: bool) -> Pass {
    session.set_abort_on_warn(abort_on_warn);
    // Always every system: `--system` is a restricted setting a daemon that does not trust the
    // client refuses (with a warning no table lists), so one system is selected afterwards from the
    // structured attribute inventory (`gate::system`).
    let rest = [
        "--no-build",
        "--no-update-lock-file",
        "--all-systems",
        input.flake_ref,
    ];
    let run = session.judge(&["flake", "check"], &rest);
    let check_text = format!("{}\n{}", run.stdout, run.stderr);
    let raw_inventory = inventory::collect(session, input.flake_ref);
    let check_ok = !check_failed_for_system(run.ok, &raw_inventory, input.system);
    let warning_scopes = warning_scopes_from_inventory(&raw_inventory, input.system);
    let inventory = raw_inventory.map(|inv| only_system(inv, input.system));
    Pass {
        check_ok,
        check_text,
        inventory,
        warning_scopes,
    }
}

fn error_texts(pass: &Pass) -> Vec<String> {
    let records = pass.inventory.iter().flat_map(|inv| inv.records.iter());
    records.filter_map(|r| r.error.clone()).collect()
}

/// Whether `abort-on-warn` is what stopped this pass: nix says so, or (the text differs on a cold
/// store) an evaluation warning was printed in an error's text.
fn stopped_by_warning(pass: &Pass) -> bool {
    let says = |text: &str| text.contains(ABORT_MARKER) || text.contains(WARNING_PREFIX);
    says(&pass.check_text) || error_texts(pass).iter().any(|e| says(e))
}

/// Whether a text says an output needs a derivation built during evaluation. Warm, nix says
/// `import-from-derivation`; on a cold store it says the derivation `path '...-x.drv' is not valid`.
fn needs_build_during_evaluation(text: &str) -> bool {
    text.contains(IFD_MARKER)
        || text
            .lines()
            .any(|l| l.contains(".drv") && l.contains("is not valid"))
}

/// Every `evaluation warning:` line of a text, in order, once each.
pub fn evaluation_warnings(text: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for line in text.lines() {
        let message = line.trim().strip_prefix(WARNING_PREFIX);
        if let Some(message) = message.filter(|m| !found.iter().any(|f| f == m)) {
            found.push(message.to_string());
        }
    }
    found
}

/// `config.warnings` of every configuration the inventory evaluated.
fn configuration_warnings(session: &Session, inv: &Inventory) -> Vec<String> {
    let configs = inv
        .records
        .iter()
        .filter(|r| r.kind == Kind::Configuration && !r.is_error());
    let mut found = Vec::new();
    for record in configs {
        let path: Vec<&str> = record.attr.split('.').collect();
        let value = inventory::eval_json(session, &inv.flake, &path, &inventory::config_warnings());
        let list = value
            .ok()
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();
        found.extend(list.iter().filter_map(Value::as_str).map(str::to_string));
    }
    found
}

fn judged(
    source: WarningSource,
    message: String,
    input: &Input,
    acks: &mut Vec<AckEntry>,
) -> WarningEntry {
    match ledger::matching(input.ledger, &message, input.today) {
        Some(entry) => {
            let ack = entry.ack(input.today);
            if !acks.contains(&ack) {
                acks.push(ack.clone());
            }
            WarningEntry {
                source,
                message,
                verdict: Verdict::Ack,
                entry: Some(ack),
            }
        }
        None => WarningEntry {
            source,
            message,
            verdict: Verdict::Refuse,
            entry: None,
        },
    }
}

fn diagnostic_entries(text: &str, input: &Input) -> Vec<WarningEntry> {
    let lines = diag::classify(input.diagnostics, input.implementation, text);
    lines
        .into_iter()
        .map(|c| {
            let verdict = if c.effect == Some(Effect::Report) {
                Verdict::Pass
            } else {
                Verdict::Refuse
            };
            let message = match &c.id {
                Some(id) => format!("{} [{id}]", c.line),
                None => format!("{} [unclassified diagnostic]", c.line),
            };
            WarningEntry {
                source: WarningSource::Diagnostic,
                message,
                verdict,
                entry: None,
            }
        })
        .collect()
}

fn output_entries(inv: &Inventory) -> Vec<OutputEntry> {
    let ok = inv
        .records
        .iter()
        .filter(|r| !r.is_error() && r.level != Level::Listed);
    // The level is the inventory's: `evaluated`, or `instantiated` for an output that is not a
    // derivation (protocol section 6.1).
    ok.map(|r| OutputEntry {
        attr: r.attr.clone(),
        level: r.level.name(),
        deferred: None,
    })
    .collect()
}

fn first_lines(text: &str) -> String {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .take(6)
        .collect();
    lines.join(" | ")
}

/// The root cause of a nix error: the last `error:` line that says something, and the lines after
/// it. Nix prints the trace first and the message last. Without one, the first lines.
fn root_cause(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let says = |l: &&str| {
        l.trim_start()
            .strip_prefix("error:")
            .is_some_and(|rest| !rest.trim().is_empty())
    };
    match lines.iter().rposition(says) {
        Some(at) => first_lines(
            &lines[at..]
                .iter()
                .take(3)
                .copied()
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        None => first_lines(text),
    }
}

fn ifd_failure(pass: &Pass) -> Option<String> {
    let in_check = needs_build_during_evaluation(&pass.check_text);
    let ifd = in_check
        || error_texts(pass)
            .iter()
            .any(|e| needs_build_during_evaluation(e));
    ifd.then(|| "N-07: import from derivation is off and an output needs it".to_string())
}

fn check_failure(pass: &Pass) -> Option<String> {
    let text = root_cause(&pass.check_text);
    (!pass.check_ok).then(|| format!("flake check failed: {text}"))
}

fn inventory_failures(
    inventory: &Result<Inventory, String>,
    outputs: &[OutputEntry],
) -> Vec<String> {
    let inv = match inventory {
        Ok(inv) => inv,
        Err(error) => {
            return vec![format!(
                "the outputs could not be listed: {}",
                root_cause(error)
            )];
        }
    };
    let broken: Vec<&Record> = inv.records.iter().filter(|r| r.is_error()).collect();
    let mut found: Vec<String> = broken
        .iter()
        .map(|r| {
            format!(
                "{} failed: {}",
                r.attr,
                root_cause(r.error.as_deref().unwrap_or(""))
            )
        })
        .collect();
    if outputs.is_empty() && broken.is_empty() {
        found.push("no output evaluated".to_string());
    }
    found
}

fn failures(pass: &Pass, outputs: &[OutputEntry]) -> Vec<String> {
    let mut found: Vec<String> = ifd_failure(pass)
        .into_iter()
        .chain(check_failure(pass))
        .collect();
    found.extend(inventory_failures(&pass.inventory, outputs));
    found
}

/// The flake's own path as `.`: a report names files relative to the flake, not the machine.
fn scrub(text: &str, flake_ref: &str) -> String {
    text.replace(flake_ref, ".")
}

/// Runs gate 3.
pub fn run(session: &mut Session, input: &Input) -> Outcome {
    let strict = run_pass(session, input, true);
    let strict_scopes = strict.warning_scopes.clone();
    let lenient = stopped_by_warning(&strict).then(|| run_pass(session, input, false));
    let pass = lenient.unwrap_or(strict);
    let mut acks = Vec::new();
    let mut warnings: Vec<WarningEntry> = Vec::new();
    for message in evaluation_warnings(&pass.check_text)
        .into_iter()
        .filter(|m| warning_concerns_system(m, strict_scopes.as_deref(), input.system))
    {
        warnings.push(judged(
            WarningSource::BuiltinsWarn,
            message,
            input,
            &mut acks,
        ));
    }
    for message in pass
        .inventory
        .iter()
        .flat_map(|inv| configuration_warnings(session, inv))
    {
        warnings.push(judged(
            WarningSource::ConfigWarnings,
            message,
            input,
            &mut acks,
        ));
    }
    warnings.extend(diagnostic_entries(&pass.check_text, input));
    let outputs = pass
        .inventory
        .as_ref()
        .map(output_entries)
        .unwrap_or_default();
    let mut reasons = failures(&pass, &outputs);
    let refused = warnings.iter().filter(|w| w.verdict == Verdict::Refuse);
    reasons.extend(refused.map(|w| format!("{}: {}", w.source_name(), w.message)));
    let reasons: Vec<String> = reasons.iter().map(|r| scrub(r, input.flake_ref)).collect();
    for warning in &mut warnings {
        warning.message = scrub(&warning.message, input.flake_ref);
    }
    let layer = if reasons.is_empty() {
        Layer::new(LayerId::Eval, Verdict::Pass, None)
    } else {
        Layer::new(LayerId::Eval, Verdict::Refuse, Some(reasons.join("\n")))
    };
    Outcome {
        layer,
        warnings,
        outputs,
        acks,
    }
}

impl WarningEntry {
    fn source_name(&self) -> &'static str {
        match self.source {
            WarningSource::BuiltinsWarn => "builtins.warn",
            WarningSource::ConfigWarnings => "config.warnings",
            WarningSource::Diagnostic => "diagnostic",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluation_warnings_are_found_once_each_in_order() {
        let text = "x\nevaluation warning: first\n  evaluation warning: second\nevaluation warning: first\nwarning: not this\n";
        assert_eq!(evaluation_warnings(text), ["first", "second"]);
        assert!(evaluation_warnings("nothing\n").is_empty());
    }

    #[test]
    fn the_root_cause_is_the_last_error_line_with_text() {
        let trace = "error:\n  … while checking\n  at x\nerror: The option 'a.b' does not exist.\nDefinition values:\n- In 'f'\nmore\n";
        assert_eq!(
            root_cause(trace),
            "error: The option 'a.b' does not exist. | Definition values: | - In 'f'"
        );
        assert_eq!(root_cause("checking a\nerror: boom\n"), "error: boom");
        assert_eq!(
            root_cause("no error here\nsecond"),
            "no error here | second"
        );
        assert_eq!(
            root_cause("error:\n  only a trace\n"),
            "error: | only a trace"
        );
    }

    #[test]
    fn scrub_replaces_the_flake_path_with_a_dot() {
        assert_eq!(
            scrub("at /a/b/flake.nix:1 and git+file:///a/b?dir=x", "/a/b"),
            "at ./flake.nix:1 and git+file://.?dir=x"
        );
    }

    #[test]
    fn warning_scopes_scope_by_structured_records_not_message_text() {
        let inv = Ok(Inventory {
            flake: "path:/x".into(),
            structure: Default::default(),
            records: vec![
                // w1: selected x86_64-linux package warns naming aarch64-linux -> MUST count.
                Record::failed(
                    "packages.x86_64-linux.w1",
                    Kind::Package,
                    Some("x86_64-linux"),
                    "evaluation warning: use the aarch64-linux build\nerror: aborting".into(),
                ),
                // w2: other aarch64-linux package warns naming x86_64-linux -> MUST NOT count.
                Record::failed(
                    "packages.aarch64-linux.w2",
                    Kind::Package,
                    Some("aarch64-linux"),
                    "evaluation warning: not for x86_64-linux\nerror: aborting".into(),
                ),
                // w4: other aarch64-linux package warns naming no system -> MUST NOT count.
                Record::failed(
                    "packages.aarch64-linux.w4",
                    Kind::Package,
                    Some("aarch64-linux"),
                    "evaluation warning: plain warning\nerror: aborting".into(),
                ),
                // w5: systemless overlay warns -> MUST count.
                Record::failed(
                    "overlays.default",
                    Kind::Overlay,
                    None,
                    "evaluation warning: overlay warning\nerror: aborting".into(),
                ),
                // non-warning record (clean) is skipped when extracting warning scopes.
                Record::new(
                    "packages.x86_64-linux.ok",
                    Kind::Package,
                    Some("x86_64-linux"),
                    Level::Evaluated,
                ),
            ],
        });
        let scopes = warning_scopes_from_inventory(&inv, Some("x86_64-linux"));
        let sc = scopes.as_deref();
        assert!(warning_concerns_system(
            "use the aarch64-linux build",
            sc,
            Some("x86_64-linux")
        ));
        assert!(!warning_concerns_system(
            "not for x86_64-linux",
            sc,
            Some("x86_64-linux")
        ));
        assert!(!warning_concerns_system(
            "plain warning",
            sc,
            Some("x86_64-linux")
        ));
        assert!(warning_concerns_system(
            "overlay warning",
            sc,
            Some("x86_64-linux")
        ));
        // Unattributed warning falls back to counting; None system / Err inventory also count.
        assert!(warning_concerns_system(
            "unattributed warning",
            sc,
            Some("x86_64-linux")
        ));
        assert!(warning_concerns_system("not for x86_64-linux", sc, None));
        assert!(warning_concerns_system(
            "plain warning",
            None,
            Some("x86_64-linux")
        ));
        assert!(warning_scopes_from_inventory(&Err("bad".into()), Some("x86_64-linux")).is_none());
    }

    #[test]
    fn first_lines_joins_the_first_six_non_empty_lines() {
        assert_eq!(
            first_lines("a\n\nb\nc\nd\ne\nf\ng"),
            "a | b | c | d | e | f"
        );
        assert_eq!(first_lines(""), "");
    }
}
