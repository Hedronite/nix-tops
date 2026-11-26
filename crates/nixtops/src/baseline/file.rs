//! `.nix-tops/baseline.json`: its shape (`schema/baseline.schema.json`), reading and writing.
//! Sorted maps and no timestamps, so the same findings give the same bytes.

use super::inputs::{self, Inputs};
use crate::{
    build_info,
    config::schema,
    report::{LintReport, Report, Verdict},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    io::ErrorKind,
    path::Path,
};

pub const BASELINE_SCHEMA: &str = include_str!("../../../../schema/baseline.schema.json");

/// One lint tool: how it was configured and what it found, file by file.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tool {
    pub version: String,
    pub config_hash: String,
    /// The config files, kept only when they reproduce `config_hash`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub inputs: Option<Inputs>,
    pub files: BTreeMap<String, usize>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Baseline {
    pub protocol: String,
    pub lint: BTreeMap<String, Tool>,
    pub acks: usize,
    /// The IDs of the laws a run judged.
    pub laws: BTreeSet<String>,
}

fn tools_of(root: &Path, doc: &Value, lint: &LintReport) -> BTreeMap<String, Tool> {
    let tools = lint.tools.iter().map(|found| {
        let counted = found.files.iter().filter(|(_, n)| **n > 0);
        let tool = Tool {
            version: found.version.clone(),
            config_hash: found.config_hash.clone(),
            inputs: inputs::verified(root, doc, &found.tool, &found.config_hash),
            files: counted.map(|(file, n)| (file.clone(), *n)).collect(),
        };
        (found.tool.clone(), tool)
    });
    tools.collect()
}

fn was_judged(law: &Value) -> bool {
    let verdict = law.get("verdict").and_then(Value::as_str).unwrap_or("");
    let unjudged = [Verdict::Skip, Verdict::NotRun, Verdict::NotEvaluated];
    !unjudged.iter().any(|v| v.name() == verdict)
}

fn judged_laws(laws: &[Value]) -> BTreeSet<String> {
    let judged = laws.iter().filter(|law| was_judged(law));
    judged
        .filter_map(|law| law.get("id").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}

impl Baseline {
    /// What this run would record.
    pub fn of(root: &Path, doc: &Value, report: &Report) -> Self {
        Self {
            protocol: build_info::PROTOCOL.to_string(),
            lint: report
                .lint
                .as_ref()
                .map(|lint| tools_of(root, doc, lint))
                .unwrap_or_default(),
            acks: report.acks.count,
            laws: judged_laws(&report.laws),
        }
    }

    /// The findings it records, over all tools and files.
    pub fn total(&self) -> usize {
        self.lint
            .values()
            .flat_map(|tool| tool.files.values())
            .sum()
    }
}

/// The file's text as a baseline, or why it is not one.
pub fn parse(text: &str) -> Result<Baseline, String> {
    let value: Value = serde_json::from_str(text).map_err(|e| format!("not JSON: {e}"))?;
    let problems = schema::problems(&schema::validator(BASELINE_SCHEMA), &value);
    if let Some(first) = problems.first() {
        return Err(format!("{}: {}", first.key, first.message));
    }
    serde_json::from_value(value).map_err(|e| format!("not a baseline: {e}"))
}

/// The baseline at `path`, `None` when there is no file.
pub fn load(path: &Path) -> Result<Option<Baseline>, String> {
    let shown = path.display();
    match fs::read_to_string(path) {
        Ok(text) => parse(&text)
            .map(Some)
            .map_err(|e| format!("{shown} is not a valid baseline: {e}")),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("cannot read {shown}: {error}")),
    }
}

/// Writes through a temporary file, so a stopped run leaves the old baseline or the new one.
pub fn write(path: &Path, baseline: &Baseline) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut text = serde_json::to_string_pretty(baseline).map_err(io::Error::other)?;
    text.push('\n');
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, text)?;
    fs::rename(&temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample() -> Baseline {
        let tool = Tool {
            version: "1".into(),
            config_hash: "h".into(),
            inputs: Some(Inputs::from([("config".to_string(), None)])),
            files: BTreeMap::from([("a.nix".to_string(), 2), ("b.nix".to_string(), 3)]),
        };
        Baseline {
            protocol: build_info::PROTOCOL.into(),
            lint: BTreeMap::from([("statix".to_string(), tool)]),
            acks: 1,
            laws: BTreeSet::from(["N-04".to_string()]),
        }
    }

    #[test]
    fn what_is_written_reads_back_and_validates() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("sub/baseline.json");
        assert_eq!(load(&path), Ok(None));
        write(&path, &sample()).expect("writes");
        assert_eq!(load(&path), Ok(Some(sample())));
        assert_eq!(sample().total(), 5);
        assert!(!path.with_extension("json.tmp").exists());
        let text = fs::read_to_string(&path).expect("read");
        write(&path, &sample()).expect("writes again");
        assert_eq!(fs::read_to_string(&path).expect("read"), text);
    }

    #[test]
    fn a_file_that_is_not_a_baseline_is_refused_by_name() {
        let good = serde_json::to_value(sample()).expect("json");
        let broken = |edit: fn(&mut Value)| {
            let mut value = good.clone();
            edit(&mut value);
            parse(&value.to_string()).expect_err("refused")
        };
        assert!(broken(|v| v["extra"] = json!(1)).contains("extra"));
        assert!(broken(|v| v["protocol"] = json!("9")).contains("protocol"));
        assert!(broken(|v| v["acks"] = json!(-1)).contains("acks"));
        let zero = broken(|v| v["lint"]["statix"]["files"]["a.nix"] = json!(0));
        assert!(zero.contains("a.nix"), "{zero}");
        assert!(parse("{").expect_err("refused").starts_with("not JSON"));
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("b.json");
        fs::write(&path, "{}").expect("write");
        assert!(
            load(&path)
                .expect_err("refused")
                .contains("not a valid baseline")
        );
        assert!(
            load(dir.path())
                .expect_err("a directory")
                .starts_with("cannot read")
        );
    }

    #[test]
    fn a_file_with_no_finding_is_left_out() {
        let found = crate::report::LintToolReport {
            tool: "deadnix".into(),
            version: "1".into(),
            config_hash: "h".into(),
            files: BTreeMap::from([("a.nix".to_string(), 0), ("b.nix".to_string(), 1)]),
        };
        let lint = LintReport {
            system: "s".into(),
            tools: vec![found],
        };
        let tools = tools_of(Path::new("."), &json!({}), &lint);
        let files: Vec<&String> = tools["deadnix"].files.keys().collect();
        assert_eq!(files, ["b.nix"]);
        assert_eq!(tools["deadnix"].inputs, None);
    }

    #[test]
    fn only_a_judged_law_enters_the_set() {
        let laws = [
            json!({"id": "N-01", "verdict": "pass"}),
            json!({"id": "N-02", "verdict": "Refuse"}),
            json!({"id": "N-03", "verdict": "skip"}),
            json!({"id": "N-04", "verdict": "not-run"}),
            json!({"id": "N-05", "verdict": "not-evaluated"}),
        ];
        let ids: Vec<String> = judged_laws(&laws).into_iter().collect();
        assert_eq!(ids, ["N-01", "N-02"]);
    }
}
