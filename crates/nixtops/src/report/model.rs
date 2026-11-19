//! The report as the schema (`schema/gate-report.schema.json`) wants it. Every layer is present;
//! a verdict that needs a reason carries one.

use serde::{Serialize, Serializer};
use serde_json::Value;

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
        }
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
        let names: Vec<&str> = ALL_LAYERS.iter().map(|l| l.as_str()).collect();
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
        assert_eq!(LayerId::parse("nope"), None);
    }

    #[test]
    fn counts_include_layers_and_warnings_each_on_their_own() {
        let layers = vec![
            Layer::new(LayerId::Source, Verdict::Pass, None),
            Layer::new(LayerId::Eval, Verdict::Refuse, Some("x".into())),
            Layer::new(LayerId::Lint, Verdict::NotRun, Some("y".into())),
            Layer::new(LayerId::Laws, Verdict::Skip, Some("z".into())),
        ];
        let warning = WarningEntry {
            source: WarningSource::BuiltinsWarn,
            message: "m".into(),
            verdict: Verdict::Ack,
            entry: None,
        };
        let report = Report::finish(parts(layers, vec![warning]));
        let v = &report.counts.verdicts;
        assert_eq!(
            (v["pass"].as_u64(), v["refuse"].as_u64(), v["ack"].as_u64()),
            (Some(1), Some(1), Some(1))
        );
        assert_eq!(
            (
                v["skip"].as_u64(),
                v["not_run"].as_u64(),
                v["vacuous"].as_u64()
            ),
            (Some(1), Some(1), Some(0))
        );
        assert_eq!(report.counts.failed, 2);
        assert!(!report.passed);
    }

    #[test]
    fn a_report_with_no_failing_layer_passed() {
        let layers = vec![
            Layer::new(LayerId::Source, Verdict::Pass, None),
            Layer::new(LayerId::Lint, Verdict::Skip, Some("r".into())),
        ];
        let report = Report::finish(parts(layers, vec![]));
        assert!(report.passed);
        assert_eq!(report.acks.count, 0);
        assert!(report.laws.is_empty());
    }

    #[test]
    fn serialization_uses_schema_names_and_skips_absent_reasons() {
        let layers = vec![
            Layer::new(LayerId::BuildTest, Verdict::NotRun, Some("why".into())),
            Layer::new(LayerId::Source, Verdict::Pass, None),
        ];
        let json = serde_json::to_value(Report::finish(parts(layers, vec![]))).unwrap();
        assert_eq!(
            json["layers"][0],
            serde_json::json!({"id": "build-test", "verdict": "not-run", "reason": "why"})
        );
        assert_eq!(
            json["layers"][1],
            serde_json::json!({"id": "source", "verdict": "pass"})
        );
        let w = WarningEntry {
            source: WarningSource::ConfigWarnings,
            message: "m".into(),
            verdict: Verdict::Refuse,
            entry: None,
        };
        assert_eq!(
            serde_json::to_value(w).unwrap(),
            serde_json::json!({"source": "config-warnings", "message": "m", "verdict": "Refuse"})
        );
        let d = WarningSource::Diagnostic;
        assert_eq!(serde_json::to_string(&d).unwrap(), "\"diagnostic\"");
    }

    #[test]
    fn acks_are_counted_and_keep_the_match_key() {
        let entry = AckEntry {
            kind: "ledger",
            matches: "m".into(),
            reason: "r".into(),
            until: "2099-01-01".into(),
            expired: false,
        };
        let mut p = parts(vec![], vec![]);
        p.acks = vec![entry.clone()];
        let report = Report::finish(p);
        assert_eq!(report.acks.count, 1);
        assert_eq!(serde_json::to_value(entry).unwrap()["match"], "m");
    }
}
