//! Which way a lint configuration moved. A hash says only that it changed, so the files are read:
//! each has lists that switch checks off (allowances) and everything else. The change is wider
//! only when everything else is equal and no allowance was added; anything this cannot show to be
//! wider or the same is narrower.

use super::inputs::Inputs;
use crate::config;
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;

#[derive(Debug, PartialEq, Eq)]
pub enum Change {
    /// Different text, the same meaning.
    Same,
    Wider,
    Narrower(String),
}

type Allowances = BTreeSet<String>;

/// statix always skips this one, so listing it is not an allowance.
const STATIX_DEFAULT_IGNORE: &str = "statix:ignore:.direnv";

/// Removes the string list at `key` and records each entry under `label`. A value that is not a
/// list of strings stays where it is, so it still counts as a difference.
fn take_list(table: &mut Map<String, Value>, key: &str, label: &str, out: &mut Allowances) {
    let strings = table
        .get(key)
        .and_then(Value::as_array)
        .is_some_and(|items| items.iter().all(Value::is_string));
    if !strings {
        return;
    }
    let items = table.remove(key).unwrap_or_default();
    let entries = items.as_array().into_iter().flatten();
    out.extend(
        entries
            .filter_map(Value::as_str)
            .map(|e| format!("{label}:{e}")),
    );
}

fn statix(table: &mut Map<String, Value>, out: &mut Allowances) {
    take_list(table, "disabled", "statix:disabled", out);
    take_list(table, "ignore", "statix:ignore", out);
    out.remove(STATIX_DEFAULT_IGNORE);
}

fn drop_if_empty(table: &mut Map<String, Value>, key: &str) {
    let empty = table
        .get(key)
        .and_then(Value::as_object)
        .is_some_and(Map::is_empty);
    if empty {
        table.remove(key);
    }
}

fn treefmt(table: &mut Map<String, Value>, out: &mut Allowances) {
    take_list(table, "excludes", "excludes", out);
    if let Some(global) = table.get_mut("global").and_then(Value::as_object_mut) {
        take_list(global, "excludes", "excludes", out);
    }
    drop_if_empty(table, "global");
    let formatters = table.get_mut("formatter").and_then(Value::as_object_mut);
    for (name, formatter) in formatters.into_iter().flatten() {
        if let Some(formatter) = formatter.as_object_mut() {
            take_list(formatter, "excludes", &format!("excludes:{name}"), out);
        }
    }
}

fn gitleaks(table: &mut Map<String, Value>, out: &mut Allowances) {
    if let Some(list) = table.get_mut("allowlist").and_then(Value::as_object_mut) {
        for key in ["paths", "regexes", "commits", "stopwords"] {
            take_list(list, key, &format!("allowlist:{key}"), out);
        }
        list.remove("description");
    }
    drop_if_empty(table, "allowlist");
    if let Some(Value::Array(lists)) = table.remove("allowlists") {
        out.extend(lists.iter().map(|entry| format!("allowlists:{entry}")));
    }
}

/// What an absent config file means.
fn default_of(tool: &str) -> Value {
    match tool {
        "nixfmt" => json!({"formatter": {"nix": {"command": "nixfmt", "includes": ["*.nix"]}}}),
        "gitleaks" => json!({"extend": {"useDefault": true}}),
        _ => json!({}),
    }
}

/// The config split into its allowances and the rest; `None` when it is not TOML.
fn shape(tool: &str, inputs: &Inputs) -> Option<(Allowances, Value)> {
    let doc = match inputs.get("config").and_then(Option::as_deref) {
        Some(text) => config::parse(text).ok()?,
        None => default_of(tool),
    };
    let mut table = doc.as_object().cloned()?;
    let mut out = Allowances::new();
    match tool {
        "statix" => statix(&mut table, &mut out),
        "nixfmt" => treefmt(&mut table, &mut out),
        "gitleaks" => gitleaks(&mut table, &mut out),
        _ => {}
    }
    let ignored = inputs.get("ignore").and_then(Option::as_deref);
    let lines = ignored.unwrap_or("").lines().map(str::trim);
    out.extend(
        lines
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(|l| format!("ignore:{l}")),
    );
    Some((out, Value::Object(table)))
}

fn deadnix(old: &Inputs, new: &Inputs) -> Change {
    let fails = |inputs: &Inputs| inputs.get("fail").and_then(Option::as_deref) != Some("false");
    match (fails(old), fails(new)) {
        (true, false) => Change::Narrower("lint.deadnix.fail was turned off".into()),
        (false, true) => Change::Wider,
        _ => Change::Same,
    }
}

fn by_allowances(old: (Allowances, Value), new: (Allowances, Value)) -> Change {
    if old.1 != new.1 {
        return Change::Narrower("a setting other than an allow list changed".into());
    }
    let added: Vec<&str> = new.0.difference(&old.0).map(String::as_str).collect();
    if !added.is_empty() {
        return Change::Narrower(format!("added {}", added.join(", ")));
    }
    if new.0 == old.0 {
        Change::Same
    } else {
        Change::Wider
    }
}

