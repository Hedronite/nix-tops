//! A run held against a baseline (section 6.8): what fails, by layer, and why a re-baseline is
//! due. The one comparison `gate` and `baseline` share, also used to hold a committed baseline
//! against the one before it. Which files changed since comes in as a function.

use super::{
    file::{Baseline, Tool},
    inputs::{self, deadnix_fails},
    widen::{self, Change},
};
use crate::report::LayerId;
use serde_json::Value;
use std::collections::BTreeMap;

/// True for a file that existed when the baseline was written and has not changed since.
pub type Unchanged<'a> = &'a dyn Fn(&str) -> bool;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Judgement {
    pub failures: Vec<(LayerId, String)>,
    pub notes: Vec<String>,
}

impl Judgement {
    fn fail(&mut self, layer: LayerId, why: String) {
        self.failures.push((layer, why));
    }

    /// The failures of one layer as one reason, or nothing.
    pub fn reason(&self, layer: LayerId) -> Option<String> {
        let found: Vec<&str> = self
            .failures
            .iter()
            .filter(|(id, _)| *id == layer)
            .map(|(_, why)| why.as_str())
            .collect();
        (!found.is_empty()).then(|| found.join("; "))
    }

    /// Every failure, for a message that is not tied to a layer.
    pub fn all(&self) -> String {
        let found: Vec<&str> = self.failures.iter().map(|(_, why)| why.as_str()).collect();
        found.join("; ")
    }
}

/// Counts that rose above the baseline's, in files `excused` does not cover; a file the baseline
/// does not name starts at 0.
fn rises(name: &str, old: Option<&Tool>, new: &Tool, excused: Unchanged<'_>, out: &mut Judgement) {
    for (file, found) in &new.files {
        let allowed = old.and_then(|t| t.files.get(file)).copied().unwrap_or(0);
        if *found > allowed && !excused(file) {
            let why = format!("{name}: {file}: {found} finding(s), baseline {allowed}");
            out.fail(LayerId::Lint, why);
        }
    }
}

/// How the configuration moved when the hashes differ; `None` when the files cannot say. The
/// files kept on each side must give that side's hash, so an edited `inputs` explains nothing.
fn moved(name: &str, old: &Tool, new: &Tool) -> Option<Change> {
    let kept = |tool: &'_ Tool| {
        let found = tool.inputs.clone()?;
        (inputs::hash(name, &found)? == tool.config_hash).then_some(found)
    };
    Some(widen::classify(name, &kept(old)?, &kept(new)?))
}

/// The configuration's verdict: `Ok(true)` when it is wider, `Err` when the tool fails on it.
fn configuration(name: &str, old: &Tool, new: &Tool, out: &mut Judgement) -> Result<bool, String> {
    if old.config_hash == new.config_hash {
        return Ok(false);
    }
    match moved(name, old, new) {
        Some(Change::Same) => {
            out.notes.push(format!(
                "{name}: the config file changed; run nixtops baseline"
            ));
            Ok(false)
        }
        Some(Change::Wider) => {
            out.notes.push(format!(
                "{name}: the config is wider than the baseline's; run nixtops baseline"
            ));
            Ok(true)
        }
        Some(Change::Narrower(why)) => Err(format!(
            "{name}: the config is narrower than the baseline's: {why}"
        )),
        None => Err(format!(
            "{name}: config_hash changed and the config files do not explain it \
             (an `exclude` in flake.nix, or a config file git does not track)"
        )),
    }
}

/// One tool: the configuration first, then the counts. A wider configuration may find more in
/// code nobody touched, so only there may a count rise.
fn tool(
    name: &str,
    old: &Tool,
    new: &Tool,
    counted: bool,
    unchanged: Unchanged<'_>,
    out: &mut Judgement,
) {
    let wider = match configuration(name, old, new, out) {
        Ok(wider) => wider,
        Err(why) => return out.fail(LayerId::Lint, why),
    };
    if counted {
        let excused = |file: &str| wider && unchanged(file);
        rises(name, Some(old), new, &excused, out);
    }
}

