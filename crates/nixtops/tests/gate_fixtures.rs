//! `fixtures/gates/*`: each flake run through the gate with the real tools, and its verdicts
//! compared with `expected.json`. Needs network and Nix, so it runs only when the tools are given:
//! `NIXTOPS_FX_JUDGE`, `NIXTOPS_FX_EVAL_JOBS`, `NIXTOPS_FX_NIXPKGS` (and `NIXTOPS_FX_EVALUATOR=host`
//! for the host's Nix, `NIXTOPS_FX_REPORTS` for a directory to copy the reports to). A normal
//! `cargo test` and the Nix build skip it.

use nixtops::{
    exit::Exit,
    gate::{self, Evaluator, Seams, Settings},
    inventory::{self, Backend, Session, Tools},
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

/// A flake with a template, from a source that is not in the store yet: the template must be
/// copied from a realised path (a comment with a fresh nonce makes the source new on every run).
#[test]
fn a_template_in_a_fresh_source_is_copied_from_the_store() {
    let Some(tools) = tools() else {
        eprintln!("skipped: NIXTOPS_FX_* not set");
        return;
    };
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/corpus/ctl-flake-lib");
    let repo = tempfile::tempdir().unwrap();
    copy_dir(&corpus, repo.path());
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let flake = repo.path().join("flake.nix");
    let text = fs::read_to_string(&flake).unwrap();
    fs::write(
        &flake,
        format!("{text}\n# fresh source {nonce} {}\n", std::process::id()),
    )
    .unwrap();
    git(repo.path(), &["init", "-q", "."]);
    git(repo.path(), &["config", "user.email", "a@b"]);
    git(repo.path(), &["config", "user.name", "n"]);
    git(repo.path(), &["add", "-A"]);
    git(repo.path(), &["commit", "-q", "-m", "fresh"]);
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
    let result = gate::run(&settings, Some(tools), &Seams::default());
    let report = serde_json::to_value(&result.report).unwrap();
    let eval = report["layers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["id"] == "eval")
        .unwrap()
        .clone();
    assert_eq!(eval["verdict"], "pass", "{eval}");
    let outputs = report["outputs"].to_string();
    assert!(outputs.contains("templates.default"), "{outputs}");
}

/// `--system` narrows: a throw on `aarch64-linux` does not fail a run that selects `x86_64-linux`,
/// and the same throw on the selected system still does.
#[test]
fn a_selected_system_narrows_the_run_with_the_real_tools() {
    let Some(tools) = tools() else {
        eprintln!("skipped: NIXTOPS_FX_* not set");
        return;
    };
    let clean = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/gates/clean");
    let verdict_for = |broken: &str, selected: &str| {
        let repo = scratch_repo(&clean);
        let flake = repo.path().join("flake.nix");
        let text = fs::read_to_string(&flake).unwrap();
        let needle = format!("packages.{broken}.default = ");
        let start = text.find(&needle).unwrap();
        let end = text[start..].find(";\n").unwrap() + start;
        let mut edited = text.clone();
        edited.replace_range(start + needle.len()..end, "throw \"qa broken\"");
        fs::write(&flake, edited).unwrap();
        git(repo.path(), &["add", "-A"]);
        git(repo.path(), &["commit", "-q", "-m", "throw"]);
        let settings = Settings {
            root: repo.path().to_path_buf(),
            keep_going: true,
            only: None,
            system: Some(selected.to_string()),
            evaluator: evaluator(),
            ci: false,
            today: gate::run::today(),
            nix: nixtops::probe::nix_bin(),
        };
        let result = gate::run(&settings, Some(tools.clone()), &Seams::default());
        let report = serde_json::to_value(&result.report).unwrap();
        report["layers"]
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["id"] == "eval")
            .unwrap()
            .clone()
    };
    let other = verdict_for("aarch64-linux", "x86_64-linux");
    assert_eq!(other["verdict"], "pass", "{other}");
    let same = verdict_for("x86_64-linux", "x86_64-linux");
    assert_eq!(same["verdict"], "Refuse", "{same}");
    assert!(
        same["reason"]
            .as_str()
            .unwrap()
            .contains("packages.x86_64-linux.default failed"),
        "{same}"
    );
}

/// A member that is not a derivation (`<output>.<system>.bad = 42`) fails on the selected system
/// and passes when the other system is selected, with the real tools.
#[test]
fn a_non_derivation_member_fails_only_on_the_selected_system_with_the_real_tools() {
    let Some(tools) = tools() else {
        eprintln!("skipped: NIXTOPS_FX_* not set");
        return;
    };
    let clean = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/gates/clean");
    let verdict_for = |output: &str, broken: &str, selected: &str| {
        let repo = scratch_repo(&clean);
        let flake = repo.path().join("flake.nix");
        let text = fs::read_to_string(&flake).unwrap();
        let at = text.find("outputs = _: {\n").unwrap() + "outputs = _: {\n".len();
        let mut edited = text.clone();
        edited.insert_str(at, &format!("    {output}.{broken}.bad = 42;\n"));
        fs::write(&flake, edited).unwrap();
        git(repo.path(), &["add", "-A"]);
        git(repo.path(), &["commit", "-q", "-m", "bad"]);
        let settings = Settings {
            root: repo.path().to_path_buf(),
            keep_going: true,
            only: None,
            system: Some(selected.to_string()),
            evaluator: evaluator(),
            ci: false,
            today: gate::run::today(),
            nix: nixtops::probe::nix_bin(),
        };
        let result = gate::run(&settings, Some(tools.clone()), &Seams::default());
        let report = serde_json::to_value(&result.report).unwrap();
        report["layers"]
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["id"] == "eval")
            .unwrap()
            .clone()
    };
    for output in ["packages", "checks"] {
        let mine = verdict_for(output, "x86_64-linux", "x86_64-linux");
        assert_eq!(mine["verdict"], "Refuse", "{output}: {mine}");
        assert!(
            mine["reason"]
                .as_str()
                .unwrap()
                .contains(&format!("{output}.x86_64-linux.bad")),
            "{mine}"
        );
        let other = verdict_for(output, "aarch64-linux", "x86_64-linux");
        assert_eq!(other["verdict"], "pass", "{output}: {other}");
    }
}

