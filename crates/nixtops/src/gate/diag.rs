//! Source 3 of gate 3: the diagnostics the `nix` command prints itself, classified by
//! `laws/diagnostics.toml` (embedded at build time: one source). A `warning:` line that no entry
//! matches is unclassified and fails the gate.

use regex::Regex;

const TABLE: &str = include_str!("../../../../laws/diagnostics.toml");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    Report,
    Fail,
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub id: String,
    pub implementation: String,
    pub effect: Effect,
    pub pattern: Regex,
    pub continuation: Option<String>,
}

fn entry_of(item: &toml::Value) -> Result<Entry, String> {
    let text = |key: &str| item.get(key).and_then(toml::Value::as_str);
    let need = |key: &str| {
        text(key)
            .map(str::to_string)
            .ok_or_else(|| format!("diagnostics.toml: an entry lacks `{key}`"))
    };
    let effect = match text("effect") {
        Some("report") | None => Effect::Report,
        Some("fail") => Effect::Fail,
        Some(other) => {
            return Err(format!(
                "diagnostics.toml: effect `{other}` is not report or fail"
            ));
        }
    };
    let pattern =
        Regex::new(&need("match")?).map_err(|e| format!("diagnostics.toml: bad match: {e}"))?;
    Ok(Entry {
        id: need("id")?,
        implementation: need("implementation")?,
        effect,
        pattern,
        continuation: text("continuation").map(str::to_string),
    })
}

/// The entries of a table's text.
pub fn parse(text: &str) -> Result<Vec<Entry>, String> {
    let table: toml::Table = text
        .parse()
        .map_err(|e: toml::de::Error| format!("diagnostics.toml: {}", e.message()))?;
    let list = table.get("diagnostic").and_then(toml::Value::as_array);
    list.into_iter().flatten().map(entry_of).collect()
}

/// The table embedded in this build.
pub fn embedded() -> Result<Vec<Entry>, String> {
    parse(TABLE)
}

/// A classified line: the entry it matched, or `None` for an unclassified one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Classified {
    pub line: String,
    pub id: Option<String>,
    pub effect: Option<Effect>,
}

fn is_diagnostic(line: &str) -> bool {
    line.trim_start().starts_with("warning:")
}

fn classify_line(entries: &[Entry], implementation: &str, line: &str) -> Option<usize> {
    let mut candidates = entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.implementation == implementation);
    candidates
        .find(|(_, e)| e.pattern.is_match(line.trim()))
        .map(|(i, _)| i)
}

/// Classifies every `warning:` line of `text` against the entries of `implementation`. The lines
/// that follow a diagnostic (an entry's `continuation`) do not start with `warning:`, so they are
/// not diagnostics of their own and nothing has to consume them.
pub fn classify(entries: &[Entry], implementation: &str, text: &str) -> Vec<Classified> {
    let lines = text.lines().filter(|line| is_diagnostic(line));
    lines
        .map(|line| {
            let entry = classify_line(entries, implementation, line).map(|i| &entries[i]);
            Classified {
                line: line.trim().to_string(),
                id: entry.map(|e| e.id.clone()),
                effect: entry.map(|e| e.effect),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SMALL: &str = r#"
table_version = 1
[[diagnostic]]
id = "D-01"
implementation = "cppnix"
effect = "report"
match = "^warning: unknown flake output '[^']+'$"
[[diagnostic]]
id = "D-04"
implementation = "cppnix"
effect = "fail"
match = "^warning: omitted: .+$"
continuation = "Use '--all-systems'."
[[diagnostic]]
id = "D-02"
implementation = "determinate"
match = "^warning: unchecked: .+$"
"#;

    fn small() -> Vec<Entry> {
        parse(SMALL).unwrap()
    }

    #[test]
    fn the_embedded_table_parses_and_every_fixture_is_classified_by_its_own_entry() {
        let entries = embedded().unwrap();
        assert!(entries.len() >= 5);
        let table: toml::Table = TABLE.parse().unwrap();
        for item in table["diagnostic"].as_array().unwrap() {
            let fixture = item["fixture"].as_str().unwrap();
            let implementation = item["implementation"].as_str().unwrap();
            let got = classify(&entries, implementation, fixture);
            assert_eq!(got.len(), 1, "{fixture}");
            assert_eq!(got[0].id.as_deref(), item["id"].as_str(), "{fixture}");
        }
    }

    #[test]
    fn parse_reads_effect_and_defaults_to_report() {
        let e = small();
        assert_eq!(
            (e[0].effect, e[1].effect, e[2].effect),
            (Effect::Report, Effect::Fail, Effect::Report)
        );
        assert_eq!(e[1].continuation.as_deref(), Some("Use '--all-systems'."));
    }

    #[test]
    fn a_bad_table_is_an_error_naming_the_problem() {
        assert!(
            parse("not toml =")
                .unwrap_err()
                .contains("diagnostics.toml")
        );
        let bad =
            "[[diagnostic]]\nid=\"x\"\nimplementation=\"c\"\neffect=\"maybe\"\nmatch=\"^a$\"\n";
        assert!(parse(bad).unwrap_err().contains("maybe"));
        let no_match = "[[diagnostic]]\nid=\"x\"\nimplementation=\"c\"\n";
        assert!(parse(no_match).unwrap_err().contains("`match`"));
        let regex = "[[diagnostic]]\nid=\"x\"\nimplementation=\"c\"\nmatch=\"(\"\n";
        assert!(parse(regex).unwrap_err().contains("bad match"));
    }

    #[test]
    fn a_known_line_takes_its_effect_and_an_unknown_one_is_unclassified() {
        let text = "checking x\nwarning: unknown flake output 'fooBar'\nwarning: something else\nerror: not a warning line\n";
        let got = classify(&small(), "cppnix", text);
        assert_eq!(got.len(), 2);
        assert_eq!(
            (got[0].id.as_deref(), got[0].effect),
            (Some("D-01"), Some(Effect::Report))
        );
        assert_eq!((got[1].id.clone(), got[1].effect), (None, None));
        assert_eq!(got[1].line, "warning: something else");
    }

    #[test]
    fn only_the_entries_of_the_running_implementation_apply() {
        let line = "warning: unchecked: fooBar.";
        assert_eq!(
            classify(&small(), "determinate", line)[0].id.as_deref(),
            Some("D-02")
        );
        assert_eq!(classify(&small(), "cppnix", line)[0].id, None);
    }

    #[test]
    fn a_continuation_line_is_not_a_diagnostic_of_its_own() {
        let text = "warning: omitted: x86_64-linux\nUse '--all-systems'.\nwarning: omitted: y\n";
        let got = classify(&small(), "cppnix", text);
        assert_eq!(got.len(), 2);
        assert!(got.iter().all(|c| c.id.as_deref() == Some("D-04")));
    }

    #[test]
    fn evaluation_warnings_are_not_diagnostics() {
        assert!(classify(&small(), "cppnix", "evaluation warning: renamed\n").is_empty());
    }
}
