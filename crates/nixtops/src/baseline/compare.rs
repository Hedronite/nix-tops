//! A run held against a baseline (section 6.8): what fails, by layer, and why a re-baseline is
//! due. Pure but for the config files, and the one comparison `gate` and `baseline` share.

use super::{
    file::{Baseline, Tool},
    inputs::deadnix_fails,
    widen::{self, Change},
};
use crate::report::LayerId;
use serde_json::Value;
use std::collections::BTreeMap;

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

/// Counts that rose above the baseline's; a file the baseline does not name starts at 0.
fn rises(name: &str, old: &Tool, new: &Tool, out: &mut Judgement) {
    for (file, found) in &new.files {
        let allowed = old.files.get(file).copied().unwrap_or(0);
        if *found > allowed {
            let why = format!("{name}: {file}: {found} finding(s), baseline {allowed}");
            out.fail(LayerId::Lint, why);
        }
    }
}

/// How the configuration moved when the hashes differ; `None` when the files cannot say.
fn moved(name: &str, old: &Tool, new: &Tool) -> Option<Change> {
    let (before, after) = (old.inputs.as_ref()?, new.inputs.as_ref()?);
    Some(widen::classify(name, before, after))
}

/// One tool: the configuration first, then the counts. A wider configuration may find more in
/// files that did not change, so its counts are recorded by the next baseline, not compared.
fn tool(name: &str, old: &Tool, new: &Tool, counted: bool, out: &mut Judgement) {
    if old.config_hash != new.config_hash {
        match moved(name, old, new) {
            Some(Change::Same) => out.notes.push(format!(
                "{name}: the config file changed; run nixtops baseline"
            )),
            Some(Change::Wider) => {
                out.notes.push(format!(
                    "{name}: the config is wider than the baseline's; run nixtops baseline"
                ));
                return;
            }
            Some(Change::Narrower(why)) => {
                let why = format!("{name}: the config is narrower than the baseline's: {why}");
                return out.fail(LayerId::Lint, why);
            }
            None => {
                let why = format!(
                    "{name}: config_hash changed and the config files do not explain it \
                     (an `exclude` in flake.nix, or a config file git does not track)"
                );
                return out.fail(LayerId::Lint, why);
            }
        }
    }
    if counted {
        rises(name, old, new, out);
    }
}

fn lint(doc: &Value, old: &BTreeMap<String, Tool>, new: &BTreeMap<String, Tool>) -> Judgement {
    let mut out = Judgement::default();
    for (name, before) in old {
        let counted = name != "deadnix" || deadnix_fails(doc);
        match new.get(name) {
            Some(after) => tool(name, before, after, counted, &mut out),
            None => out.fail(
                LayerId::Lint,
                format!("{name}: in the baseline and not run now"),
            ),
        }
    }
    for name in new.keys().filter(|name| !old.contains_key(*name)) {
        out.notes
            .push(format!("{name}: not in the baseline; run nixtops baseline"));
    }
    out
}