/// QA's q4: the other system's broken output quotes the selected system's name in its source
/// excerpt. That must not make the error the selected system's, with the real tools.
#[test]
fn a_system_quoted_in_the_excerpt_of_another_systems_error_does_not_refuse_the_run() {
    let Some(tools) = tools() else {
        eprintln!("skipped: NIXTOPS_FX_* not set");
        return;
    };
    let clean = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/gates/clean");
    let repo = scratch_repo(&clean);
    let flake = repo.path().join("flake.nix");
    let text = fs::read_to_string(&flake).unwrap();
    let needle = "packages.aarch64-linux.default = ";
    let start = text.find(needle).unwrap();
    let end = text[start..].find(";\n").unwrap() + start;
    let mut edited = text.clone();
    edited.replace_range(
        start + needle.len()..end,
        "throw \"qa broken\" /* 'x86_64-linux' */",
    );
    fs::write(&flake, edited).unwrap();
    git(repo.path(), &["add", "-A"]);
    git(repo.path(), &["commit", "-q", "-m", "throw"]);
    let settings = Settings {
        root: repo.path().to_path_buf(),
        keep_going: true,
        only: None,
        system: Some("x86_64-linux".to_string()),
        evaluator: evaluator(),
        ci: false,
        today: gate::run::today(),
        nix: nixtops::probe::nix_bin(),
    };
    let result = gate::run(&settings, Some(tools), &Seams::default());
    let report = serde_json::to_value(&result.report).unwrap();
    let eval = report["layers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["id"] == "eval")
        .unwrap();
    assert_eq!(eval["verdict"], "pass", "{eval}");
}

/// The Nix system of the machine the test runs on, which is where `nix` looks first for a bare
/// attribute (`packages.<host>.<name>`).
fn host_system() -> &'static str {
    match (std::env::consts::ARCH, std::env::consts::OS) {
        ("aarch64", "macos") => "aarch64-darwin",
        ("x86_64", "macos") => "x86_64-darwin",
        ("aarch64", _) => "aarch64-linux",
        _ => "x86_64-linux",
    }
}

