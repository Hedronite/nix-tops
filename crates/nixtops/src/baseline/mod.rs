//! The `ratchet` comparison and the file it compares against (protocol section 6.8).
//!
//! The gate calls [`judge_lint`] from the lint layer and [`ratchet_report`] once, after the
//! layers. `nixtops baseline` uses the same [`compare::judge`], so what it records can never be
//! looser than what the gate accepts.

pub mod compare;
pub mod file;
pub mod history;
pub mod inputs;
pub mod sha256;
pub mod widen;

use crate::{
    config::{baseline::real_git, semantic},
    report::{LayerId, LintReport, Report, Verdict},
};
use compare::Judgement;
use file::Baseline;
use serde_json::Value;
use std::path::Path;

fn load(root: &Path, doc: &Value) -> Result<Option<Baseline>, String> {
    file::load(&root.join(semantic::baseline_path(doc)))
}

/// Whether a file of the flake is unchanged since the baseline was last written.
pub fn since_last_write(root: &Path, doc: &Value) -> impl Fn(&str) -> bool {
    let git = real_git(root);
    let base = history::last_write(&git, semantic::baseline_path(doc));
    move |file: &str| {
        base.as_deref()
            .is_some_and(|commit| history::unchanged(&git, commit, None, file))
    }
}

/// The committed baseline held against the version before it: nobody but `nixtops baseline`
/// writes it, and that never loosens it.
fn against_prior(root: &Path, doc: &Value, current: &Baseline) -> Result<(), String> {
    let git = real_git(root);
    let path = semantic::baseline_path(doc);
    let Some(prior) = history::prior(&git, path)? else {
        return Ok(());
    };
    let at = prior.commit.get(..12).unwrap_or(&prior.commit);
    let old = file::parse(&prior.text)
        .map_err(|e| format!("{path} at {at} is not a valid baseline: {e}"))?;
    let unchanged =
        |file: &str| history::unchanged(&git, &prior.commit, Some(&prior.current), file);
    let found = compare::judge(doc, &old, current, &unchanged);
    if found.failures.is_empty() {
        return Ok(());
    }
    Err(format!(
        "{path} is looser than its version at {at}: {}",
        found.all()
    ))
}

fn missing(doc: &Value) -> String {
    format!("{} does not exist", semantic::baseline_path(doc))
}

/// The lint half alone: what the lint layer knows when it asks.
fn lint_only(root: &Path, doc: &Value, lint: &LintReport, old: Baseline) -> Result<(), String> {
    let now = Baseline::of(root, doc, &report_of(lint));
    compare::judge(doc, &old, &now, &since_last_write(root, doc))
        .reason(LayerId::Lint)
        .map_or(Ok(()), Err)
}

/// A report holding only the lint findings.
fn report_of(lint: &LintReport) -> Report {
    let identity = crate::report::BaselineIdentity {
        implementation: String::new(),
        version: String::new(),
        system: String::new(),
        lock_hash: String::new(),
    };
    Report::finish(crate::report::Parts {
        protocol: String::new(),
        enforce: String::new(),
        baseline_identity: identity,
        facets: Vec::new(),
        layers: Vec::new(),
        outputs: Vec::new(),
        warnings: Vec::new(),
        acks: Vec::new(),
        lint: Some(lint.clone()),
    })
}

/// Evaluates `lint` against the baseline under `enforce = "ratchet"`. `Err` names every rise and
/// every narrowed config. With no baseline, only the run that writes the first one passes.
pub fn judge_lint(
    root: &Path,
    doc: &Value,
    lint: &LintReport,
    writing_baseline: bool,
) -> Result<(), String> {
    match load(root, doc)? {
        Some(old) => lint_only(root, doc, lint, old),
        None if writing_baseline => Ok(()),
        None => Err(missing(doc)),
    }
}

fn verdict_of(report: &Report, id: LayerId) -> Option<Verdict> {
    report.layers.iter().find(|l| l.id == id).map(|l| l.verdict)
}

/// Sets a layer to `Refuse` with `reason`, keeping the reason it already gave when it failed.
fn refuse(report: &mut Report, id: LayerId, reason: &str) {
    for layer in report.layers.iter_mut().filter(|l| l.id == id) {
        let kept = layer.reason.take().filter(|_| layer.verdict.fails());
        layer.reason = Some(match kept {
            Some(old) if old.contains(reason) => old,
            Some(old) => format!("{old}; {reason}"),
            None => reason.to_string(),
        });
        layer.verdict = Verdict::Refuse;
    }
}

