//! Gate 3, eval (protocol section 3.1): `nix flake check --no-build --all-systems` under
//! `abort-on-warn` with import from derivation off, the inventory of every output (so one error
//! does not hide the others), the configuration build roots, `config.warnings` read as data, the
//! `nix` command's own diagnostics classified by `laws/diagnostics.toml`, and the ledger.

use super::{
    diag::{self, Effect},
    ledger::{self, Entry},
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
}

fn run_pass(session: &mut Session, input: &Input, abort_on_warn: bool) -> Pass {
    session.set_abort_on_warn(abort_on_warn);
    // Always every system: `--system` is a restricted setting a daemon that does not trust the
    // client refuses (with a warning no table lists), so one system is selected afterwards, by
    // what the check says and by the inventory.
    let rest = [
        "--no-build",
        "--no-update-lock-file",
        "--all-systems",
        input.flake_ref,
    ];
    let run = session.judge(&["flake", "check"], &rest);
    let check_text = format!("{}\n{}", run.stdout, run.stderr);
    let inventory =
        inventory::collect(session, input.flake_ref).map(|inv| only_system(inv, input.system));
    Pass {
        // With a system selected, a failure of `flake check` that is about an attribute on
        // another system is not this run's (the same rule as for a warning); one that names the
        // selected system, or no system at all, counts.
        check_ok: run.ok || !error_concerns(&run.stderr, input.system),
        check_text,
        inventory,
    }
}

/// The systems the kit supports, and the one it dropped: how an error is told to be about one.
const SYSTEMS: [&str; 4] = [
    "aarch64-darwin",
    "x86_64-linux",
    "aarch64-linux",
    "x86_64-darwin",
];

/// Whether a warning is about the selected system. With no selection every warning counts. With
/// one, a warning that names only other systems does not (it is about an output on another
/// system); one that names the selected system, or names no system at all (it cannot be tied to
/// an output), does.
fn concerns(text: &str, system: Option<&str>) -> bool {
    match system {
        None => true,
        Some(wanted) => text.contains(wanted) || !SYSTEMS.iter().any(|s| text.contains(s)),
    }
}

/// The system a record is for: its own, or, for an output kept per system, the second component
/// of its attribute path (`packages.<system>.<name>`).
fn record_system(record: &Record) -> Option<&str> {
    if record.system.is_some() {
        return record.system.as_deref();
    }
    let per_system = !matches!(
        record.kind,
        Kind::Overlay
            | Kind::Template
            | Kind::Module
            | Kind::Lib
            | Kind::Configuration
            | Kind::Unknown
    );
    per_system.then(|| record.attr.split('.').nth(1)).flatten()
}

/// The error of a `flake check` run: from the first line that says `error` on, without the
/// progress lines before it (they name every attribute checked, whatever its system).
fn error_block(stderr: &str) -> String {
    let lines: Vec<&str> = stderr.lines().collect();
    let first = lines
        .iter()
        .position(|l| l.trim_start().starts_with("error"));
    lines[first.unwrap_or(0)..].join("\n")
}

/// Whether a line of an error names an attribute: a trace line (`… while checking the derivation
/// 'packages.aarch64-linux.default'`) or an `error:` line. The source excerpts under them are
/// code and may quote anything, so they are not read.
fn names_attribute(line: &str) -> bool {
    let line = line.trim_start();
    line.starts_with("error:") || (line.starts_with('…') && line.contains("while "))
}

/// The systems of the attributes an error names between single quotes, on those lines only.
fn quoted_attribute_systems(error: &str) -> Vec<&'static str> {
    let mut found: Vec<&'static str> = Vec::new();
    for line in error.lines().filter(|l| names_attribute(l)) {
        for token in line.split('\'').skip(1).step_by(2) {
            for part in token.split('.') {
                if let Some(system) = SYSTEMS.iter().find(|s| **s == part) {
                    found.push(system);
                }
            }
        }
    }
    found
}

