//! Reading the outputs with stand-in tools: a shell script that answers the judge's calls from a
//! table, so `check` and `init` run end to end without a real Nix.

mod common;
use common::fake_nix;
use nixtops::{
    cmd::{check, init},
    config::outputs::{self, Read},
    inventory::Tools,
};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};
use tempfile::tempdir;

fn judge(dir: &Path, arms: &[(&str, &str, i32)]) -> PathBuf {
    let mut text = String::from("#!/bin/sh\ncase \"$*\" in\n");
    for (pattern, stdout, code) in arms {
        text.push_str(&format!(
            "*'{pattern}'*)\ncat <<'__OUT__'\n{stdout}\n__OUT__\nexit {code};;\n"
        ));
    }
    text.push_str("*) echo \"no arm for: $*\" >&2; exit 99;;\nesac\n");
    let path = dir.join("judge");
    fs::write(&path, text).expect("write");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod");
    path
}

const OK: [(&str, &str, i32); 4] = [
    (
        "flake'*'metadata'*'--no-update-lock-file",
        r#"{"url":"fake:locked"}"#,
        0,
    ),
    (r##"#. --apply"##, r#"["devShells","packages"]"#, 0),
    (
        r##"#."devShells" --apply"##,
        r#"["aarch64-linux","x86_64-linux"]"#,
        0,
    ),
    (
        r##"#."packages" --apply"##,
        r#"["aarch64-linux","x86_64-linux"]"#,
        0,
    ),
];

fn tools(judge: PathBuf) -> Option<Tools> {
    Tools::from_parts(judge.to_str().expect("utf8"), "eval-jobs", "nixpkgs")
}

const GOOD: &str = "protocol = \"0.2.0\"\nfacets = [\"dev-flake\"]\nenforce = \"hard\"\n\n[toolchain]\nsystems = [\"aarch64-linux\", \"x86_64-linux\"]\n";

#[test]
fn read_reports_no_judge_a_failing_judge_and_a_found_structure() {
    let dir = tempdir().expect("tempdir");
    assert_eq!(outputs::read_with(None, dir.path()), Read::NoJudge);
    let bad = judge(
        dir.path(),
        &[("flake'*'metadata'*'--no-update-lock-file", "", 1)],
    );
    assert!(matches!(
        outputs::read_with(tools(bad), dir.path()),
        Read::Failed(_)
    ));
    let good = judge(dir.path(), &OK);
    let Read::Found(s) = outputs::read_with(tools(good), dir.path()) else {
        panic!("not found")
    };
    assert_eq!(s.systems().len(), 2);
}

#[test]
fn found_says_why_there_is_no_structure() {
    assert!(
        outputs::found(Read::NoJudge)
            .expect_err("no judge")
            .contains("no pinned judge")
    );
    assert!(
        outputs::found(Read::Failed("boom".into()))
            .expect_err("failed")
            .contains("boom")
    );
    assert!(outputs::found(Read::Found(Default::default())).is_ok());
}

fn args(dir: &Path) -> check::Args {
    check::Args {
        config: dir.join("nix-tops.toml"),
    }
}

#[test]
fn check_runs_end_to_end_with_stand_in_tools() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("nix-tops.toml"), GOOD).expect("write");
    let nix = fake_nix(dir.path(), "nix (Nix) 2.35.2");
    let nix = nix.to_str().expect("utf8");
    let pass = check::run_with(
        &args(dir.path()),
        "2026-11-18",
        nix,
        tools(judge(dir.path(), &OK)),
    );
    assert_eq!(pass.exit.code(), 0, "{}", pass.stderr);
    let none = check::run_with(&args(dir.path()), "2026-11-18", nix, None);
    assert_eq!(none.exit.code(), 3);
    assert!(none.stderr.contains("outputs") && none.stderr.contains("no pinned judge"));
    let down = judge(
        dir.path(),
        &[("flake'*'metadata'*'--no-update-lock-file", "", 1)],
    );
    let failed = check::run_with(&args(dir.path()), "2026-11-18", nix, tools(down));
    assert_eq!(failed.exit.code(), 3);
}

#[test]
fn init_run_in_reads_the_outputs_with_the_given_tools() {
    let dir = tempdir().expect("tempdir");
    let given = init::Args {
        facets: vec![],
        force: false,
    };
    let out = init::run_in(dir.path(), &given, tools(judge(dir.path(), &OK)));
    assert_eq!(out.exit.code(), 0, "{}", out.stderr);
    let toml = fs::read_to_string(dir.path().join("nix-tops.toml")).expect("written");
    assert!(
        toml.contains("facets = [\"dev-flake\", \"package-set\"]"),
        "{toml}"
    );
}
