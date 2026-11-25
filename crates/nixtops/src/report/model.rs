//! The report as the schema (`schema/gate-report.schema.json`) wants it. Every layer is present;
//! a verdict that needs a reason carries one.

use serde::{Serialize, Serializer};
use serde_json::Value;
use std::collections::BTreeMap;

/// A verdict (protocol section 3.3), written as the schema's names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Refuse,
    Ack,
    Skip,
    NotRun,
    NotEvaluated,
    Deferred,
    Vacuous,
}

const VERDICTS: [(Verdict, &str, &str); 8] = [
    (Verdict::Pass, "pass", "pass"),
    (Verdict::Refuse, "Refuse", "refuse"),
    (Verdict::Ack, "Ack", "ack"),
    (Verdict::Skip, "skip", "skip"),
    (Verdict::NotRun, "not-run", "not_run"),
    (Verdict::NotEvaluated, "not-evaluated", "not_evaluated"),
    (Verdict::Deferred, "deferred", "deferred"),
    (Verdict::Vacuous, "vacuous", "vacuous"),
];

impl Verdict {
    /// The name in text and in the `verdict` field.
    pub fn name(self) -> &'static str {
        VERDICTS
            .iter()
            .find(|(v, _, _)| *v == self)
            .map_or("pass", |(_, name, _)| name)
    }

    /// The same name as a JSON key.
    pub fn key(self) -> &'static str {
        VERDICTS
            .iter()
            .find(|(v, _, _)| *v == self)
            .map_or("pass", |(_, _, key)| key)
    }

    /// A layer with this verdict fails the gate.
    pub fn fails(self) -> bool {
        matches!(self, Verdict::Refuse | Verdict::NotRun)
    }
}

impl Serialize for Verdict {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.name())
    }
}

/// The layers of the report, in the order of the gates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerId {
    Source,
    Lint,
    Eval,
    BuildTest,
    Coverage,
    FacetLayers,
    Laws,
}

pub const ALL_LAYERS: [LayerId; 7] = [
    LayerId::Source,
    LayerId::Lint,
    LayerId::Eval,
    LayerId::BuildTest,
    LayerId::Coverage,
    LayerId::FacetLayers,
    LayerId::Laws,
];

impl LayerId {
    pub fn as_str(self) -> &'static str {
        match self {
            LayerId::Source => "source",
            LayerId::Lint => "lint",
            LayerId::Eval => "eval",
            LayerId::BuildTest => "build-test",
            LayerId::Coverage => "coverage",
            LayerId::FacetLayers => "facet-layers",
            LayerId::Laws => "laws",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        ALL_LAYERS.into_iter().find(|id| id.as_str() == text)
    }
}

