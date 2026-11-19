//! `nixtops gate` and `nixtops report` on the command line: arguments, dry run, and the report
//! file. Runs that need Nix use a directory with no config, which stops before any layer.

mod common;
use common::{nixtops, stderr, stdout};
use std::fs;

fn dir_with(config: Option<&str>) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    if let Some(text) = config {
        fs::write(dir.path().join("nix-tops.toml"), text).unwrap();
    }
    dir
}

#[test]
fn a_directory_without_a_config_is_exit_two_and_writes_a_not_run_report() {
    let dir = dir_with(None);
    let out = nixtops(&["gate", dir.path().to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        stderr(&out).contains("nix-tops.toml: (file): cannot read"),
        "{}",
        stderr(&out)
    );
    let report: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.path().join(".nix-tops/report.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(report["passed"], false);
    assert_eq!(report["layers"].as_array().unwrap().len(), 7);
    assert!(stdout(&out).contains("gate: failed"));
}

#[test]
fn json_prints_the_report_and_text_prints_the_layers() {
    let dir = dir_with(None);
    let path = dir.path().to_str().unwrap();
    let json = nixtops(&["gate", path, "--json"]);
    let value: serde_json::Value = serde_json::from_str(&stdout(&json)).unwrap();
    assert_eq!(value["layers"][0]["id"], "source");
    let text = nixtops(&["gate", path]);
    assert!(
        stdout(&text).starts_with("source        not-run"),
        "{}",
        stdout(&text)
    );
}

#[test]
fn only_takes_a_layer_id() {
    let dir = dir_with(None);
    let out = nixtops(&["gate", dir.path().to_str().unwrap(), "--only", "nonsense"]);
    assert_eq!(out.status.code(), Some(4));
    assert!(stderr(&out).contains("--only takes one of source, lint, eval"));
}

#[test]
fn a_dry_run_shows_the_plan_and_runs_nothing() {
    let dir = dir_with(None);
    let out = nixtops(&["gate", dir.path().to_str().unwrap(), "--dry-run"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(stdout(&out).starts_with("gate (dry run) in "));
    assert!(stdout(&out).contains("  eval\n"));
    assert!(!dir.path().join(".nix-tops").exists());
}

#[test]
fn a_dry_run_is_refused_when_ci_is_set() {
    let dir = dir_with(None);
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_nixtops"))
        .args(["gate", dir.path().to_str().unwrap(), "--dry-run"])
        .env("CI", "true")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(4));
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        "nixtops gate: --dry-run is refused when CI is set\n"
    );
    assert!(dir.path().join(".nix-tops/report.json").is_file());
}

#[test]
fn report_renders_the_last_report_as_text_or_json_and_says_when_there_is_none() {
    let dir = dir_with(None);
    let path = dir.path().to_str().unwrap();
    let none = nixtops(&["report", path]);
    assert_eq!(none.status.code(), Some(3));
    assert!(stderr(&none).contains("no report at"));
    let _ = nixtops(&["gate", path]);
    let text = nixtops(&["report", path]);
    assert_eq!(text.status.code(), Some(0));
    assert!(stdout(&text).contains("gate: failed"));
    let json = nixtops(&["report", path, "--format", "json"]);
    let value: serde_json::Value = serde_json::from_str(&stdout(&json)).unwrap();
    assert_eq!(value["passed"], false);
}

#[test]
fn a_report_file_that_is_not_json_is_an_environment_error() {
    let dir = dir_with(None);
    fs::create_dir_all(dir.path().join(".nix-tops")).unwrap();
    fs::write(dir.path().join(".nix-tops/report.json"), "nope").unwrap();
    let out = nixtops(&["report", dir.path().to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(3));
    assert!(stderr(&out).contains("is not JSON"));
}
