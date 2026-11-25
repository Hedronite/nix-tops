//! `nixtops baseline` as a command: the exit code and the message when nothing can be written.

mod common;
use common::{nixtops, stderr};
use std::fs;

#[test]
fn a_config_that_does_not_load_exits_2_and_writes_nothing_outside_a_repository_too() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("nix-tops.toml"), "protocol = ").unwrap();
    let path = dir.path().display().to_string();
    for evaluator in ["pinned", "host"] {
        let out = nixtops(&["baseline", &path, "--evaluator", evaluator]);
        assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
        assert!(stderr(&out).contains("(file)"), "{}", stderr(&out));
        assert!(stderr(&out).contains("not written"), "{}", stderr(&out));
    }
    assert!(!dir.path().join(".nix-tops/baseline.json").exists());
}

#[test]
fn outside_a_repository_the_command_refuses_with_exit_4() {
    let dir = tempfile::tempdir().unwrap();
    let config = "protocol = \"0.2.0\"\nfacets = [\"dev-flake\"]\n\n[toolchain]\nsystems = [\"x86_64-linux\"]\n";
    fs::write(dir.path().join("nix-tops.toml"), config).unwrap();
    let out = nixtops(&["baseline", &dir.path().display().to_string()]);
    assert_eq!(out.status.code(), Some(4), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("not a git repository"),
        "{}",
        stderr(&out)
    );
}
