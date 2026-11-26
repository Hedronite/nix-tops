//! Real-Nix tests for gate 4 (`gate::build`) and structured `--system` scoping (`gate::eval` +
//! `gate::system`, including M1 q4 and q9 plants on both pinned CppNix and host Determinate Nix).
//! Skipped unless `NIXTOPS_FX_JUDGE`, `NIXTOPS_FX_EVAL_JOBS`, and `NIXTOPS_FX_NIXPKGS` are set.

use nixtops::{
    gate::{build, diag, eval},
    inventory::{Backend, Session, Tools},
    probe,
    report::Verdict,
};
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

fn host_system() -> &'static str {
    match (std::env::consts::ARCH, std::env::consts::OS) {
        ("aarch64", "macos") => "aarch64-darwin",
        ("x86_64", "macos") => "x86_64-darwin",
        ("aarch64", _) => "aarch64-linux",
        _ => "x86_64-linux",
    }
}

fn other_system() -> &'static str {
    if host_system() == "x86_64-linux" {
        "aarch64-darwin"
    } else {
        "x86_64-linux"
    }
}

fn run_eval_and_build(
    tools: Tools,
    repo: &Path,
    use_host: bool,
    selected: Option<&str>,
    ci: bool,
) -> (
    eval::Outcome,
    Option<build::Outcome>,
    Vec<nixtops::report::OutputEntry>,
) {
    let nix = probe::nix_bin();
    let session_tools = if use_host {
        Tools {
            judge: PathBuf::from(&nix),
            ..tools
        }
    } else {
        tools
    };
    let mut session = Session::new(session_tools).unwrap();
    let is_det = use_host && probe::version_line(&nix).is_some_and(|v| v.contains("Determinate"));
    if use_host {
        session.set_backend(Backend::PerAttribute);
        session.set_determinate(is_det);
    }
    let table = diag::embedded().unwrap_or_default();
    let flake_ref = repo.canonicalize().unwrap().display().to_string();
    let impl_name = if is_det { "determinate" } else { "cppnix" };
    let eval_in = eval::Input {
        flake_ref: &flake_ref,
        today: "2026-11-25",
        ledger: &[],
        diagnostics: &table,
        implementation: impl_name,
        system: selected,
    };
    let eval_out = eval::run(&mut session, &eval_in);
    let mut outputs = eval_out.outputs.clone();
    if eval_out.layer.verdict != Verdict::Pass {
        return (eval_out, None, outputs);
    }
    let build_in = build::Input {
        flake_ref: &flake_ref,
        nix_bin: &nix,
        host_system: host_system(),
        selected_system: selected,
        enforce: "hard",
        ci,
    };
    let build_out = build::run(&session, &build_in, &mut outputs);
    (eval_out, Some(build_out), outputs)
}

#[test]
fn real_failing_check_failing_package_and_ifd_fixtures_refuse() {
    let Some(tools) = tools() else {
        eprintln!("skipped: NIXTOPS_FX_* not set");
        return;
    };
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/gates");

    for (name, needle) in [
        ("build-failing-check", "flake check build failed"),
        ("build-failing-package", "build failed"),
    ] {
        let repo = scratch_repo(&fixtures.join(name));
        let (eval_out, build_out, _) =
            run_eval_and_build(tools.clone(), repo.path(), false, None, false);
        assert_eq!(
            eval_out.layer.verdict,
            Verdict::Pass,
            "{name}: {:?}",
            eval_out.layer
        );
        let b = build_out.unwrap();
        assert_eq!(b.layer.verdict, Verdict::Refuse, "{name}: {:?}", b.layer);
        assert!(
            b.layer.reason.as_deref().unwrap_or("").contains(needle),
            "{:?}",
            b.layer
        );
        assert!(b.sandbox.is_some());
    }

    let ifd_repo = scratch_repo(&fixtures.join("build-ifd"));
    let (eval_out, build_out, _) = run_eval_and_build(tools, ifd_repo.path(), false, None, false);
    assert_eq!(eval_out.layer.verdict, Verdict::Refuse);
    assert!(
        eval_out
            .layer
            .reason
            .as_deref()
            .unwrap_or("")
            .contains("N-07")
    );
    assert!(build_out.is_none());
}

