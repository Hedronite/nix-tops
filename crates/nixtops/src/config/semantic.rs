//! The checks of section 12 that need no outputs. Time and the filesystem come in as arguments:
//! there is no override of the date, so an expired entry cannot be skipped.

use super::Problem;
use crate::profiles::{self, Status};
use serde_json::Value;
use time::{Date, format_description::well_known::Iso8601};

/// What the checks read besides the config itself.
pub struct Context<'a> {
    /// The UTC date, `YYYY-MM-DD`, taken once at the edge.
    pub today: &'a str,
    /// The parsed `flake.lock`, if there is one.
    pub lock: Option<&'a Value>,
    /// The baseline file named by `baseline` exists.
    pub baseline_exists: bool,
}

#[derive(Debug, Default, PartialEq)]
pub struct Findings {
    pub problems: Vec<Problem>,
    pub notes: Vec<String>,
}

pub const DEFAULT_BASELINE: &str = ".nix-tops/baseline.json";

pub fn baseline_path(doc: &Value) -> &str {
    doc.get("baseline")
        .and_then(Value::as_str)
        .unwrap_or(DEFAULT_BASELINE)
}

/// The UTC date today as `YYYY-MM-DD`.
pub fn today_utc() -> String {
    time::OffsetDateTime::now_utc()
        .date()
        .format(&Iso8601::DATE)
        .expect("a date formats")
}

fn valid_date(text: &str) -> bool {
    Date::parse(text, &Iso8601::DATE).is_ok()
}

/// The `nix-tops` input of the root: `Some(true)` when it follows nixpkgs, `None` when absent.
pub fn kit_follows_nixpkgs(lock: &Value) -> Option<bool> {
    let nodes = lock.get("nodes")?;
    let root = lock.get("root").and_then(Value::as_str).unwrap_or("root");
    let input = nodes.get(root)?.get("inputs")?.get("nix-tops")?;
    // A root input that is itself a follows path is not the kit's own node.
    let Some(node) = input.as_str() else {
        return Some(true);
    };
    let inner = nodes.get(node)?.get("inputs")?.get("nixpkgs");
    Some(inner.is_some_and(Value::is_array))
}

