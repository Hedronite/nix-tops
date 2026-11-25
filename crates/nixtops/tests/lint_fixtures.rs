//! Real-Nix tests for gate 2 (`lib.mkChecks` and `nixtops gate` lint layer). Skipped unless
//! `NIXTOPS_FX_JUDGE`, `NIXTOPS_FX_EVAL_JOBS`, and `NIXTOPS_FX_NIXPKGS` are set.

use nixtops::{
    exit::Exit,
    gate::{self, Evaluator, Seams, Settings},
    inventory::Tools,
};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

fn tools() -> Option<Tools> {
    Tools::from_parts(
        &env("NIXTOPS_FX_JUDGE")?,
        &env("NIXTOPS_FX_EVAL_JOBS")?,
        &env("NIXTOPS_FX_NIXPKGS")?,
    )
}

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap()
        .status
        .success();
    assert!(ok, "git {args:?}");
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).unwrap();
        }
    }
}

fn scratch_repo(fixture: &Path) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    copy_dir(fixture, dir.path());
    git(dir.path(), &["init", "-q", "."]);
    git(dir.path(), &["config", "user.email", "a@b"]);
    git(dir.path(), &["config", "user.name", "n"]);
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-q", "-m", "fixture"]);
    dir
}

#[test]
fn deadnix_and_statix_findings_fail_hard_and_clean_passes_with_real_tools() {
    let Some(tools) = tools() else {
        eprintln!("skipped: NIXTOPS_FX_* not set");
        return;
    };
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/gates");

    for (name, want_exit, want_tool) in [
        ("lint-clean", Exit::Pass, None),
        ("lint-deadnix", Exit::GateFailed, Some("deadnix")),
        ("lint-statix", Exit::GateFailed, Some("statix")),
    ] {
        let repo = scratch_repo(&root.join(name));
        let settings = Settings {
            root: repo.path().to_path_buf(),
            keep_going: false,
            only: None,
            system: None,
            evaluator: Evaluator::Pinned,
            ci: false,
            today: gate::run::today(),
            nix: nixtops::probe::nix_bin(),
        };
        let res = gate::run(&settings, Some(tools.clone()), &Seams::default());
        assert_eq!(res.exit, want_exit, "{name}: {:?}", res.report);
        let report: Value = serde_json::to_value(&res.report).unwrap();
        assert_eq!(report["lint"]["tools"].as_array().unwrap().len(), 4);
        if let Some(tool) = want_tool {
            let entry = report["lint"]["tools"]
                .as_array()
                .unwrap()
                .iter()
                .find(|t| t["tool"] == tool)
                .unwrap();
            assert!(
                entry["files"]["flake.nix"].as_u64().unwrap() > 0,
                "{name}: {entry}"
            );
        }
    }
}