#[test]
fn real_non_host_only_flake_is_deferred_locally_and_refused_in_ci_hard() {
    let Some(tools) = tools() else {
        eprintln!("skipped: NIXTOPS_FX_* not set");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let other = other_system();
    let lock = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/gates/lint-clean/flake.lock");
    let flake_src = r#"{
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/04906337b6d53e8bb0eaf0470ae9f070a4d4eb55";
  outputs = { nixpkgs, ... }: {
    packages."__OTHER__".default = nixpkgs.legacyPackages."__OTHER__".runCommand "other" { } "mkdir -p $out";
  };
}
"#
    .replace("__OTHER__", other);
    fs::write(dir.path().join("flake.nix"), flake_src).unwrap();
    fs::copy(lock, dir.path().join("flake.lock")).unwrap();
    git(dir.path(), &["init", "-q", "."]);
    git(dir.path(), &["config", "user.email", "a@b"]);
    git(dir.path(), &["config", "user.name", "n"]);
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-q", "-m", "other-only"]);

    let (e1, b1, out1) = run_eval_and_build(tools.clone(), dir.path(), false, None, false);
    assert_eq!(e1.layer.verdict, Verdict::Pass, "{:?}", e1.layer);
    let b1 = b1.unwrap();
    assert_eq!(b1.layer.verdict, Verdict::Deferred, "{:?}", b1.layer);
    assert_eq!(out1[0].deferred, Some(true));

    let (_e2, b2, out2) = run_eval_and_build(tools, dir.path(), false, None, true);
    let b2 = b2.unwrap();
    assert_eq!(b2.layer.verdict, Verdict::Refuse, "{:?}", b2.layer);
    assert_eq!(out2[0].deferred, Some(true));
}

#[test]
fn real_host_clean_flake_builds_outputs_and_records_sandbox() {
    let Some(tools) = tools() else {
        eprintln!("skipped: NIXTOPS_FX_* not set");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let host = host_system();
    let other = other_system();
    let lock = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/gates/lint-clean/flake.lock");
    let flake_src = r#"{
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/04906337b6d53e8bb0eaf0470ae9f070a4d4eb55";
  outputs = { nixpkgs, ... }:
    let
      pkgs = nixpkgs.legacyPackages."__HOST__";
      otherPkgs = nixpkgs.legacyPackages."__OTHER__";
    in {
      packages."__HOST__".default = pkgs.runCommand "host-pkg" { } "mkdir -p $out";
      devShells."__HOST__".default = pkgs.runCommand "host-shell" { } "mkdir -p $out";
      checks."__HOST__".unit = pkgs.runCommand "host-check" { } "mkdir -p $out";
      packages."__OTHER__".default = otherPkgs.runCommand "other-pkg" { } "mkdir -p $out";
    };
}
"#
    .replace("__HOST__", host)
    .replace("__OTHER__", other);
    fs::write(dir.path().join("flake.nix"), flake_src).unwrap();
    fs::copy(lock, dir.path().join("flake.lock")).unwrap();
    git(dir.path(), &["init", "-q", "."]);
    git(dir.path(), &["config", "user.email", "a@b"]);
    git(dir.path(), &["config", "user.name", "n"]);
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-q", "-m", "clean-multi"]);

    let (ev, b, outputs) = run_eval_and_build(tools, dir.path(), false, None, false);
    assert_eq!(ev.layer.verdict, Verdict::Pass, "{:?}", ev.layer);
    let b = b.unwrap();
    assert_eq!(b.layer.verdict, Verdict::Pass, "{:?}", b.layer);
    assert!(matches!(
        b.sandbox.as_deref(),
        Some("true" | "false" | "relaxed")
    ));
    for entry in &outputs {
        if entry.attr.contains(host) {
            assert_eq!(entry.level, "built", "{entry:?}");
            assert_eq!(entry.deferred, None);
        } else {
            assert_eq!(entry.deferred, Some(true), "{entry:?}");
        }
    }
}

