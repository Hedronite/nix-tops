//! Baseline writing and `ratchet` comparison (M2.3).
//!
//! Laid out as a seam so baseline/ratchet logic can be implemented in `src/baseline/**` and
//! `src/cmd/baseline.rs` without editing shared dispatch files.

use crate::report::{LintReport, Report};
use serde_json::Value;
use std::path::Path;

/// Evaluates `lint` against the tracked baseline under `enforce = "ratchet"`.
/// Returns `Ok(())` when within baseline (or bootstrapping with `writing_baseline`), or
/// `Err(reason)` when a count rises or the config narrowed.
pub fn judge_lint(
    _root: &Path,
    _doc: &Value,
    _lint: &LintReport,
    writing_baseline: bool,
) -> Result<(), String> {
    if writing_baseline {
        Ok(())
    } else {
        Err("ratchet not built yet".to_string())
    }
}

/// Applies cross-layer baseline checks (ack count, judged-law set) under `enforce = "ratchet"`.
pub fn ratchet_report(_root: &Path, _doc: &Value, _report: &mut Report, _writing_baseline: bool) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::{ALL_LAYERS, BaselineIdentity, Layer, Parts, Verdict};
    use serde_json::json;

    #[test]
    fn stub_ratchet_hooks_are_no_ops_until_ratchet_lands() {
        let lint = LintReport {
            system: "x86_64-linux".into(),
            tools: vec![],
        };
        assert_eq!(
            judge_lint(Path::new("."), &json!({}), &lint, false),
            Err("ratchet not built yet".to_string())
        );
        assert!(judge_lint(Path::new("."), &json!({}), &lint, true).is_ok());

        let mut report = Report::finish(Parts {
            protocol: crate::build_info::PROTOCOL.into(),
            enforce: "ratchet".into(),
            baseline_identity: BaselineIdentity {
                implementation: "cppnix".into(),
                version: "9.9.9".into(),
                system: "x86_64-linux".into(),
                lock_hash: "sha256-x".into(),
            },
            facets: vec!["package-set".into()],
            layers: ALL_LAYERS
                .into_iter()
                .map(|id| Layer::new(id, Verdict::Pass, None))
                .collect(),
            outputs: vec![],
            warnings: vec![],
            acks: vec![],
            lint: Some(lint),
        });
        ratchet_report(Path::new("."), &json!({}), &mut report, false);
        assert!(report.passed);
    }
}