impl Serialize for LayerId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Layer {
    pub id: LayerId,
    pub verdict: Verdict,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl Layer {
    pub fn new(id: LayerId, verdict: Verdict, reason: Option<String>) -> Self {
        Self {
            id,
            verdict,
            reason,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WarningSource {
    BuiltinsWarn,
    ConfigWarnings,
    Diagnostic,
}

impl Serialize for WarningSource {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            WarningSource::BuiltinsWarn => "builtins-warn",
            WarningSource::ConfigWarnings => "config-warnings",
            WarningSource::Diagnostic => "diagnostic",
        })
    }
}

/// An acknowledgement from the ledger, as the core schema writes it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AckEntry {
    pub kind: &'static str,
    #[serde(rename = "match")]
    pub matches: String,
    pub reason: String,
    pub until: String,
    pub expired: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct WarningEntry {
    pub source: WarningSource,
    pub message: String,
    pub verdict: Verdict,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry: Option<AckEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct OutputEntry {
    pub attr: String,
    pub level: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deferred: Option<bool>,
}

/// Per-tool findings summary from gate 2 (lint).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LintToolReport {
    pub tool: String,
    pub version: String,
    pub config_hash: String,
    pub files: BTreeMap<String, usize>,
}

/// Lint layer results recorded in `report.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LintReport {
    pub system: String,
    pub tools: Vec<LintToolReport>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BaselineIdentity {
    pub implementation: String,
    pub version: String,
    pub system: String,
    pub lock_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Counts {
    pub verdicts: Value,
    pub failed: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Acks {
    pub count: usize,
    pub entries: Vec<AckEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Report {
    pub protocol: String,
    pub enforce: String,
    pub baseline_identity: BaselineIdentity,
    pub facets: Vec<String>,
    pub passed: bool,
    pub counts: Counts,
    pub layers: Vec<Layer>,
    pub laws: Vec<Value>,
    pub outputs: Vec<OutputEntry>,
    pub warnings: Vec<WarningEntry>,
    pub acks: Acks,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lint: Option<LintReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sandbox: Option<String>,
}

/// Every verdict counted on its own, over the layers and the warnings.
fn verdict_counts(layers: &[Layer], warnings: &[WarningEntry]) -> Value {
    let all = layers
        .iter()
        .map(|l| l.verdict)
        .chain(warnings.iter().map(|w| w.verdict));
    let all: Vec<Verdict> = all.collect();
    let counted = VERDICTS.iter().map(|(verdict, _, key)| {
        let n = all.iter().filter(|v| *v == verdict).count();
        ((*key).to_string(), Value::from(n))
    });
    Value::Object(counted.collect())
}

/// What a report is built from; `finish` fills the counts and `passed`.
pub struct Parts {
    pub protocol: String,
    pub enforce: String,
    pub baseline_identity: BaselineIdentity,
    pub facets: Vec<String>,
    pub layers: Vec<Layer>,
    pub outputs: Vec<OutputEntry>,
    pub warnings: Vec<WarningEntry>,
    pub acks: Vec<AckEntry>,
    pub lint: Option<LintReport>,
}

impl Report {
    pub fn finish(parts: Parts) -> Self {
        let failed = parts.layers.iter().filter(|l| l.verdict.fails()).count();
        let counts = Counts {
            verdicts: verdict_counts(&parts.layers, &parts.warnings),
            failed,
        };
        Self {
            protocol: parts.protocol,
            enforce: parts.enforce,
            baseline_identity: parts.baseline_identity,
            facets: parts.facets,
            passed: failed == 0,
            counts,
            layers: parts.layers,
            laws: Vec::new(),
            outputs: parts.outputs,
            warnings: parts.warnings,
            acks: Acks {
                count: parts.acks.len(),
                entries: parts.acks,
            },
            lint: parts.lint,
            sandbox: None,
        }
    }

    pub fn with_sandbox(mut self, sandbox: Option<String>) -> Self {
        self.sandbox = sandbox;
        self
    }

    /// Recomputes `counts` and `passed` after a post-layer pass (such as ratchet).
    pub fn refresh_counts(&mut self) {
        let failed = self.layers.iter().filter(|l| l.verdict.fails()).count();
        self.counts = Counts {
            verdicts: verdict_counts(&self.layers, &self.warnings),
            failed,
        };
        self.passed = failed == 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> BaselineIdentity {
        BaselineIdentity {
            implementation: "cppnix".into(),
            version: "1".into(),
            system: "s".into(),
            lock_hash: "h".into(),
        }
    }

    fn parts(layers: Vec<Layer>, warnings: Vec<WarningEntry>) -> Parts {
        Parts {
            protocol: "P".into(),
            enforce: "hard".into(),
            baseline_identity: identity(),
            facets: vec![],
            layers,
            outputs: vec![],
            warnings,
            acks: vec![],
            lint: None,
        }
    }

    #[test]
    fn verdict_names_and_keys_follow_the_schema() {
        let names: Vec<&str> = VERDICTS.iter().map(|(v, _, _)| v.name()).collect();
        assert_eq!(
            names,
            [
                "pass",
                "Refuse",
                "Ack",
                "skip",
                "not-run",
                "not-evaluated",
                "deferred",
                "vacuous"
            ]
        );
        let keys: Vec<&str> = VERDICTS.iter().map(|(v, _, _)| v.key()).collect();
        assert_eq!(
            keys,
            [
                "pass",
                "refuse",
                "ack",
                "skip",
                "not_run",
                "not_evaluated",
                "deferred",
                "vacuous"
            ]
        );
    }

    #[test]
    fn only_refuse_and_not_run_fail() {
        let failing: Vec<Verdict> = VERDICTS
            .iter()
            .map(|(v, _, _)| *v)
            .filter(|v| v.fails())
            .collect();
        assert_eq!(failing, [Verdict::Refuse, Verdict::NotRun]);
    }

    #[test]
    fn layer_ids_round_trip_in_gate_order() {
        let names: Vec<&str> = ALL_LAYERS.into_iter().map(LayerId::as_str).collect();
        assert_eq!(
            names,
            [
                "source",
                "lint",
                "eval",
                "build-test",
                "coverage",
                "facet-layers",
                "laws"
            ]
        );
        for id in ALL_LAYERS {
            assert_eq!(LayerId::parse(id.as_str()), Some(id));
        }
        assert_eq!(LayerId::parse("other"), None);
    }

    #[test]
    fn finish_counts_every_verdict_and_fails_on_refuse_or_not_run() {
        let layers = vec![
            Layer::new(LayerId::Source, Verdict::Pass, None),
            Layer::new(LayerId::Lint, Verdict::Skip, Some("off".into())),
            Layer::new(LayerId::Eval, Verdict::Refuse, Some("bad".into())),
            Layer::new(LayerId::BuildTest, Verdict::NotRun, Some("stopped".into())),
        ];
        let warnings = vec![WarningEntry {
            source: WarningSource::BuiltinsWarn,
            message: "w".into(),
            verdict: Verdict::Ack,
            entry: None,
        }];
        let mut report = Report::finish(parts(layers, warnings));
        assert!(!report.passed);
        assert_eq!(report.counts.failed, 2);
        assert_eq!(report.counts.verdicts["pass"], 1);
        assert_eq!(report.counts.verdicts["skip"], 1);
        assert_eq!(report.counts.verdicts["refuse"], 1);
        assert_eq!(report.counts.verdicts["not_run"], 1);
        assert_eq!(report.counts.verdicts["ack"], 1);
        assert_eq!(report.counts.verdicts["deferred"], 0);
        assert!(serde_json::to_value(&report).unwrap().get("lint").is_none());

        report.layers[2].verdict = Verdict::Pass;
        report.layers[3].verdict = Verdict::Pass;
        report.refresh_counts();
        assert!(report.passed);
        assert_eq!(report.counts.failed, 0);
    }

    #[test]
    fn lint_report_serializes_when_present() {
        let mut files = BTreeMap::new();
        files.insert("flake.nix".to_string(), 2);
        let mut p = parts(vec![], vec![]);
        p.lint = Some(LintReport {
            system: "aarch64-darwin".into(),
            tools: vec![LintToolReport {
                tool: "deadnix".into(),
                version: "9.9.9".into(),
                config_hash: "abc".into(),
                files,
            }],
        });
        let report = Report::finish(p);
        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["lint"]["system"], "aarch64-darwin");
        assert_eq!(json["lint"]["tools"][0]["files"]["flake.nix"], 2);
    }
}