#[test]
fn g1c_system_scoping_closes_q4_and_q9_on_pinned_and_host_evaluators() {
    let Some(tools) = tools() else {
        eprintln!("skipped: NIXTOPS_FX_* not set");
        return;
    };
    let clean = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/gates/clean");

    for use_host in [false, true] {
        let repo_q4 = scratch_repo(&clean);
        let f_q4 = repo_q4.path().join("flake.nix");
        let t_q4 = fs::read_to_string(&f_q4).unwrap();
        let needle = "packages.aarch64-linux.default = ";
        let start = t_q4.find(needle).unwrap();
        let end = t_q4[start..].find(";\n").unwrap() + start;
        let mut edited = t_q4.clone();
        edited.replace_range(
            start + needle.len()..end,
            r#"throw "qa broken" /* 'x86_64-linux' */"#,
        );
        fs::write(&f_q4, edited).unwrap();
        git(repo_q4.path(), &["add", "-A"]);
        git(repo_q4.path(), &["commit", "-q", "-m", "q4"]);
        let (ev, _, _) = run_eval_and_build(
            tools.clone(),
            repo_q4.path(),
            use_host,
            Some("x86_64-linux"),
            false,
        );
        assert_eq!(
            ev.layer.verdict,
            Verdict::Pass,
            "q4 host={use_host}: {:?}",
            ev.layer
        );

        let repo_q9 = scratch_repo(&clean);
        let f_q9 = repo_q9.path().join("flake.nix");
        let mut edited_q9 = t_q4.clone();
        edited_q9.replace_range(
            start + needle.len()..end,
            r#"throw "unsupported platform 'x86_64-linux'""#,
        );
        fs::write(&f_q9, edited_q9).unwrap();
        git(repo_q9.path(), &["add", "-A"]);
        git(repo_q9.path(), &["commit", "-q", "-m", "q9"]);

        let (ev_x86, _, _) = run_eval_and_build(
            tools.clone(),
            repo_q9.path(),
            use_host,
            Some("x86_64-linux"),
            false,
        );
        assert_eq!(
            ev_x86.layer.verdict,
            Verdict::Pass,
            "q9 selected x86 host={use_host}: {:?}",
            ev_x86.layer
        );
        let (ev_a64, _, _) = run_eval_and_build(
            tools.clone(),
            repo_q9.path(),
            use_host,
            Some("aarch64-linux"),
            false,
        );
        assert_eq!(
            ev_a64.layer.verdict,
            Verdict::Refuse,
            "q9 selected aarch64 host={use_host}: {:?}",
            ev_a64.layer
        );
    }
}

#[test]
fn g1c_warnings_scope_by_structured_records_w1_w2_w4_on_both_evaluators() {
    let Some(tools) = tools() else {
        eprintln!("skipped: NIXTOPS_FX_* not set");
        return;
    };
    let clean = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/gates/clean");
    for use_host in [false, true] {
        // w1: selected x86_64-linux warns naming aarch64-linux -> MUST REFUSE.
        let repo_w1 = scratch_repo(&clean);
        let f1 = repo_w1.path().join("flake.nix");
        let base = fs::read_to_string(&f1).unwrap();
        let needle_x86 = "packages.x86_64-linux.default = ";
        let s1 = base.find(needle_x86).unwrap();
        let mut src_w1 = base.clone();
        src_w1.insert_str(
            s1 + needle_x86.len(),
            r#"builtins.warn "use the aarch64-linux build" "#,
        );
        fs::write(&f1, src_w1).unwrap();
        git(repo_w1.path(), &["add", "-A"]);
        git(repo_w1.path(), &["commit", "-q", "-m", "w1"]);
        let (ev_w1, _, _) = run_eval_and_build(
            tools.clone(),
            repo_w1.path(),
            use_host,
            Some("x86_64-linux"),
            false,
        );
        assert_eq!(
            ev_w1.layer.verdict,
            Verdict::Refuse,
            "w1 host={use_host}: {:?}",
            ev_w1.layer
        );

        // w2 (other warns naming x86_64-linux) and w4 (other warns naming no system) -> MUST PASS for x86_64-linux, REFUSE for aarch64-linux.
        for (tag, msg) in [
            ("w2", r#"builtins.warn "not for x86_64-linux" "#),
            ("w4", r#"builtins.warn "plain other-system warning" "#),
        ] {
            let repo = scratch_repo(&clean);
            let f = repo.path().join("flake.nix");
            let needle_a64 = "packages.aarch64-linux.default = ";
            let idx = base.find(needle_a64).unwrap();
            let mut src = base.clone();
            src.insert_str(idx + needle_a64.len(), msg);
            fs::write(&f, src).unwrap();
            git(repo.path(), &["add", "-A"]);
            git(repo.path(), &["commit", "-q", "-m", tag]);
            let (ev_x86, _, _) = run_eval_and_build(
                tools.clone(),
                repo.path(),
                use_host,
                Some("x86_64-linux"),
                false,
            );
            assert_eq!(
                ev_x86.layer.verdict,
                Verdict::Pass,
                "{tag} x86 host={use_host}: {:?}",
                ev_x86.layer
            );
            let (ev_a64, _, _) = run_eval_and_build(
                tools.clone(),
                repo.path(),
                use_host,
                Some("aarch64-linux"),
                false,
            );
            assert_eq!(
                ev_a64.layer.verdict,
                Verdict::Refuse,
                "{tag} a64 host={use_host}: {:?}",
                ev_a64.layer
            );
        }
    }
}