fn facets(doc: &Value) -> Vec<String> {
    doc.get("facets")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// The enforcement mode; `ratchet` when the key is absent (section 6.8).
pub fn enforce_of(doc: &Value) -> &str {
    doc.get("enforce")
        .and_then(Value::as_str)
        .unwrap_or("ratchet")
}

fn enforce_problems(doc: &Value, ctx: &Context<'_>) -> Vec<Problem> {
    let enforce = enforce_of(doc);
    let mut out = Vec::new();
    if enforce == "advisory" {
        out.push(Problem::new(
            "enforce".into(),
            format!(
                "advisory needs a dated exemption, and protocol {} has no exemption key yet; use hard or ratchet",
                crate::build_info::PROTOCOL
            ),
        ));
    }
    if enforce == "ratchet" && !ctx.baseline_exists {
        out.push(Problem::new(
            "baseline".into(),
            format!(
                "{} does not exist, and enforce = ratchet needs it (run nixtops baseline after a passing gate)",
                baseline_path(doc)
            ),
        ));
    }
    out
}

fn facet_notes(doc: &Value) -> Vec<String> {
    facets(doc)
        .into_iter()
        .map(|facet| match profiles::status(&facet) {
            Status::Launch => format!("facet {facet}: profile found"),
            Status::Unproven => {
                format!("facet {facet}: specified, unproven (profile found; no adopter has run it)")
            }
            Status::NoProfile => format!("facet {facet}: no profile"),
        })
        .collect()
}

fn ledger_problem(i: usize, entry: &Value, today: &str) -> Option<Problem> {
    let until = entry.get("until").and_then(Value::as_str).unwrap_or("");
    let key = format!("warnings.allow[{i}].until");
    if !valid_date(until) {
        return Some(Problem::new(key, format!("{until} is not a calendar date")));
    }
    (until < today).then(|| {
        Problem::new(
            key,
            format!("expired on {until} (today is {today}); an expired entry fails the gate"),
        )
    })
}

fn ledger_problems(doc: &Value, today: &str) -> Vec<Problem> {
    let entries = doc.pointer("/warnings/allow").and_then(Value::as_array);
    entries
        .into_iter()
        .flatten()
        .enumerate()
        .filter_map(|(i, entry)| ledger_problem(i, entry, today))
        .collect()
}

/// A switched-off layer needs a reason that says something: whitespace alone is none.
fn layer_problems(doc: &Value) -> Vec<Problem> {
    let layers = doc.get("layers").and_then(Value::as_object);
    layers
        .into_iter()
        .flatten()
        .filter(|(_, layer)| {
            layer
                .get("reason")
                .and_then(Value::as_str)
                .is_some_and(|reason| reason.trim().is_empty())
        })
        .map(|(id, _)| {
            Problem::new(
                format!("layers.{id}.reason"),
                "a blank reason is no reason; say why the layer is off".into(),
            )
        })
        .collect()
}

/// A problem when the kit input follows nixpkgs, otherwise a note.
fn lock_finding(lock: Option<&Value>) -> Result<String, Problem> {
    match lock.map(kit_follows_nixpkgs) {
        Some(Some(true)) => Err(Problem::new(
            "inputs.nix-tops.inputs.nixpkgs".into(),
            "the nix-tops input follows nixpkgs; its tools stay on the kit's pin (N-43)".into(),
        )),
        Some(Some(false)) => Ok("nix-tops input: does not follow nixpkgs".into()),
        Some(None) => {
            Ok("no nix-tops input in flake.lock (the kit itself, or not yet added)".into())
        }
        None => Ok("no flake.lock found; the nix-tops input was not checked".into()),
    }
}

pub fn check(doc: &Value, ctx: &Context<'_>) -> Findings {
    let mut out = Findings {
        problems: enforce_problems(doc, ctx),
        notes: facet_notes(doc),
    };
    out.problems.extend(ledger_problems(doc, ctx.today));
    out.problems.extend(layer_problems(doc));
    match lock_finding(ctx.lock) {
        Ok(note) => out.notes.push(note),
        Err(problem) => out.problems.push(problem),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ctx<'a>(today: &'a str, lock: Option<&'a Value>, baseline: bool) -> Context<'a> {
        Context {
            today,
            lock,
            baseline_exists: baseline,
        }
    }
    fn doc() -> Value {
        json!({"protocol":crate::build_info::PROTOCOL,"facets":["dev-flake"],"toolchain":{"systems":["x86_64-linux"]},"enforce":"hard"})
    }
    fn keys(f: &Findings) -> Vec<&str> {
        f.problems.iter().map(|p| p.key.as_str()).collect()
    }

    #[test]
    fn a_clean_hard_config_passes() {
        assert_eq!(
            keys(&check(&doc(), &ctx("2026-11-18", None, false))),
            Vec::<&str>::new()
        );
    }

    #[test]
    fn advisory_is_refused_naming_enforce() {
        let mut d = doc();
        d["enforce"] = json!("advisory");
        let f = check(&d, &ctx("2026-11-18", None, true));
        assert_eq!(keys(&f), ["enforce"]);
        assert!(f.problems[0].message.contains("no exemption key"));
    }

    #[test]
    fn ratchet_needs_the_baseline_and_the_default_is_ratchet() {
        let mut d = doc();
        d.as_object_mut().unwrap().remove("enforce");
        assert_eq!(
            keys(&check(&d, &ctx("2026-11-18", None, false))),
            ["baseline"]
        );
        assert!(
            check(&d, &ctx("2026-11-18", None, true))
                .problems
                .is_empty()
        );
        d["baseline"] = json!("b.json");
        assert!(
            check(&d, &ctx("2026-11-18", None, false)).problems[0]
                .message
                .contains("b.json")
        );
    }

    #[test]
    fn an_expired_entry_fails_on_the_day_after_and_not_on_the_day() {
        let mut d = doc();
        d["warnings"] = json!({"allow":[{"match":"x","reason":"r","until":"2026-11-18"}]});
        assert!(
            check(&d, &ctx("2026-11-18", None, false))
                .problems
                .is_empty()
        );
        let f = check(&d, &ctx("2026-11-19", None, false));
        assert_eq!(keys(&f), ["warnings.allow[0].until"]);
        assert!(f.problems[0].message.contains("expired"));
    }

    #[test]
    fn an_impossible_date_fails() {
        let mut d = doc();
        d["warnings"] = json!({"allow":[{"match":"x","reason":"r","until":"2026-02-31"}]});
        let f = check(&d, &ctx("2026-11-18", None, false));
        assert_eq!(keys(&f), ["warnings.allow[0].until"]);
        assert!(f.problems[0].message.contains("not a calendar date"));
    }

    #[test]
    fn a_blank_layer_reason_fails_naming_the_layer_and_a_real_one_passes() {
        let mut d = doc();
        d["layers"] = json!({"lint":{"enabled":false,"reason":" \t "},
            "coverage":{"enabled":false,"reason":"not built in M1"}});
        let f = check(&d, &ctx("2026-11-18", None, false));
        assert_eq!(keys(&f), ["layers.lint.reason"]);
        d["layers"]["lint"]["reason"] = json!("");
        assert_eq!(
            keys(&check(&d, &ctx("2026-11-18", None, false))),
            ["layers.lint.reason"]
        );
        d["layers"]["lint"]["reason"] = json!("not built in M1");
        assert!(
            check(&d, &ctx("2026-11-18", None, false))
                .problems
                .is_empty()
        );
    }

    fn lock(inner: Value) -> Value {
        json!({"root":"root","nodes":{"root":{"inputs":{"nix-tops":"nix-tops","nixpkgs":"nixpkgs"}},
          "nix-tops":{"inputs":{"nixpkgs":inner}},"nixpkgs":{}}})
    }

    #[test]
    fn a_kit_input_that_follows_nixpkgs_fails() {
        let l = lock(json!(["nixpkgs"]));
        let f = check(&doc(), &ctx("2026-11-18", Some(&l), false));
        assert_eq!(keys(&f), ["inputs.nix-tops.inputs.nixpkgs"]);
        let own = lock(json!("nixpkgs_2"));
        assert!(
            check(&doc(), &ctx("2026-11-18", Some(&own), false))
                .problems
                .is_empty()
        );
    }

    #[test]
    fn no_kit_input_is_reported_not_failed() {
        let l =
            json!({"root":"root","nodes":{"root":{"inputs":{"nixpkgs":"nixpkgs"}},"nixpkgs":{}}});
        let f = check(&doc(), &ctx("2026-11-18", Some(&l), false));
        assert!(f.problems.is_empty());
        assert!(f.notes.iter().any(|n| n.contains("no nix-tops input")));
    }

    #[test]
    fn facets_are_reported_by_status() {
        let mut d = doc();
        d["facets"] = json!(["dev-flake", "nixos-host", "image"]);
        let n = check(&d, &ctx("2026-11-18", None, false)).notes.join("\n");
        assert!(n.contains("dev-flake: profile found"));
        assert!(n.contains("nixos-host: specified, unproven"));
        assert!(n.contains("image: no profile"));
    }

    #[test]
    fn today_is_an_iso_date() {
        assert!(valid_date(&today_utc()));
    }
}