/// The outputs a Determinate `flake check` marks failed: its `❌ <attribute>` lines, which come
/// before the error and name each failing attribute.
fn failed_attributes(stderr: &str) -> Vec<&str> {
    let marked = stderr
        .lines()
        .filter_map(|l| l.trim_start().strip_prefix('❌'));
    marked.map(str::trim).collect()
}

/// Whether an attribute path is on the wanted system, or on none (an output without a system level
/// such as a configuration counts for every run).
fn on_system(attribute: &str, wanted: &str) -> bool {
    let systems: Vec<&str> = attribute
        .split('.')
        .filter(|p| SYSTEMS.contains(p))
        .collect();
    systems.is_empty() || systems.contains(&wanted)
}

/// Whether a failure of `flake check` is about the selected system. With no selection always.
/// With one: when the check names its failed attributes (`❌` lines), by those; otherwise by the
/// attributes the error quotes (`while checking the derivation '...'`), unless all are on other
/// systems. An error that names no attribute counts. The source excerpts under an error name
/// other systems too, so only these are read.
fn error_concerns(stderr: &str, system: Option<&str>) -> bool {
    let Some(wanted) = system else {
        return true;
    };
    let failed = failed_attributes(stderr);
    if !failed.is_empty() {
        return failed.iter().any(|a| on_system(a, wanted));
    }
    let named = quoted_attribute_systems(&error_block(stderr));
    named.is_empty() || named.contains(&wanted)
}

fn only_system(mut inv: Inventory, system: Option<&str>) -> Inventory {
    if let Some(wanted) = system {
        inv.records
            .retain(|r| record_system(r).is_none_or(|s| s == wanted));
    }
    inv
}

fn error_texts(pass: &Pass) -> Vec<String> {
    let records = pass.inventory.iter().flat_map(|inv| inv.records.iter());
    records.filter_map(|r| r.error.clone()).collect()
}

