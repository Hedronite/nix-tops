//! Reading `nix-eval-jobs` output: one JSON line per attribute, an `error` key on the ones that
//! failed. A failure of the whole attribute set (the container) has no line; the process just
//! exits non-zero and says why on stderr.

use super::record::{Kind, Level, Record, prune_meta};
use serde_json::Value;

/// How many characters of an error message the inventory keeps.
const ERROR_LIMIT: usize = 4000;

/// Lines on stderr that say nothing about the evaluation.
const NOISE: [&str; 3] = ["unknown setting", "unpacking '", "copying path"];

/// An error message without the host's configuration noise, cut to a fixed length.
pub fn clean(stderr: &str) -> String {
    let kept: Vec<&str> = stderr
        .lines()
        .filter(|line| !NOISE.iter().any(|noise| line.contains(noise)))
        .collect();
    let text = kept.join("\n");
    text.trim().chars().take(ERROR_LIMIT).collect()
}

/// One parsed line.
#[derive(Debug, PartialEq)]
pub struct Job {
    pub attr_path: Vec<String>,
    pub drv_path: Option<String>,
    pub name: Option<String>,
    pub error: Option<String>,
    pub warnings: Vec<String>,
    pub meta: Option<Value>,
}

fn text(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

fn string_array(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// The lines of `stdout` that are JSON objects with an `attrPath`; anything else is skipped.
pub fn parse(stdout: &str) -> Vec<Job> {
    let objects = stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok());
    let jobs = objects.filter(|value| value.get("attrPath").is_some());
    jobs.map(|value| Job {
        attr_path: string_array(&value, "attrPath"),
        drv_path: text(&value, "drvPath"),
        name: text(&value, "name"),
        error: text(&value, "error").map(|e| clean(&e)),
        warnings: string_array(&value, "warnings"),
        meta: value.get("meta").and_then(prune_meta),
    })
    .collect()
}

/// Where the records of one attribute set belong.
pub struct Target<'a> {
    pub base: &'a str,
    pub kind: Kind,
    pub system: &'a str,
}

fn with_warnings(warnings: Vec<String>, error: String) -> String {
    if warnings.is_empty() {
        return error;
    }
    let mut text: String = warnings
        .into_iter()
        .map(|w| format!("evaluation warning: {w}\n"))
        .collect();
    text.push_str(&error);
    text
}

fn record(target: &Target, job: Job) -> Record {
    let attr = std::iter::once(target.base.to_string())
        .chain(job.attr_path)
        .collect::<Vec<_>>()
        .join(".");
    match job.error {
        Some(error) => {
            let error = with_warnings(job.warnings, error);
            Record::failed(&attr, target.kind, Some(target.system), error)
        }
        None => Record::new(&attr, target.kind, Some(target.system), Level::Evaluated)
            .with_drv(job.drv_path, job.name)
            .with_meta(job.meta),
    }
}

/// The records for one attribute set. A non-zero exit adds a record for the set itself, so an
/// error that stopped the evaluation before any line is never lost.
pub fn records(target: &Target, ok: bool, stdout: &str, stderr: &str) -> Vec<Record> {
    let mut found: Vec<Record> = parse(stdout)
        .into_iter()
        .map(|job| record(target, job))
        .collect();
    if !ok {
        found.push(Record::failed(
            target.base,
            target.kind,
            Some(target.system),
            clean(stderr),
        ));
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target() -> Target<'static> {
        Target {
            base: "packages.x86_64-linux",
            kind: Kind::Package,
            system: "x86_64-linux",
        }
    }

    const GOOD: &str = r#"{"attr":"a","attrPath":["a"],"drvPath":"/nix/store/x-a.drv","name":"a-1","meta":{"description":"d","platforms":[1]}}"#;
    const BAD: &str = r#"{"attr":"b","attrPath":["b"],"error":"error: boom\nunknown setting 'x'"}"#;

    #[test]
    fn clean_drops_configuration_noise_and_cuts_to_the_limit() {
        assert_eq!(
            clean("warning: unknown setting 'a'\nreal\nunpacking 'x'\ncopying path y\n"),
            "real"
        );
        assert_eq!(clean("  \n"), "");
        let long = "x".repeat(ERROR_LIMIT + 50);
        assert_eq!(clean(&long).chars().count(), ERROR_LIMIT);
    }

    #[test]
    fn parse_keeps_job_lines_and_skips_the_rest() {
        let jobs = parse(&format!("{GOOD}\nnot json\n{{\"other\":1}}\n{BAD}\n"));
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].attr_path, ["a"]);
        assert_eq!(jobs[0].drv_path.as_deref(), Some("/nix/store/x-a.drv"));
        assert_eq!(jobs[0].name.as_deref(), Some("a-1"));
        assert_eq!(jobs[0].meta, Some(serde_json::json!({"description": "d"})));
        assert_eq!(jobs[1].error.as_deref(), Some("error: boom"));
        assert!(jobs[1].drv_path.is_none());
    }

    #[test]
    fn a_job_without_an_attr_path_is_still_a_job_with_an_empty_path_when_the_key_is_not_an_array() {
        let jobs = parse(r#"{"attrPath":"oops","drvPath":"/d"}"#);
        assert_eq!(jobs.len(), 1);
        assert!(jobs[0].attr_path.is_empty());
    }

    #[test]
    fn records_join_the_base_and_the_attribute_path() {
        let found = records(&target(), true, &format!("{GOOD}\n{BAD}"), "");
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].attr, "packages.x86_64-linux.a");
        assert_eq!(found[0].level, Level::Evaluated);
        assert_eq!(found[0].system.as_deref(), Some("x86_64-linux"));
        assert_eq!(found[1].attr, "packages.x86_64-linux.b");
        assert!(found[1].is_error());
    }

    #[test]
    fn a_non_zero_exit_adds_a_record_for_the_set() {
        let found = records(
            &target(),
            false,
            "",
            "warning: unknown setting 'z'\nerror: whole set failed",
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].attr, "packages.x86_64-linux");
        assert_eq!(found[0].error.as_deref(), Some("error: whole set failed"));
    }

    #[test]
    fn a_failed_exit_keeps_the_lines_that_came_first() {
        let found = records(&target(), false, GOOD, "error: later");
        assert_eq!(found.len(), 2);
        assert!(!found[0].is_error() && found[1].is_error());
    }

    #[test]
    fn failed_job_with_warnings_array_prepends_evaluation_warnings() {
        let line =
            r#"{"attr":"w","attrPath":["w"],"error":"error: aborting","warnings":["use other"]}"#;
        let found = records(&target(), true, line, "");
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].error.as_deref(),
            Some("evaluation warning: use other\nerror: aborting")
        );
    }
}
