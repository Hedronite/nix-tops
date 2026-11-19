//! `fixtures/gates/*`: each flake run through the gate with the real tools, and its verdicts
//! compared with `expected.json`. Needs network and Nix, so it runs only when the tools are given:
//! `NIXTOPS_FX_JUDGE`, `NIXTOPS_FX_EVAL_JOBS`, `NIXTOPS_FX_NIXPKGS` (and `NIXTOPS_FX_EVALUATOR=host`
//! for the host's Nix, `NIXTOPS_FX_REPORTS` for a directory to copy the reports to). A normal
//! `cargo test` and the Nix build skip it.

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

/// The fixture as its own git repository, so the gate's source layer sees a clean tracked tree.
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

fn evaluator() -> Evaluator {
    if env("NIXTOPS_FX_EVALUATOR").as_deref() == Some("host") {
        Evaluator::Host
    } else {
        Evaluator::Pinned
    }
}

fn exit_code(exit: Exit) -> i64 {
    i64::from(exit.code())
}

fn fixtures() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/gates");
    let mut dirs: Vec<PathBuf> = fs::read_dir(root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    dirs
}

#[test]
fn every_gate_fixture_has_the_verdicts_its_expected_json_says() {
    let Some(tools) = tools() else {
        eprintln!("skipped: NIXTOPS_FX_* not set");
        return;
    };
    let evaluator = if env("NIXTOPS_FX_EVALUATOR").as_deref() == Some("host") {
        Evaluator::Host
    } else {
        Evaluator::Pinned
    };
    let mut wrong = Vec::new();
    for fixture in fixtures() {
        let name = fixture.file_name().unwrap().to_string_lossy().to_string();
        let expected: Value =
            serde_json::from_str(&fs::read_to_string(fixture.join("expected.json")).unwrap())
                .unwrap();
        let repo = scratch_repo(&fixture);
        let settings = Settings {
            root: repo.path().to_path_buf(),
            keep_going: false,
            only: None,
            system: None,
            evaluator,
            ci: false,
            today: gate::run::today(),
            nix: nixtops::probe::nix_bin(),
        };
        let result = gate::run(&settings, Some(tools.clone()), &Seams::default());
        let report = serde_json::to_value(&result.report).unwrap();
        if let Some(dir) = env("NIXTOPS_FX_REPORTS") {
            fs::create_dir_all(&dir).unwrap();
            fs::write(
                Path::new(&dir).join(format!("{name}.json")),
                serde_json::to_string_pretty(&report).unwrap() + "\n",
            )
            .unwrap();
        }
        let verdict = |id: &str| {
            report["layers"]
                .as_array()
                .unwrap()
                .iter()
                .find(|l| l["id"] == id)
                .unwrap()["verdict"]
                .clone()
        };
        let named = expected["named"].as_str().unwrap();
        let ok = exit_code(result.exit) == expected["exit"].as_i64().unwrap()
            && verdict("source") == expected["layers"]["source"]
            && verdict("eval") == expected["layers"]["eval"]
            && (named.is_empty()
                || report.to_string().contains(named)
                || result.problems.join(" ").contains(named));
        eprintln!(
            "{name}: exit {} source {} eval {} {}",
            exit_code(result.exit),
            verdict("source"),
            verdict("eval"),
            if ok { "ok" } else { "DIFF" }
        );
        if !ok && expected.get("pending").is_none() {
            wrong.push(name);
        }
    }
    assert!(wrong.is_empty(), "fixtures that differ: {wrong:?}");
}

/// A dirty git tree: a tracked file edited and not committed, and an untracked `.nix` file. The
/// inventory reads the flake the way `nix flake check` does, from its tracked files, so the edit
/// is seen and the untracked file is not; gate 1 judges the dirty tree itself.
#[test]
fn a_dirty_tree_is_evaluated_from_its_tracked_files_and_gate_one_still_refuses_it() {
    let Some(tools) = tools() else {
        eprintln!("skipped: NIXTOPS_FX_* not set");
        return;
    };
    let clean = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/gates/clean");
    let repo = scratch_repo(&clean);
    let flake = repo.path().join("flake.nix");
    let edited = fs::read_to_string(&flake).unwrap().replace(
        "packages.aarch64-linux.default",
        "packages.x86_64-linux.second = builtins.derivation { name = \"second\"; system = \"x86_64-linux\"; builder = \"/bin/sh\"; };\n    packages.aarch64-linux.default",
    );
    fs::write(&flake, edited).unwrap();
    fs::write(repo.path().join("untracked.nix"), "throw \"never read\"\n").unwrap();
    let settings = Settings {
        root: repo.path().to_path_buf(),
        keep_going: true,
        only: None,
        system: None,
        evaluator: evaluator(),
        ci: false,
        today: gate::run::today(),
        nix: nixtops::probe::nix_bin(),
    };
    // nix prints `warning: Git tree ...` for a dirty tree; laws/diagnostics.toml classifies it.
    let result = gate::run(&settings, Some(tools), &Seams::default());
    let report = serde_json::to_value(&result.report).unwrap();
    let layer = |id: &str| {
        report["layers"]
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["id"] == id)
            .unwrap()
            .clone()
    };
    assert_eq!(layer("source")["verdict"], "Refuse");
    assert!(
        layer("source")["reason"]
            .as_str()
            .unwrap()
            .contains("untracked.nix is not tracked by git")
    );
    assert_eq!(layer("eval")["verdict"], "pass", "{}", layer("eval"));
    let attrs: Vec<&str> = report["outputs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["attr"].as_str().unwrap())
        .collect();
    assert!(attrs.contains(&"packages.x86_64-linux.second"), "{attrs:?}");
}