/// Whether `abort-on-warn` is what stopped this pass.
fn stopped_by_warning(pass: &Pass) -> bool {
    pass.check_text.contains(ABORT_MARKER)
        || error_texts(pass).iter().any(|e| e.contains(ABORT_MARKER))
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
    let in_check = pass.check_text.contains(IFD_MARKER);
    let ifd = in_check || error_texts(pass).iter().any(|e| e.contains(IFD_MARKER));
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
    let lenient = stopped_by_warning(&strict).then(|| run_pass(session, input, false));
    let pass = lenient.unwrap_or(strict);
    let mut acks = Vec::new();
    let mut warnings: Vec<WarningEntry> = Vec::new();
    for message in evaluation_warnings(&pass.check_text)
        .into_iter()
        .filter(|m| concerns(m, input.system))
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
        .filter(|m| concerns(m, input.system))
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
    fn an_error_is_about_the_systems_of_the_attributes_it_quotes() {
        let throw = "error:\n  … while checking the derivation 'packages.aarch64-linux.default'\n     6| packages.x86_64-linux.default = a;\n  error: qa broken";
        assert!(!error_concerns(throw, Some("x86_64-linux")));
        assert!(error_concerns(throw, Some("aarch64-linux")));
        assert!(error_concerns(throw, None));
        let none = "error: something broke at packages.x86_64-linux.a";
        assert!(error_concerns(none, Some("aarch64-linux")));
        assert!(error_concerns("", Some("x86_64-linux")));
        let both = "error: 'checks.x86_64-linux.a' and 'checks.aarch64-linux.b'";
        assert!(
            error_concerns(both, Some("x86_64-linux"))
                && error_concerns(both, Some("aarch64-linux"))
        );
        assert!(!error_concerns(
            "error: 'a.x86_64-darwin.b'",
            Some("x86_64-linux")
        ));
    }

    /// QA's q4: another system's broken output quotes the selected system's name in its source
    /// excerpt (CppNix prints no per-attribute lines, so the trace lines decide).
    #[test]
    fn a_system_quoted_in_a_source_excerpt_does_not_make_the_error_the_selected_systems() {
        let q4 = "error:\n       … while checking the derivation 'packages.aarch64-linux.default'\n         at /s/flake.nix:6:5:\n            5|     packages.aarch64-linux.default = throw \"x\"; /* 'x86_64-linux' */\n            6|     packages.x86_64-linux.default = a;\n\n       error: x";
        assert!(!error_concerns(q4, Some("x86_64-linux")));
        assert!(error_concerns(q4, Some("aarch64-linux")));
        let message = "error: flake attribute 'checks.aarch64-linux.bad' is not a derivation";
        assert!(!error_concerns(message, Some("x86_64-linux")));
    }

    #[test]
    fn a_determinate_check_names_its_failed_attributes_and_they_decide() {
        let text = "✅ packages.x86_64-linux.default (build skipped)\n❌ packages.aarch64-linux.default\nerror:\n  at flake.nix\n  packages.x86_64-linux.default = a;\n  error: qa broken";
        assert!(!error_concerns(text, Some("x86_64-linux")));
        assert!(error_concerns(text, Some("aarch64-linux")));
        let both = "❌ checks.x86_64-linux.a\n❌ checks.aarch64-linux.b\nerror: x";
        assert!(
            error_concerns(both, Some("x86_64-linux"))
                && error_concerns(both, Some("aarch64-linux"))
        );
        // A failed output without a system level counts for every selection.
        assert!(error_concerns(
            "❌ nixosConfigurations.host\nerror: x",
            Some("x86_64-linux")
        ));
        assert_eq!(failed_attributes("a\n  ❌ b.c\n❌d"), ["b.c", "d"]);
    }

    #[test]
    fn the_error_block_starts_at_the_first_error_line() {
        assert_eq!(
            error_block(
                "checking derivation packages.x86_64-linux.a\nerror: boom\n  at packages.aarch64-linux.b"
            ),
            "error: boom\n  at packages.aarch64-linux.b"
        );
        assert_eq!(error_block("no error line"), "no error line");
        assert_eq!(error_block(""), "");
    }

    #[test]
    fn a_records_system_is_its_own_or_the_second_part_of_a_per_system_path() {
        let own = Record::new(
            "packages.aarch64-linux.a",
            Kind::Package,
            Some("x86_64-linux"),
            Level::Evaluated,
        );
        assert_eq!(record_system(&own), Some("x86_64-linux"));
        let derived = Record::new("checks.aarch64-linux.a", Kind::Check, None, Level::Error);
        assert_eq!(record_system(&derived), Some("aarch64-linux"));
        let apps = Record::new(
            "apps.x86_64-linux.default",
            Kind::App,
            None,
            Level::Evaluated,
        );
        assert_eq!(record_system(&apps), Some("x86_64-linux"));
        for (attr, kind) in [
            ("overlays.default", Kind::Overlay),
            ("nixosModules.m", Kind::Module),
            ("templates.t", Kind::Template),
            ("lib", Kind::Lib),
            ("nixosConfigurations.h", Kind::Configuration),
            ("fooBar", Kind::Unknown),
        ] {
            assert_eq!(
                record_system(&Record::new(attr, kind, None, Level::Evaluated)),
                None,
                "{attr}"
            );
        }
    }

    #[test]
    fn a_warning_is_about_the_selected_system_when_it_names_it_or_names_none() {
        assert!(concerns("error: boom", None));
        assert!(concerns("error: packages.x86_64-darwin.a", None));
        assert!(concerns(
            "error: packages.x86_64-linux.a",
            Some("x86_64-linux")
        ));
        assert!(concerns("error: no system named", Some("x86_64-linux")));
        assert!(!concerns(
            "error: checks.aarch64-darwin.a",
            Some("x86_64-linux")
        ));
        assert!(concerns(
            "error: a.aarch64-darwin and a.x86_64-linux",
            Some("x86_64-linux")
        ));
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