/// How `tool`'s configuration moved from `old` to `new`.
pub fn classify(tool: &str, old: &Inputs, new: &Inputs) -> Change {
    if tool == "deadnix" {
        return deadnix(old, new);
    }
    match (shape(tool, old), shape(tool, new)) {
        (Some(before), Some(after)) => by_allowances(before, after),
        _ => Change::Narrower("a config file is not valid TOML".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(text: Option<&str>) -> Inputs {
        Inputs::from([("config".to_string(), text.map(str::to_string))])
    }

    fn change(tool: &str, old: Option<&str>, new: Option<&str>) -> Change {
        classify(tool, &config(old), &config(new))
    }

    fn narrower(found: &Change) -> &str {
        match found {
            Change::Narrower(why) => why,
            other => panic!("expected narrower, got {other:?}"),
        }
    }

    #[test]
    fn statix_disabling_or_ignoring_more_is_narrower_and_less_is_wider() {
        let one = Some("disabled = [\"a\"]\n");
        let found = change("statix", None, one);
        assert_eq!(narrower(&found), "added statix:disabled:a");
        assert_eq!(change("statix", one, None), Change::Wider);
        assert_eq!(
            change("statix", one, Some("# c\ndisabled=['a']")),
            Change::Same
        );
        let found = change("statix", None, Some("ignore = ['x/*']"));
        assert_eq!(narrower(&found), "added statix:ignore:x/*");
        assert_eq!(
            change("statix", None, Some("ignore = ['.direnv']")),
            Change::Same
        );
        let swapped = change("statix", one, Some("disabled = ['b']"));
        assert_eq!(narrower(&swapped), "added statix:disabled:b");
    }

    #[test]
    fn any_other_setting_or_a_broken_file_is_narrower() {
        let found = change("statix", None, Some("nix_version = '2.4'"));
        assert!(narrower(&found).contains("other than an allow list"));
        let found = change("statix", None, Some("disabled = 'a'"));
        assert!(narrower(&found).contains("other than"));
        let found = change("statix", None, Some("disabled = ["));
        assert!(narrower(&found).contains("not valid TOML"));
        assert!(narrower(&change("statix", Some("="), None)).contains("not valid"));
    }

    #[test]
    fn treefmt_excludes_are_the_allowances_and_the_default_is_nixfmt_over_nix() {
        let same = "[formatter.nix]\ncommand = 'nixfmt'\nincludes = ['*.nix']\n";
        assert_eq!(change("nixfmt", None, Some(same)), Change::Same);
        for more in [
            format!("excludes = ['a.nix']\n{same}"),
            format!("{same}excludes = ['a.nix']\n"),
            format!("{same}[global]\nexcludes = ['a.nix']\n"),
        ] {
            assert!(
                narrower(&change("nixfmt", None, Some(&more))).contains("a.nix"),
                "{more}"
            );
            assert_eq!(
                change("nixfmt", Some(&more), Some(same)),
                Change::Wider,
                "{more}"
            );
        }
        let dropped = "[formatter.nix]\ncommand = 'true'\nincludes = ['*.nix']\n";
        assert!(narrower(&change("nixfmt", None, Some(dropped))).contains("other than"));
    }

    #[test]
    fn gitleaks_allowlists_and_ignore_lines_are_the_allowances() {
        let base = "[extend]\nuseDefault = true\n";
        assert_eq!(change("gitleaks", None, Some(base)), Change::Same);
        for more in [
            "[allowlist]\ndescription = 'd'\npaths = ['p']",
            "[allowlist]\nregexes = ['r']",
            "[[allowlists]]\npaths = ['p']",
        ] {
            let text = format!("{base}{more}\n");
            assert!(
                narrower(&change("gitleaks", None, Some(&text))).starts_with("added"),
                "{more}"
            );
            assert_eq!(
                change("gitleaks", Some(&text), None),
                Change::Wider,
                "{more}"
            );
        }
        let no_defaults = change("gitleaks", None, Some("title = 't'\n"));
        assert!(narrower(&no_defaults).contains("other than"));
        let ignore = |text: &str| {
            let mut inputs = config(None);
            inputs.insert("ignore".into(), Some(text.into()));
            inputs
        };
        let found = classify("gitleaks", &config(None), &ignore("# c\n\n abc:1 \n"));
        assert_eq!(narrower(&found), "added ignore:abc:1");
        assert_eq!(
            classify("gitleaks", &ignore("abc:1"), &config(None)),
            Change::Wider
        );
    }

    #[test]
    fn deadnix_failing_less_is_narrower() {
        let fail = |v: &str| Inputs::from([("fail".to_string(), Some(v.to_string()))]);
        let off = classify("deadnix", &fail("true"), &fail("false"));
        assert!(narrower(&off).contains("turned off"));
        assert_eq!(
            classify("deadnix", &fail("false"), &fail("true")),
            Change::Wider
        );
        assert_eq!(
            classify("deadnix", &fail("false"), &fail("false")),
            Change::Same
        );
        assert_eq!(
            classify("deadnix", &fail("true"), &fail("true")),
            Change::Same
        );
    }
}
