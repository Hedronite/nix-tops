//! The checks of section 12 that read the flake's outputs: each listed facet is one the outputs
//! show (section 5.4), and `[toolchain].systems` equals the systems the outputs export. The
//! checks are pure over a [`Structure`]; [`read`] is the one place that runs the pinned tools.

use super::{Outcome, Problem};
use crate::{
    build_info,
    inventory::{self, Session, Structure, Tools},
};
use serde_json::Value;
use std::{collections::BTreeSet, path::Path};

/// Facets of section 5.1 that M1 does not derive from outputs.
pub const NOT_DERIVED: [&str; 2] = ["fleet-root", "image"];

/// What reading the outputs gave.
#[derive(Debug, PartialEq)]
pub enum Read {
    Found(Structure),
    /// A build outside the kit's flake has no pinned judge to read with.
    NoJudge,
    Failed(String),
}

/// The structure of the flake in `dir`, read with the kit's pinned judge.
pub fn read(dir: &Path) -> Read {
    read_with(
        Tools::from_parts(
            build_info::JUDGE_PATH,
            build_info::EVAL_JOBS_PATH,
            build_info::NIXPKGS_REF,
        ),
        dir,
    )
}

/// As [`read`], with the tools chosen by the caller (`None` is a build with no pinned judge).
pub fn read_with(tools: Option<Tools>, dir: &Path) -> Read {
    let Some(tools) = tools else {
        return Read::NoJudge;
    };
    let session = match Session::new(tools) {
        Ok(session) => session,
        Err(err) => return Read::Failed(format!("cannot create a scratch directory: {err}")),
    };
    let flake = dir.to_str().unwrap_or(".");
    match inventory::structure_of(&session, flake) {
        Ok(structure) => Read::Found(structure),
        Err(error) => Read::Failed(first_line(&error).to_string()),
    }
}

/// A read as the structure or the sentence that says why there is none.
pub fn found(read: Read) -> Result<Structure, String> {
    match read {
        Read::Found(structure) => Ok(structure),
        Read::NoJudge => Err("this build has no pinned judge (built outside the kit's flake), so the outputs cannot be read".into()),
        Read::Failed(message) => Err(format!("cannot read the flake's outputs: {message}")),
    }
}

/// The line of a Nix error that says what is wrong: the last `error:` line with text after it,
/// else the first line with any text.
fn first_line(text: &str) -> &str {
    let lines = || text.lines().map(str::trim).filter(|l| !l.is_empty());
    let said = |l: &&str| {
        l.strip_prefix("error:")
            .is_some_and(|rest| !rest.trim().is_empty())
    };
    lines()
        .filter(said)
        .next_back()
        .or_else(|| lines().next())
        .unwrap_or("")
}

fn strings(doc: &Value, pointer: &str) -> Vec<String> {
    let items = doc.pointer(pointer).and_then(Value::as_array);
    items
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(String::from))
        .collect()
}

/// An output that could not be listed makes the rest unknowable: name it and stop.
fn unlisted(structure: &Structure) -> Vec<Problem> {
    structure
        .errors
        .iter()
        .map(|(name, error)| {
            Problem::new(
                format!("outputs.{name}"),
                format!("cannot list its members: {}", first_line(error)),
            )
        })
        .collect()
}

fn facet_problem(i: usize, facet: &str, shown: &BTreeSet<&str>) -> Option<Problem> {
    let key = format!("facets[{i}]");
    if NOT_DERIVED.contains(&facet) {
        let message = format!("{facet}: M1 does not derive this facet from the outputs");
        return Some(Problem::new(key, message));
    }
    (!shown.contains(facet)).then(|| {
        let seen: Vec<&str> = shown.iter().copied().collect();
        let message = format!(
            "{facet}: the outputs do not show it (they show: {})",
            seen.join(", ")
        );
        Problem::new(key, message)
    })
}

fn system_problem(listed: &[String], exported: &BTreeSet<String>) -> Option<Problem> {
    let listed: BTreeSet<String> = listed.iter().cloned().collect();
    if &listed == exported {
        return None;
    }
    let join = |a: &BTreeSet<String>, b: &BTreeSet<String>| {
        a.difference(b).cloned().collect::<Vec<_>>().join(", ")
    };
    Some(Problem::new(
        "toolchain.systems".into(),
        format!(
            "must equal the systems the outputs export: missing [{}], not exported [{}]",
            join(exported, &listed),
            join(&listed, exported)
        ),
    ))
}