fn lint(
    doc: &Value,
    old: &BTreeMap<String, Tool>,
    new: &BTreeMap<String, Tool>,
    unchanged: Unchanged<'_>,
) -> Judgement {
    let mut out = Judgement::default();
    let counted = |name: &str| name != "deadnix" || deadnix_fails(doc);
    for (name, before) in old {
        match new.get(name) {
            Some(after) => tool(name, before, after, counted(name), unchanged, &mut out),
            None => out.fail(
                LayerId::Lint,
                format!("{name}: in the baseline and not run now"),
            ),
        }
    }
    // A tool the baseline does not have is the widest change of all: the same rule.
    for (name, after) in new.iter().filter(|(name, _)| !old.contains_key(*name)) {
        out.notes
            .push(format!("{name}: not in the baseline; run nixtops baseline"));
        if counted(name) {
            rises(name, None, after, unchanged, &mut out);
        }
    }
    out
}

/// `now` held against `old`.
pub fn judge(doc: &Value, old: &Baseline, now: &Baseline, unchanged: Unchanged<'_>) -> Judgement {
    let mut out = lint(doc, &old.lint, &now.lint, unchanged);
    if now.acks > old.acks {
        let why = format!("the ack count rose from {} to {}", old.acks, now.acks);
        out.fail(LayerId::Eval, why);
    }
    for law in old.laws.difference(&now.laws) {
        out.fail(
            LayerId::Laws,
            format!("{law}: judged in the baseline and not now"),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::baseline::inputs::Inputs;
    use serde_json::json;
    use std::collections::BTreeSet;

    /// A statix result whose hash is the one its config file gives.
    fn found(config: Option<&str>, files: &[(&str, usize)]) -> Tool {
        let kept = Inputs::from([("config".to_string(), config.map(str::to_string))]);
        Tool {
            version: "1".into(),
            config_hash: inputs::hash("statix", &kept).expect("hashes"),
            inputs: Some(kept),
            files: files.iter().map(|(f, n)| ((*f).to_string(), *n)).collect(),
        }
    }

    const NONE: Unchanged<'static> = &|_| false;

    fn only_old(file: &str) -> bool {
        file == "old.nix"
    }

    fn base(name: &str, tool: Tool) -> Baseline {
        Baseline {
            lint: BTreeMap::from([(name.to_string(), tool)]),
            ..Baseline::default()
        }
    }

    fn statix(old: Tool, new: Tool) -> Judgement {
        judge(
            &json!({}),
            &base("statix", old),
            &base("statix", new),
            &only_old,
        )
    }

    #[test]
    fn equal_or_fallen_counts_pass_and_a_rise_or_a_new_file_fails() {
        let old = found(None, &[("old.nix", 2)]);
        for same in [&[("old.nix", 2)][..], &[("old.nix", 1)], &[]] {
            let got = statix(old.clone(), found(None, same));
            assert_eq!(got, Judgement::default(), "{same:?}");
        }
        // Without a wider config an unchanged file gets no excuse.
        let got = statix(old.clone(), found(None, &[("old.nix", 3), ("b.nix", 1)]));
        assert_eq!(
            got.reason(LayerId::Lint).as_deref(),
            Some(
                "statix: b.nix: 1 finding(s), baseline 0; statix: old.nix: 3 finding(s), baseline 2"
            )
        );
        assert_eq!(got.reason(LayerId::Eval), None);
        assert_eq!(got.all(), got.reason(LayerId::Lint).unwrap());
    }

    #[test]
    fn a_narrower_config_fails_even_with_no_finding() {
        let got = statix(found(None, &[]), found(Some("disabled=['x']"), &[]));
        assert!(got.all().contains("narrower"), "{got:?}");
        assert!(got.all().contains("statix:disabled:x"));
    }

    #[test]
    fn a_wider_config_lets_a_count_rise_only_in_an_unchanged_file() {
        let narrow = found(Some("disabled=['x']"), &[]);
        let got = statix(narrow.clone(), found(None, &[("old.nix", 4)]));
        assert_eq!(got.failures, []);
        assert_eq!(
            got.notes,
            ["statix: the config is wider than the baseline's; run nixtops baseline"]
        );
        let got = statix(narrow, found(None, &[("old.nix", 4), ("s1.nix", 1)]));
        assert_eq!(got.all(), "statix: s1.nix: 1 finding(s), baseline 0");
    }

    #[test]
    fn the_same_meaning_under_a_new_hash_still_compares_counts() {
        let got = statix(found(None, &[]), found(Some("# note"), &[("old.nix", 1)]));
        assert_eq!(got.failures.len(), 1);
        assert!(got.notes[0].contains("the config file changed"));
    }

    #[test]
    fn a_hash_the_files_cannot_explain_fails_closed() {
        let unknown = Tool {
            inputs: None,
            config_hash: "i".into(),
            ..found(None, &[])
        };
        let forged = Tool {
            config_hash: "i".into(),
            ..found(Some("disabled=['x']"), &[])
        };
        for other in [unknown, forged] {
            let got = statix(found(None, &[]), other.clone());
            assert!(got.all().contains("do not explain"), "{got:?}");
            let got = statix(other, found(None, &[]));
            assert!(got.all().contains("do not explain"), "{got:?}");
        }
    }

    #[test]
    fn a_tool_that_stopped_running_fails_and_a_new_one_starts_at_zero_in_changed_files() {
        let old = base("statix", found(None, &[]));
        let new = base("nixfmt", found(None, &[("old.nix", 1), ("a.nix", 1)]));
        let got = judge(&json!({}), &old, &new, &only_old);
        assert_eq!(
            got.all(),
            "statix: in the baseline and not run now; nixfmt: a.nix: 1 finding(s), baseline 0"
        );
        assert_eq!(
            got.notes,
            ["nixfmt: not in the baseline; run nixtops baseline"]
        );
        let new = base("deadnix", found(None, &[("a.nix", 1)]));
        let off = json!({"lint": {"deadnix": {"fail": false}}});
        let got = judge(&off, &Baseline::default(), &new, NONE);
        assert_eq!(got.failures, []);
    }

    #[test]
    fn deadnix_counts_matter_only_while_it_fails() {
        let old = base("deadnix", found(None, &[]));
        let new = base("deadnix", found(None, &[("a.nix", 1)]));
        assert_eq!(judge(&json!({}), &old, &new, NONE).failures.len(), 1);
        let off = json!({"lint": {"deadnix": {"fail": false}}});
        assert_eq!(judge(&off, &old, &new, NONE).failures, []);
    }

    #[test]
    fn the_ack_count_must_not_rise_and_no_judged_law_may_be_lost() {
        let old = Baseline {
            acks: 1,
            laws: BTreeSet::from(["N-01".to_string(), "N-02".to_string()]),
            ..Baseline::default()
        };
        let same = judge(&json!({}), &old, &old, NONE);
        assert_eq!(same, Judgement::default());
        let now = Baseline {
            acks: 2,
            laws: BTreeSet::from(["N-02".to_string(), "N-03".to_string()]),
            ..Baseline::default()
        };
        let got = judge(&json!({}), &old, &now, NONE);
        assert_eq!(
            got.reason(LayerId::Eval).as_deref(),
            Some("the ack count rose from 1 to 2")
        );
        assert_eq!(
            got.reason(LayerId::Laws).as_deref(),
            Some("N-01: judged in the baseline and not now")
        );
        assert_eq!(
            judge(&json!({}), &now, &old, NONE).reason(LayerId::Eval),
            None
        );
    }
}
