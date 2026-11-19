//! `doctor` reports the host and gates only on the daemon floor.

mod common;
use common::{fake_nix, nixtops, stderr, stdout};
use nixtops::cmd::doctor;
use std::process::Command;

fn run_with(version_line: &str) -> (Option<i32>, String) {
    let dir = tempfile::tempdir().unwrap();
    let nix = fake_nix(dir.path(), version_line);
    let out = Command::new(env!("CARGO_BIN_EXE_nixtops"))
        .arg("doctor")
        .env("NIXTOPS_NIX", &nix)
        .output()
        .unwrap();
    (out.status.code(), stdout(&out))
}

#[test]
fn the_floor_table() {
    let cases = [
        ("nix (Nix) 2.23.4", Some(3)),
        ("nix (Nix) 2.24.0", Some(0)),
        ("nix (Nix) 2.35.2", Some(0)),
        ("nix (Determinate Nix 3.22.3) 2.35.2", Some(0)),
        ("nix (Determinate Nix 2.9.0) 2.30.0", Some(3)),
        ("nix (Lix, like Nix) 2.95.0", Some(3)),
        ("nix (Mystery Nix) 2.90.0", Some(3)),
    ];
    for (line, code) in cases {
        assert_eq!(run_with(line).0, code, "{line}");
    }
}

#[test]
fn doctor_prints_every_field() {
    let (_, text) = run_with("nix (Determinate Nix 3.22.3) 2.35.2");
    for field in [
        "judge:",
        "nix: Determinate Nix 3.22.3 (reports Nix 2.35.2)",
        "daemon: daemon 9.9.9",
        "sandbox: true",
        "experimental-features: nix-command flakes",
        "kvm: ",
        "remote builders: ",
        "substituters: https://cache.example/",
        "lazy-trees: false",
        "eval-cores: absent",
        "floor: met",
    ] {
        assert!(text.contains(field), "missing {field:?} in\n{text}");
    }
}

#[test]
fn below_the_floor_says_why() {
    let (_, text) = run_with("nix (Nix) 2.23.4");
    assert!(
        text.contains("floor: not met: CppNix 2.23.4 is below 2.24"),
        "{text}"
    );
}

#[test]
fn doctor_without_nix_is_an_environment_error() {
    let out = Command::new(env!("CARGO_BIN_EXE_nixtops"))
        .arg("doctor")
        .env("NIXTOPS_NIX", "/nonexistent/nix")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
    assert!(stderr(&out).contains("cannot read the version"));
}

#[test]
fn report_marks_kvm_and_names_the_judge() {
    let dir = tempfile::tempdir().unwrap();
    let nix = fake_nix(dir.path(), "nix (Nix) 2.35.2");
    let text = doctor::report(nix.to_str().unwrap(), true).stdout;
    assert!(text.contains("kvm: yes"), "{text}");
    assert!(text.contains("judge: "), "{text}");
    assert!(nixtops(&["doctor"]).status.code().is_some());
}