/// `now` held against `old`.
pub fn judge(doc: &Value, old: &Baseline, now: &Baseline) -> Judgement {
    let mut out = lint(doc, &old.lint, &now.lint);
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

    fn found(hash: &str, config: Option<&str>, files: &[(&str, usize)]) -> Tool {
        Tool {
            version: "1".into(),
            config_hash: hash.into(),
            inputs: Some(Inputs::from([(
                "config".to_string(),
                config.map(str::to_string),
            )])),
            files: files.iter().map(|(f, n)| ((*f).to_string(), *n)).collect(),
        }
    }

    fn base(name: &str, tool: Tool) -> Baseline {
        Baseline {
            lint: BTreeMap::from([(name.to_string(), tool)]),
            ..Baseline::default()
        }
    }

    fn statix(old: Tool, new: Tool) -> Judgement {
        judge(&json!({}), &base("statix", old), &base("statix", new))
    }

    #[test]
    fn equal_or_fallen_counts_pass_and_a_rise_or_a_new_file_fails() {
        let old = found("h", None, &[("a.nix", 2)]);
        for same in [&[("a.nix", 2)][..], &[("a.nix", 1)], &[]] {
            let got = statix(old.clone(), found("h", None, same));
            assert_eq!(got, Judgement::default(), "{same:?}");
        }
        let got = statix(old.clone(), found("h", None, &[("a.nix", 3), ("b.nix", 1)]));
        assert_eq!(
            got.reason(LayerId::Lint).as_deref(),
            Some(
                "statix: a.nix: 3 finding(s), baseline 2; statix: b.nix: 1 finding(s), baseline 0"
            )
        );
        assert_eq!(got.reason(LayerId::Eval), None);
        assert_eq!(got.all(), got.reason(LayerId::Lint).unwrap());
    }

    #[test]
    fn a_narrower_config_fails_even_with_no_finding() {
        let got = statix(
            found("h", None, &[]),
            found("i", Some("disabled=['x']"), &[]),
        );
        assert!(got.all().contains("narrower"), "{got:?}");
        assert!(got.all().contains("statix:disabled:x"));
    }

    #[test]
    fn a_wider_config_passes_with_the_message_and_does_not_compare_counts() {
        let got = statix(
            found("h", Some("disabled=['x']"), &[]),
            found("i", None, &[("a.nix", 4)]),
        );
        assert_eq!(got.failures, []);
        assert_eq!(
            got.notes,
            ["statix: the config is wider than the baseline's; run nixtops baseline"]
        );
    }

    #[test]
    fn the_same_meaning_under_a_new_hash_still_compares_counts() {
        let got = statix(
            found("h", None, &[]),
            found("i", Some("# note"), &[("a.nix", 1)]),
        );
        assert_eq!(got.failures.len(), 1);
        assert!(got.notes[0].contains("the config file changed"));
    }

    #[test]
    fn a_hash_the_files_cannot_explain_fails_closed() {
        let unknown = Tool {
            inputs: None,
            ..found("i", None, &[])
        };
        let got = statix(found("h", None, &[]), unknown.clone());
        assert!(got.all().contains("do not explain"), "{got:?}");
        let got = statix(unknown, found("h", None, &[]));
        assert!(got.all().contains("do not explain"), "{got:?}");
    }

    #[test]
    fn a_tool_that_stopped_running_fails_and_a_new_one_asks_for_a_baseline() {
        let old = base("statix", found("h", None, &[]));
        let new = base("nixfmt", found("h", None, &[("a.nix", 1)]));
        let got = judge(&json!({}), &old, &new);
        assert_eq!(got.all(), "statix: in the baseline and not run now");
        assert_eq!(
            got.notes,
            ["nixfmt: not in the baseline; run nixtops baseline"]
        );
    }

    #[test]
    fn deadnix_counts_matter_only_while_it_fails() {
        let old = base("deadnix", found("h", None, &[]));
        let new = base("deadnix", found("h", None, &[("a.nix", 1)]));
        assert_eq!(judge(&json!({}), &old, &new).failures.len(), 1);
        let off = json!({"lint": {"deadnix": {"fail": false}}});
        assert_eq!(judge(&off, &old, &new).failures, []);
    }

    #[test]
    fn the_ack_count_must_not_rise_and_no_judged_law_may_be_lost() {
        let old = Baseline {
            acks: 1,
            laws: BTreeSet::from(["N-01".to_string(), "N-02".to_string()]),
            ..Baseline::default()
        };
        let same = judge(&json!({}), &old, &old);
        assert_eq!(same, Judgement::default());
        let now = Baseline {
            acks: 2,
            laws: BTreeSet::from(["N-02".to_string(), "N-03".to_string()]),
            ..Baseline::default()
        };
        let got = judge(&json!({}), &old, &now);
        assert_eq!(
            got.reason(LayerId::Eval).as_deref(),
            Some("the ack count rose from 1 to 2")
        );
        assert_eq!(
            got.reason(LayerId::Laws).as_deref(),
            Some("N-01: judged in the baseline and not now")
        );
        assert_eq!(judge(&json!({}), &now, &old).reason(LayerId::Eval), None);
    }
}
