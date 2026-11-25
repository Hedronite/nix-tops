//! Real-Nix tests for gate 2 (`lib.mkChecks` and `nixtops gate` lint layer). Skipped unless
//! `NIXTOPS_FX_JUDGE`, `NIXTOPS_FX_EVAL_JOBS`, and `NIXTOPS_FX_NIXPKGS` are set.

use nixtops::{
    exit::Exit,
    gate::{self, Evaluator, Seams, Settings},
    inventory::Tools,
    report::{LayerId, Verdict},
};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
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

fn settings_for(root: &Path) -> Settings {
    Settings {
        root: root.to_path_buf(),
        keep_going: false,
        only: None,
        system: None,
        evaluator: Evaluator::Pinned,
        ci: false,
        today: gate::run::today(),
        nix: nixtops::probe::nix_bin(),
        writing_baseline: false,
    }
}

fn run_nix(judge: &Path, cache: &Path, args: &[&str]) -> Output {
    Command::new(judge)
        .env("XDG_CACHE_HOME", cache)
        .arg("--extra-experimental-features")
        .arg("nix-command flakes")
        .args(args)
        .output()
        .unwrap()
}

fn prefetch_dir(judge: &Path, cache: &Path, dir: &Path) -> (String, String) {
    let out = run_nix(
        judge,
        cache,
        &["flake", "prefetch", "--json", &dir.display().to_string()],
    );
    assert!(
        out.status.success(),
        "prefetch {}: {}",
        dir.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: Value = serde_json::from_slice(&out.stdout).unwrap();
    (
        parsed["storePath"].as_str().unwrap().to_string(),
        parsed["hash"].as_str().unwrap().to_string(),
    )
}

fn stage_kit_lib(root_repo: &Path) -> tempfile::TempDir {
    let stage = tempfile::tempdir().unwrap();
    let nix_dir = stage.path().join("nix/lib");
    fs::create_dir_all(&nix_dir).unwrap();
    fs::write(stage.path().join("flake.nix"), "{ outputs = _: {}; }\n").unwrap();
    fs::copy(
        root_repo.join("nix/tools.nix"),
        stage.path().join("nix/tools.nix"),
    )
    .unwrap();
    for file in ["default.nix", "mkChecks.nix", "parse_findings.py"] {
        fs::copy(root_repo.join("nix/lib").join(file), nix_dir.join(file)).unwrap();
    }
    stage
}

fn nix_quote(s: &str) -> String {
    format!(
        "\"{}\"",
        s.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('$', "\\$")
    )
}

fn host_system(judge: &Path, cache: &Path) -> String {
    let out = run_nix(judge, cache, &["config", "show", "system"]);
    assert!(out.status.success());
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn mkchecks_expr(
    nixpkgs: &str,
    kit_path: &str,
    kit_hash: &str,
    target_path: &str,
    target_hash: &str,
    host: &str,
) -> String {
    format!(
        "let np = builtins.getFlake {np};          kit = builtins.fetchTree {{ type = \"path\"; path = {kp}; narHash = {kh}; }};          self = builtins.fetchTree {{ type = \"path\"; path = {tp}; narHash = {th}; }};          mkChecks = import (kit + \"/nix/lib/mkChecks.nix\") {{ nixpkgs = np; }};          in (mkChecks {{ inherit self; systems = [ {h} ]; }}).{h}",
        np = nix_quote(nixpkgs),
        kp = nix_quote(kit_path),
        kh = nix_quote(kit_hash),
        tp = nix_quote(target_path),
        th = nix_quote(target_hash),
        h = nix_quote(host),
    )
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn deadnix_and_statix_findings_fail_hard_and_clean_passes_with_real_tools() {
    let Some(tools) = tools() else {
        eprintln!("skipped: NIXTOPS_FX_* not set");
        return;
    };
    let root = repo_root().join("fixtures/gates");

    for (name, want_exit, want_tool) in [
        ("lint-clean", Exit::Pass, None),
        ("lint-deadnix", Exit::GateFailed, Some("deadnix")),
        ("lint-statix", Exit::GateFailed, Some("statix")),
    ] {
        let repo = scratch_repo(&root.join(name));
        let res = gate::run(
            &settings_for(repo.path()),
            Some(tools.clone()),
            &Seams::default(),
        );
        assert_eq!(res.exit, want_exit, "{name}: {:?}", res.report);
        let report: Value = serde_json::to_value(&res.report).unwrap();
        let entries = report["lint"]["tools"].as_array().unwrap();
        assert_eq!(entries.len(), 4);
        let names: Vec<&str> = entries
            .iter()
            .map(|t| t["tool"].as_str().unwrap())
            .collect();
        assert_eq!(names, vec!["deadnix", "gitleaks", "nixfmt", "statix"]);
        if let Some(tool) = want_tool {
            let entry = entries.iter().find(|t| t["tool"] == tool).unwrap();
            assert!(
                entry["files"]["flake.nix"].as_u64().unwrap() > 0,
                "{name}: {entry}"
            );
        }
    }
}

#[test]
fn verdict_fails_on_finding_while_findings_succeeds_and_stays_in_store() {
    let Some(tools) = tools() else {
        eprintln!("skipped: NIXTOPS_FX_* not set");
        return;
    };
    let root = repo_root();
    let cache = tempfile::tempdir().unwrap();
    let stage = stage_kit_lib(&root);
    let repo = scratch_repo(&root.join("fixtures/gates/lint-deadnix"));
    let host = host_system(&tools.judge, cache.path());
    let (kp, kh) = prefetch_dir(&tools.judge, cache.path(), stage.path());
    let (tp, th) = prefetch_dir(&tools.judge, cache.path(), repo.path());
    let base = mkchecks_expr(&tools.nixpkgs, &kp, &kh, &tp, &th, &host);

    let findings_expr = format!("({base}).nixtops-deadnix-findings");
    let findings_build = run_nix(
        &tools.judge,
        cache.path(),
        &[
            "build",
            "--no-link",
            "--json",
            "--no-update-lock-file",
            "--option",
            "allow-import-from-derivation",
            "false",
            "--expr",
            &findings_expr,
        ],
    );
    assert!(
        findings_build.status.success(),
        "findings build failed: {}",
        String::from_utf8_lossy(&findings_build.stderr)
    );
    let built: Value = serde_json::from_slice(&findings_build.stdout).unwrap();
    let out_path = PathBuf::from(built[0]["outputs"]["out"].as_str().unwrap());
    let json_text = fs::read_to_string(&out_path).unwrap();
    let parsed: Value = serde_json::from_str(&json_text).unwrap();
    assert_eq!(parsed["tool"], "deadnix");
    assert!(!parsed["files"]["flake.nix"].as_array().unwrap().is_empty());

    let verdict_expr = format!("({base}).nixtops-deadnix");
    let verdict_build = run_nix(
        &tools.judge,
        cache.path(),
        &[
            "build",
            "--no-link",
            "--no-update-lock-file",
            "--option",
            "allow-import-from-derivation",
            "false",
            "--expr",
            &verdict_expr,
        ],
    );
    assert!(
        !verdict_build.status.success(),
        "expected nixtops-deadnix verdict to fail"
    );
    let verdict_err = String::from_utf8_lossy(&verdict_build.stderr);
    assert!(verdict_err.contains("deadnix:"), "{verdict_err}");

    assert!(out_path.is_file(), "findings output vanished from store");
}

#[test]
fn readme_edit_keeps_nix_linter_drv_paths_identical() {
    let Some(tools) = tools() else {
        eprintln!("skipped: NIXTOPS_FX_* not set");
        return;
    };
    let root = repo_root();
    let cache = tempfile::tempdir().unwrap();
    let stage = stage_kit_lib(&root);
    let repo = scratch_repo(&root.join("fixtures/gates/lint-clean"));
    let host = host_system(&tools.judge, cache.path());
    let (kp, kh) = prefetch_dir(&tools.judge, cache.path(), stage.path());

    let eval_drvs = |dir: &Path| -> BTreeMap<String, String> {
        let (tp, th) = prefetch_dir(&tools.judge, cache.path(), dir);
        let base = mkchecks_expr(&tools.nixpkgs, &kp, &kh, &tp, &th, &host);
        let out = run_nix(
            &tools.judge,
            cache.path(),
            &[
                "eval",
                "--json",
                "--no-update-lock-file",
                "--option",
                "allow-import-from-derivation",
                "false",
                "--expr",
                &base,
                "--apply",
                r#"c: builtins.listToAttrs (map (n: { name = n; value = c.${n}.drvPath; }) ["nixtops-nixfmt" "nixtops-nixfmt-findings" "nixtops-statix" "nixtops-statix-findings" "nixtops-deadnix" "nixtops-deadnix-findings"])"#,
            ],
        );
        assert!(
            out.status.success(),
            "eval failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    };

    let before = eval_drvs(repo.path());
    assert_eq!(before.len(), 6);

    fs::write(repo.path().join("README.md"), "# First\n").unwrap();
    git(repo.path(), &["add", "README.md"]);
    git(repo.path(), &["commit", "-q", "-m", "add readme"]);
    let after_add = eval_drvs(repo.path());
    assert_eq!(before, after_add);

    fs::write(repo.path().join("README.md"), "# Second\n").unwrap();
    git(repo.path(), &["add", "README.md"]);
    git(repo.path(), &["commit", "-q", "-m", "edit readme"]);
    let after_edit = eval_drvs(repo.path());
    assert_eq!(before, after_edit);
}

#[test]
fn malformed_tool_configs_and_missing_formatters_refuse_under_hard() {
    let Some(tools) = tools() else {
        eprintln!("skipped: NIXTOPS_FX_* not set");
        return;
    };
    let gates_root = repo_root().join("fixtures/gates");

    // 1. Statix finding + malformed statix.toml.
    let statix_repo = scratch_repo(&gates_root.join("lint-statix"));
    fs::write(statix_repo.path().join("statix.toml"), "disabled = [[\n").unwrap();
    git(statix_repo.path(), &["add", "-A"]);
    git(
        statix_repo.path(),
        &["commit", "-q", "-m", "bad statix.toml"],
    );
    let res = gate::run(
        &settings_for(statix_repo.path()),
        Some(tools.clone()),
        &Seams::default(),
    );
    assert_eq!(res.exit, Exit::GateFailed, "malformed statix.toml passed!");
    assert_eq!(res.report.layers[1].id, LayerId::Lint);
    assert_eq!(res.report.layers[1].verdict, Verdict::Refuse);

    // 2. Gitleaks secret finding + malformed .gitleaks.toml.
    let gitleaks_repo = scratch_repo(&gates_root.join("lint-clean"));
    let secret = format!("{}_{}", "ghp", "0123456789abcdefghijklmnopqrstuvwxyz");
    fs::write(
        gitleaks_repo.path().join("README.md"),
        format!("{secret}\n"),
    )
    .unwrap();
    fs::write(gitleaks_repo.path().join(".gitleaks.toml"), "bad = [[\n").unwrap();
    git(gitleaks_repo.path(), &["add", "-A"]);
    git(
        gitleaks_repo.path(),
        &["commit", "-q", "-m", "bad .gitleaks.toml"],
    );
    let res = gate::run(
        &settings_for(gitleaks_repo.path()),
        Some(tools.clone()),
        &Seams::default(),
    );
    assert_eq!(
        res.exit,
        Exit::GateFailed,
        "malformed .gitleaks.toml passed!"
    );
    assert_eq!(res.report.layers[1].verdict, Verdict::Refuse);

    // 3. Unformatted nix file + treefmt.toml naming missing formatter.
    let fmt_repo = scratch_repo(&gates_root.join("lint-clean"));
    fs::write(fmt_repo.path().join("unfmt.nix"), "{x=1;}\n").unwrap();
    fs::write(
        fmt_repo.path().join("treefmt.toml"),
        "[formatter.nix]\ncommand = \"no-such-fmt-bin\"\nincludes = [\"*.nix\"]\n",
    )
    .unwrap();
    git(fmt_repo.path(), &["add", "-A"]);
    git(
        fmt_repo.path(),
        &["commit", "-q", "-m", "missing formatter"],
    );
    let res = gate::run(
        &settings_for(fmt_repo.path()),
        Some(tools.clone()),
        &Seams::default(),
    );
    assert_eq!(res.exit, Exit::GateFailed, "missing formatter passed!");
    assert_eq!(res.report.layers[1].verdict, Verdict::Refuse);

    // 4. Unformatted nix file + malformed TOML treefmt.toml.
    fs::write(fmt_repo.path().join("treefmt.toml"), "bad = [[\n").unwrap();
    git(fmt_repo.path(), &["add", "-A"]);
    git(
        fmt_repo.path(),
        &["commit", "-q", "-m", "malformed treefmt.toml"],
    );
    let res = gate::run(
        &settings_for(fmt_repo.path()),
        Some(tools),
        &Seams::default(),
    );
    assert_eq!(res.exit, Exit::GateFailed, "malformed treefmt.toml passed!");
    assert_eq!(res.report.layers[1].verdict, Verdict::Refuse);
}

#[test]
fn deadnix_fail_false_and_dot_directory_paths_work_with_real_tools() {
    let Some(tools) = tools() else {
        eprintln!("skipped: NIXTOPS_FX_* not set");
        return;
    };
    let gates_root = repo_root().join("fixtures/gates");
    let repo = scratch_repo(&gates_root.join("lint-deadnix"));
    let cfg = fs::read_to_string(repo.path().join("nix-tops.toml")).unwrap();
    fs::write(
        repo.path().join("nix-tops.toml"),
        format!("{cfg}\n[lint.deadnix]\nfail = false\n"),
    )
    .unwrap();
    git(repo.path(), &["add", "-A"]);
    git(repo.path(), &["commit", "-q", "-m", "deadnix fail false"]);

    let res = gate::run(
        &settings_for(repo.path()),
        Some(tools.clone()),
        &Seams::default(),
    );
    assert_eq!(
        res.exit,
        Exit::Pass,
        "deadnix.fail=false should pass: {:?}",
        res.report
    );
    let report: Value = serde_json::to_value(&res.report).unwrap();
    let deadnix = report["lint"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["tool"] == "deadnix")
        .unwrap();
    assert!(deadnix["files"]["flake.nix"].as_u64().unwrap() > 0);

    // Dot-directory file paths keep their leading dot in parse_findings.py (not lstrip).
    fs::create_dir_all(repo.path().join(".config")).unwrap();
    fs::write(repo.path().join(".config/x.nix"), "{a=1;}\n").unwrap();
    git(repo.path(), &["add", "-A"]);
    git(repo.path(), &["commit", "-q", "-m", "dot dir nixfmt"]);
    let res2 = gate::run(&settings_for(repo.path()), Some(tools), &Seams::default());
    assert_eq!(res2.exit, Exit::GateFailed);
    let report2: Value = serde_json::to_value(&res2.report).unwrap();
    let nixfmt = report2["lint"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["tool"] == "nixfmt")
        .unwrap();
    assert_eq!(nixfmt["files"][".config/x.nix"].as_u64(), Some(1));
}