fn inventory_of(flake: &str) -> inventory::Inventory {
    let tools = tools().unwrap();
    let repo = tempfile::tempdir().unwrap();
    fs::write(repo.path().join("flake.nix"), flake).unwrap();
    git(repo.path(), &["init", "-q", "."]);
    git(repo.path(), &["config", "user.email", "a@b"]);
    git(repo.path(), &["config", "user.name", "n"]);
    git(repo.path(), &["add", "-A"]);
    git(repo.path(), &["commit", "-q", "-m", "f"]);
    let mut session = Session::new(tools).unwrap();
    if evaluator() == Evaluator::Host {
        session.set_backend(Backend::PerAttribute);
    }
    let root = repo.path().canonicalize().unwrap().display().to_string();
    inventory::collect(&session, &root).unwrap()
}

fn drv(system: &str) -> String {
    format!("builtins.derivation {{ name = \"x\"; system = \"{system}\"; builder = \"/bin/sh\"; }}")
}

/// Every attribute the inventory asks for is anchored at the outputs (`#."devShells"`). A
/// throwing `packages.<host>` is where `nix` looks first for a bare name, so unanchored it hid
/// every output; and a member named like an output shadowed it.
#[test]
fn a_throwing_packages_set_for_the_host_does_not_hide_the_other_outputs_with_the_real_tools() {
    if tools().is_none() {
        eprintln!("skipped: NIXTOPS_FX_* not set");
        return;
    }
    let host = host_system();
    let flake = format!(
        "{{ outputs = _: {{\n packages.{host} = throw \"packages throws\";\n devShells.{host}.default = {d};\n checks.{host}.real = {d};\n }}; }}",
        d = drv(host)
    );
    let inv = inventory_of(&flake);
    let ok = |attr: &str| {
        inv.records
            .iter()
            .any(|r| r.attr == attr && r.error.is_none())
    };
    assert!(
        ok(&format!("devShells.{host}.default")),
        "{:?}",
        inv.records
    );
    assert!(ok(&format!("checks.{host}.real")), "{:?}", inv.records);
    let failed = inv.records.iter().find(|r| r.error.is_some()).unwrap();
    assert!(failed.error.as_deref().unwrap().contains("packages throws"));
}

#[test]
fn a_member_named_like_an_output_is_not_read_as_that_output_with_the_real_tools() {
    if tools().is_none() {
        eprintln!("skipped: NIXTOPS_FX_* not set");
        return;
    }
    let host = host_system();
    let flake = format!(
        "{{ outputs = _: {{\n packages.{host}.checks = {d};\n packages.{host}.devShells = {d};\n checks.{host}.real = {d};\n devShells.{host}.mine = {d};\n }}; }}",
        d = drv(host)
    );
    let inv = inventory_of(&flake);
    let attrs: Vec<&str> = inv.records.iter().map(|r| r.attr.as_str()).collect();
    for want in [
        format!("checks.{host}.real"),
        format!("devShells.{host}.mine"),
        format!("packages.{host}.checks"),
        format!("packages.{host}.devShells"),
    ] {
        assert!(attrs.contains(&want.as_str()), "{want} in {attrs:?}");
    }
    assert!(
        inv.records.iter().all(|r| r.error.is_none()),
        "{:?}",
        inv.records
    );
}