pub fn check(doc: &Value, structure: &Structure) -> Outcome {
    let mut out = Outcome {
        environment: unlisted(structure),
        ..Outcome::default()
    };
    if !out.environment.is_empty() {
        return out;
    }
    let facets = strings(doc, "/facets");
    let derived = inventory::derived_facets(structure);
    let shown: BTreeSet<&str> = derived.iter().map(|f| f.as_str()).collect();
    out.problems.extend(
        facets
            .iter()
            .enumerate()
            .filter_map(|(i, f)| facet_problem(i, f, &shown)),
    );
    let exported = inventory::exported_systems(structure);
    out.problems.extend(system_problem(
        &strings(doc, "/toolchain/systems"),
        &exported,
    ));
    out.notes.push(format!(
        "outputs show facets: {}; export systems: {}",
        shown.iter().copied().collect::<Vec<_>>().join(", "),
        exported.iter().cloned().collect::<Vec<_>>().join(", ")
    ));
    if facets.iter().any(|f| f == "home-config") {
        out.notes.push(
            "home-config is derived from homeConfigurations only in M1 (not from Home Manager as a module)"
                .into(),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn structure(entries: &[(&str, &[&str])]) -> Structure {
        let mut s = Structure::default();
        for (name, members) in entries {
            s.insert(name, members.iter().map(|m| m.to_string()).collect());
        }
        s
    }
    fn doc(facets: &[&str], systems: &[&str]) -> Value {
        json!({"facets": facets, "toolchain": {"systems": systems}})
    }
    fn keys(o: &Outcome) -> Vec<&str> {
        o.problems.iter().map(|p| p.key.as_str()).collect()
    }
    fn flake() -> Structure {
        structure(&[
            ("packages", &["x86_64-linux", "aarch64-linux"]),
            ("devShells", &["x86_64-linux", "aarch64-linux"]),
        ])
    }

    #[test]
    fn facets_and_systems_that_match_pass() {
        let o = check(
            &doc(
                &["package-set", "dev-flake"],
                &["aarch64-linux", "x86_64-linux"],
            ),
            &flake(),
        );
        assert_eq!(o.problems, vec![]);
        assert!(
            o.notes[0].contains("dev-flake, package-set"),
            "{:?}",
            o.notes
        );
    }

    #[test]
    fn a_facet_the_outputs_do_not_show_fails_naming_its_index() {
        let o = check(
            &doc(
                &["dev-flake", "nixos-module"],
                &["x86_64-linux", "aarch64-linux"],
            ),
            &flake(),
        );
        assert_eq!(keys(&o), ["facets[1]"]);
        assert!(
            o.problems[0]
                .message
                .contains("do not show it (they show: dev-flake, package-set)")
        );
    }

    #[test]
    fn fleet_root_and_image_are_refused_as_not_derived_in_m1() {
        for facet in NOT_DERIVED {
            let o = check(&doc(&[facet], &["x86_64-linux", "aarch64-linux"]), &flake());
            assert_eq!(keys(&o), ["facets[0]"], "{facet}");
            assert!(
                o.problems[0].message.contains("M1 does not derive"),
                "{facet}"
            );
        }
    }

    #[test]
    fn systems_that_differ_from_the_exports_fail_and_say_how() {
        let o = check(
            &doc(&["dev-flake"], &["x86_64-linux", "aarch64-darwin"]),
            &flake(),
        );
        assert_eq!(keys(&o), ["toolchain.systems"]);
        let m = &o.problems[0].message;
        assert!(
            m.contains("missing [aarch64-linux]") && m.contains("not exported [aarch64-darwin]"),
            "{m}"
        );
    }

    #[test]
    fn an_unlisted_output_is_an_environment_failure_that_names_it_and_stops() {
        let mut s = flake();
        s.insert_error("checks", "boom\nmore".into());
        let o = check(&doc(&["nixos-module"], &["x86_64-linux"]), &s);
        assert_eq!(o.problems, vec![]);
        assert_eq!(o.environment.len(), 1);
        assert_eq!(o.environment[0].key, "outputs.checks");
        assert!(o.environment[0].message.ends_with("boom"));
        assert!(o.notes.is_empty());
    }

    #[test]
    fn home_config_says_what_m1_derives_it_from() {
        let s = structure(&[("homeConfigurations", &["me"])]);
        let o = check(&doc(&["home-config"], &[]), &s);
        assert!(
            o.notes
                .iter()
                .any(|n| n.contains("homeConfigurations only"))
        );
        assert!(!o.problems.iter().any(|p| p.key.starts_with("facets")));
    }

    #[test]
    fn first_line_skips_blank_lines() {
        assert_eq!(first_line("\n  \n x \ny"), "x");
        assert_eq!(first_line(""), "");
    }

    #[test]
    fn first_line_prefers_the_last_error_line_that_has_text() {
        let nix =
            "error:\n       … while updating\n\n       error: lock file changes are not allowed\n";
        assert_eq!(first_line(nix), "error: lock file changes are not allowed");
        assert_eq!(first_line("error:\n  trace\n"), "error:");
        assert_eq!(first_line("error: first\nerror: second"), "error: second");
    }
}