/// A passing lint layer says why a re-baseline is due.
fn annotate(report: &mut Report, notes: &[String]) {
    if notes.is_empty() {
        return;
    }
    for layer in &mut report.layers {
        if layer.id == LayerId::Lint && layer.verdict == Verdict::Pass {
            layer.reason = Some(format!("re-baseline: {}", notes.join("; ")));
        }
    }
}

/// The baseline judged against the whole report. Layers that did not run have nothing to compare.
fn judge_report(root: &Path, doc: &Value, report: &Report, old: &Baseline) -> Judgement {
    let mut now = Baseline::of(root, doc, report);
    let lint = verdict_of(report, LayerId::Lint);
    if report.lint.is_none() && lint != Some(Verdict::Skip) {
        now.lint = old.lint.clone();
    }
    compare::judge(doc, old, &now, &since_last_write(root, doc))
}

fn apply(report: &mut Report, found: &Judgement) {
    for id in [LayerId::Lint, LayerId::Eval, LayerId::Laws] {
        if let Some(reason) = found.reason(id) {
            refuse(report, id, &reason);
        }
    }
    annotate(report, &found.notes);
}

/// Applies the whole comparison under `enforce = "ratchet"`: the ack count, the judged laws, a
/// lint layer switched off since the baseline, and the re-baseline message.
pub fn ratchet_report(root: &Path, doc: &Value, report: &mut Report, writing_baseline: bool) {
    if semantic::enforce_of(doc) != "ratchet" {
        return;
    }
    match load(root, doc) {
        Ok(Some(old)) => {
            let found = judge_report(root, doc, report, &old);
            apply(report, &found);
            if let Err(why) = against_prior(root, doc, &old) {
                refuse(report, LayerId::Lint, &why);
            }
        }
        Ok(None) if writing_baseline => return,
        Ok(None) => refuse(report, LayerId::Lint, &missing(doc)),
        Err(reason) => refuse(report, LayerId::Lint, &reason),
    }
    report.refresh_counts();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::{ALL_LAYERS, AckEntry, BaselineIdentity, Layer, LintToolReport, Parts};
    use serde_json::json;
    use std::{collections::BTreeMap, fs};

    fn lint(count: usize) -> LintReport {
        LintReport {
            system: "x86_64-linux".into(),
            tools: vec![LintToolReport {
                tool: "deadnix".into(),
                version: "1".into(),
                config_hash: sha256::hex(b"fail=true\nexclude:"),
                files: BTreeMap::from([("a.nix".to_string(), count)]),
            }],
        }
    }

    fn report(lint: Option<LintReport>, lint_verdict: Verdict, acks: usize) -> Report {
        let layers = ALL_LAYERS
            .into_iter()
            .map(|id| {
                let verdict = if id == LayerId::Lint {
                    lint_verdict
                } else {
                    Verdict::Pass
                };
                Layer::new(id, verdict, None)
            })
            .collect();
        let ack = AckEntry {
            kind: "ledger",
            matches: "m".into(),
            reason: "r".into(),
            until: "2027-01-01".into(),
            expired: false,
        };
        Report::finish(crate::report::Parts {
            acks: vec![ack; acks],
            layers,
            lint,
            ..parts()
        })
    }

    fn parts() -> Parts {
        Parts {
            protocol: String::new(),
            enforce: "ratchet".into(),
            baseline_identity: BaselineIdentity {
                implementation: "i".into(),
                version: "v".into(),
                system: "s".into(),
                lock_hash: "h".into(),
            },
            facets: vec![],
            layers: vec![],
            outputs: vec![],
            warnings: vec![],
            acks: vec![],
            lint: None,
        }
    }

    /// A root whose baseline records one deadnix finding and no acks.
    fn root() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let first = report(Some(lint(1)), Verdict::Pass, 0);
        let mut base = Baseline::of(dir.path(), &json!({}), &first);
        base.laws.clear();
        file::write(&dir.path().join(semantic::DEFAULT_BASELINE), &base).expect("write");
        dir
    }

    fn reason(report: &Report, id: LayerId) -> (Verdict, String) {
        let layer = report.layers.iter().find(|l| l.id == id).expect("layer");
        (layer.verdict, layer.reason.clone().unwrap_or_default())
    }

    fn run(dir: &Path, doc: &Value, mut got: Report) -> Report {
        ratchet_report(dir, doc, &mut got, false);
        got
    }

    #[test]
    fn judge_lint_compares_with_the_file_and_needs_it_unless_writing() {
        let dir = root();
        let doc = json!({});
        assert_eq!(judge_lint(dir.path(), &doc, &lint(1), false), Ok(()));
        let why = judge_lint(dir.path(), &doc, &lint(2), true).expect_err("rose");
        assert_eq!(why, "deadnix: a.nix: 2 finding(s), baseline 1");
        let empty = tempfile::tempdir().expect("tempdir");
        assert_eq!(judge_lint(empty.path(), &doc, &lint(2), true), Ok(()));
        let why = judge_lint(empty.path(), &doc, &lint(0), false).expect_err("missing");
        assert_eq!(why, ".nix-tops/baseline.json does not exist");
        fs::write(dir.path().join(semantic::DEFAULT_BASELINE), "{}").expect("write");
        let why = judge_lint(dir.path(), &doc, &lint(0), true).expect_err("invalid");
        assert!(why.contains("not a valid baseline"), "{why}");
    }

    #[test]
    fn an_unchanged_run_stays_passed_and_a_rise_in_acks_refuses_eval() {
        let dir = root();
        let doc = json!({});
        let same = run(dir.path(), &doc, report(Some(lint(1)), Verdict::Pass, 0));
        assert!(same.passed);
        assert_eq!(reason(&same, LayerId::Lint), (Verdict::Pass, String::new()));
        let acked = run(dir.path(), &doc, report(Some(lint(1)), Verdict::Pass, 1));
        assert!(!acked.passed);
        assert_eq!(acked.counts.failed, 1);
        assert_eq!(
            reason(&acked, LayerId::Eval),
            (
                Verdict::Refuse,
                "the ack count rose from 0 to 1".to_string()
            )
        );
    }

    #[test]
    fn a_lint_layer_switched_off_after_the_baseline_refuses_and_one_not_run_is_left() {
        let dir = root();
        let doc = json!({});
        let off = run(dir.path(), &doc, report(None, Verdict::Skip, 0));
        assert_eq!(
            reason(&off, LayerId::Lint),
            (
                Verdict::Refuse,
                "deadnix: in the baseline and not run now".to_string()
            )
        );
        assert!(!off.passed);
        let left = run(dir.path(), &doc, report(None, Verdict::NotRun, 0));
        assert_eq!(
            reason(&left, LayerId::Lint),
            (Verdict::NotRun, String::new())
        );
    }

    #[test]
    fn a_missing_or_broken_baseline_refuses_unless_it_is_being_written() {
        let empty = tempfile::tempdir().expect("tempdir");
        let doc = json!({});
        let got = run(empty.path(), &doc, report(Some(lint(0)), Verdict::Pass, 0));
        assert_eq!(reason(&got, LayerId::Lint).0, Verdict::Refuse);
        assert!(!got.passed);
        let mut boot = report(Some(lint(3)), Verdict::Pass, 2);
        ratchet_report(empty.path(), &doc, &mut boot, true);
        assert!(boot.passed);
        let hard = json!({"enforce": "hard"});
        let ignored = run(empty.path(), &hard, report(Some(lint(0)), Verdict::Pass, 0));
        assert!(ignored.passed);
        let dir = root();
        fs::write(dir.path().join(semantic::DEFAULT_BASELINE), "[]").expect("write");
        let broken = run(dir.path(), &doc, report(None, Verdict::Skip, 0));
        assert!(
            reason(&broken, LayerId::Lint)
                .1
                .contains("not a valid baseline")
        );
        assert!(!broken.passed);
    }

    #[test]
    fn a_refusal_keeps_what_the_failed_layer_already_said_once() {
        let mut got = report(None, Verdict::Refuse, 0);
        let set = |r: &mut Report, text: &str| {
            for layer in r.layers.iter_mut().filter(|l| l.id == LayerId::Lint) {
                layer.reason = Some(text.to_string());
            }
        };
        set(&mut got, "x: rose");
        refuse(&mut got, LayerId::Lint, "x: rose");
        assert_eq!(reason(&got, LayerId::Lint).1, "x: rose");
        refuse(&mut got, LayerId::Lint, "y");
        assert_eq!(reason(&got, LayerId::Lint).1, "x: rose; y");
        let mut passed = report(None, Verdict::Pass, 0);
        set(&mut passed, "advisory note");
        refuse(&mut passed, LayerId::Lint, "y");
        assert_eq!(reason(&passed, LayerId::Lint).1, "y");
    }

    #[test]
    fn a_note_is_added_only_to_a_passing_lint_layer() {
        let notes = ["a".to_string(), "b".to_string()];
        let mut got = report(None, Verdict::Pass, 0);
        annotate(&mut got, &notes);
        assert_eq!(reason(&got, LayerId::Lint).1, "re-baseline: a; b");
        assert_eq!(reason(&got, LayerId::Eval).1, "");
        let mut failed = report(None, Verdict::Refuse, 0);
        annotate(&mut failed, &notes);
        assert_eq!(reason(&failed, LayerId::Lint).1, "");
    }
}
