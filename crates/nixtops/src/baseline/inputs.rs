//! What each lint tool's `config_hash` is taken over, read from the flake root the same way
//! `nix/lib/mkChecks.nix` selects it.
//!
//! `mkChecks` also hashes its `exclude` argument, which lives in `flake.nix` and is not read here.
//! [`hash`] reproduces the hash for an empty `exclude`, so a reported hash it does not equal means
//! the files do not tell the whole story, and the caller must not argue from them.

use super::sha256;
use serde_json::Value;
use std::{collections::BTreeMap, fs, io::ErrorKind, path::Path};

/// The named inputs of one tool's configuration; `None` is "the file is absent".
pub type Inputs = BTreeMap<String, Option<String>>;

/// `builtins.concatStringsSep` over an empty `exclude`, after its marker.
const NO_EXCLUDE: &str = "\nexclude:";

/// `Ok(None)` for an absent file; `Err` for one that exists and cannot be read as text.
fn optional(root: &Path, name: &str) -> Result<Option<String>, ()> {
    match fs::read_to_string(root.join(name)) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(_) => Err(()),
    }
}

fn treefmt(root: &Path) -> Result<Option<String>, ()> {
    match optional(root, "treefmt.toml")? {
        Some(text) => Ok(Some(text)),
        None => optional(root, ".treefmt.toml"),
    }
}

fn statix_path(doc: &Value) -> &str {
    doc.pointer("/lint/statix/config")
        .and_then(Value::as_str)
        .unwrap_or("statix.toml")
}

/// `lint.deadnix.fail`, on unless the config turns it off.
pub fn deadnix_fails(doc: &Value) -> bool {
    doc.pointer("/lint/deadnix/fail")
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

fn named(entries: Vec<(&str, Option<String>)>) -> Inputs {
    entries
        .into_iter()
        .map(|(name, value)| (name.to_string(), value))
        .collect()
}

fn read(root: &Path, doc: &Value, tool: &str) -> Result<Inputs, ()> {
    let entries = match tool {
        "nixfmt" => vec![("config", treefmt(root)?)],
        "statix" => vec![("config", optional(root, statix_path(doc))?)],
        "deadnix" => vec![("fail", Some(deadnix_fails(doc).to_string()))],
        "gitleaks" => vec![
            ("config", optional(root, ".gitleaks.toml")?),
            ("ignore", optional(root, ".gitleaksignore")?),
        ],
        _ => return Err(()),
    };
    Ok(named(entries))
}

fn value<'a>(inputs: &'a Inputs, name: &str) -> Option<&'a str> {
    inputs.get(name).and_then(Option::as_deref)
}

/// The text `mkChecks` hashes for this tool when `exclude` is empty.
fn preimage(tool: &str, inputs: &Inputs) -> Option<String> {
    let config = value(inputs, "config");
    let body = match tool {
        "nixfmt" => config.unwrap_or("default-nixfmt").to_string(),
        "statix" => config.unwrap_or("default").to_string(),
        "deadnix" => format!("fail={}", value(inputs, "fail")?),
        "gitleaks" => format!(
            "{}|{}",
            config.unwrap_or("default"),
            value(inputs, "ignore").unwrap_or("")
        ),
        _ => return None,
    };
    Some(body + NO_EXCLUDE)
}

/// The `config_hash` these inputs give with an empty `exclude`.
pub fn hash(tool: &str, inputs: &Inputs) -> Option<String> {
    preimage(tool, inputs).map(|text| sha256::hex(text.as_bytes()))
}

/// The inputs of `tool`, only when they reproduce the reported `config_hash`.
pub fn verified(root: &Path, doc: &Value, tool: &str, config_hash: &str) -> Option<Inputs> {
    let inputs = read(root, doc, tool).ok()?;
    (hash(tool, &inputs)?.as_str() == config_hash).then_some(inputs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn of(tool: &str, root: &Path, doc: &Value) -> String {
        hash(tool, &read(root, doc, tool).expect("reads")).expect("hashes")
    }

    #[test]
    fn the_preimages_follow_mk_checks() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (root, doc) = (dir.path(), json!({}));
        for (tool, text) in [
            ("nixfmt", "default-nixfmt\nexclude:"),
            ("statix", "default\nexclude:"),
            ("deadnix", "fail=true\nexclude:"),
            ("gitleaks", "default|\nexclude:"),
        ] {
            assert_eq!(of(tool, root, &doc), sha256::hex(text.as_bytes()), "{tool}");
        }
    }

    #[test]
    fn each_tool_reads_the_files_mk_checks_reads() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let write = |name: &str, text: &str| fs::write(root.join(name), text).expect("write");
        write(".treefmt.toml", "hidden");
        let doc = json!({"lint": {"statix": {"config": "s.toml"}, "deadnix": {"fail": false}}});
        assert_eq!(
            value(&read(root, &doc, "nixfmt").unwrap(), "config"),
            Some("hidden")
        );
        write("treefmt.toml", "plain");
        assert_eq!(
            value(&read(root, &doc, "nixfmt").unwrap(), "config"),
            Some("plain")
        );
        write("s.toml", "S");
        write(".gitleaks.toml", "G");
        write(".gitleaksignore", "I");
        assert_eq!(of("statix", root, &doc), sha256::hex(b"S\nexclude:"));
        assert_eq!(
            of("deadnix", root, &doc),
            sha256::hex(b"fail=false\nexclude:")
        );
        assert_eq!(of("gitleaks", root, &doc), sha256::hex(b"G|I\nexclude:"));
    }

    #[test]
    fn inputs_are_given_only_when_they_explain_the_reported_hash() {
        let dir = tempfile::tempdir().expect("tempdir");
        let doc = json!({});
        let good = sha256::hex(b"default\nexclude:");
        assert!(verified(dir.path(), &doc, "statix", &good).is_some());
        let excluded = sha256::hex(b"default\nexclude:a.nix");
        assert_eq!(verified(dir.path(), &doc, "statix", &excluded), None);
        assert_eq!(verified(dir.path(), &doc, "unknown", &good), None);
        fs::write(dir.path().join("statix.toml"), [0xff, 0xfe]).expect("write");
        assert_eq!(verified(dir.path(), &doc, "statix", &good), None);
        assert_eq!(preimage("unknown", &Inputs::new()), None);
        assert_eq!(preimage("deadnix", &Inputs::new()), None);
    }
}
