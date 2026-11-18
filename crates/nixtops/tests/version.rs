//! `version` prints values from the build. Nothing is typed into the source: a scan fails if the
//! protocol id or a pinned tool version appears as a literal in `src/`.

mod common;
use common::{nixtops, repo_root, stdout};
use std::{fs, path::Path};

fn protocol_from_schema() -> String {
    let text = fs::read_to_string(repo_root().join("schema/nix-tops.schema.json")).unwrap();
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    json["properties"]["protocol"]["const"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn version_prints_the_schema_protocol_the_package_and_the_pinned_tools() {
    let text = stdout(&nixtops(&["version"]));
    assert!(
        text.starts_with(&format!("nixtops {}\n", env!("CARGO_PKG_VERSION"))),
        "{text}"
    );
    assert!(
        text.contains(&format!("protocol {}\n", protocol_from_schema())),
        "{text}"
    );
    let pinned = option_env!("NIXTOPS_TOOLS").unwrap_or_default();
    for entry in pinned.split(';').filter(|entry| !entry.is_empty()) {
        let (name, version) = entry.split_once('=').unwrap();
        assert!(
            text.contains(&format!("  {name} {version}\n")),
            "{name}: {text}"
        );
    }
    if pinned.is_empty() {
        assert!(text.contains("none pinned"), "{text}");
    }
}

fn rust_sources(dir: &Path, found: &mut Vec<String>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_sources(&path, found);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            found.push(fs::read_to_string(&path).unwrap());
        }
    }
}

#[test]
fn no_version_literal_in_the_source() {
    let mut files = Vec::new();
    rust_sources(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    let mut banned = vec![protocol_from_schema()];
    let pinned = option_env!("NIXTOPS_TOOLS").unwrap_or_default();
    banned.extend(
        pinned
            .split(';')
            .filter_map(|e| e.split_once('='))
            .map(|(_, v)| v.to_string()),
    );
    for version in banned {
        assert!(
            files.iter().all(|text| !text.contains(&version)),
            "literal {version} in src/"
        );
    }
}
