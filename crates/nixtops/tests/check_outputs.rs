//! `nixtops check` with the outputs and git read: facets and systems against the outputs, the
//! baseline tracked and clean under `ratchet`, and the exit code of each failure. The world is
//! stood in by probes; one test runs the real git when the host has it.

mod common;
use common::fake_nix;
use nixtops::{
    cmd::check::{Probes, evaluate_with, evaluate_with_outputs},
    config::baseline::GitRun,
    exit::Exit,
    inventory::Structure,
};
use std::{fs, path::Path, process::Command};
use tempfile::tempdir;

const HARD: &str = "protocol = \"0.2.0\"\nfacets = [\"dev-flake\"]\nenforce = \"hard\"\n\n[toolchain]\nsystems = [\"x86_64-linux\", \"aarch64-linux\"]\n";

fn flake() -> Structure {
    let mut s = Structure::default();
    s.insert(
        "devShells",
        vec!["aarch64-linux".into(), "x86_64-linux".into()],
    );
    s
}

fn setup(toml: &str) -> (tempfile::TempDir, String) {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("nix-tops.toml"), toml).expect("write");
    let nix = fake_nix(dir.path(), "nix (Nix) 2.35.2");
    (dir, nix.to_str().expect("utf8").to_string())
}

fn git(
    inside: Option<bool>,
    tracked: bool,
    status: &'static str,
) -> impl Fn(&[&str]) -> Option<GitRun> {
    move |args: &[&str]| {
        let run = |ok: bool, out: &str| {
            Some(GitRun {
                ok,
                stdout: out.into(),
            })
        };
        match args[0] {
            "rev-parse" => inside.and_then(|ok| run(ok, "true\n")),
            "ls-files" => run(tracked, ""),
            _ => run(true, status),
        }
    }
}

fn verdict(
    toml: &str,
    structure: Result<Structure, String>,
    g: impl Fn(&[&str]) -> Option<GitRun>,
) -> nixtops::cmd::check::Verdict {
    let (dir, nix) = setup(toml);
    let outputs = move || structure.clone();
    let probes = Probes {
        outputs: &outputs,
        git: &g,
    };
    evaluate_with(
        &dir.path().join("nix-tops.toml"),
        "2026-11-18",
        &nix,
        Some(&probes),
    )
}

fn keys(v: &nixtops::cmd::check::Verdict) -> Vec<&str> {
    v.problems.iter().map(|p| p.key.as_str()).collect()
}

#[test]
fn matching_facets_and_systems_pass_under_hard_without_touching_git() {
    let panics = |_: &[&str]| -> Option<GitRun> { panic!("git is read only under ratchet") };
    let v = verdict(HARD, Ok(flake()), panics);
    assert_eq!((v.exit, v.problems.len()), (Exit::Pass, 0), "{v:?}");
    assert!(
        v.notes
            .iter()
            .any(|n| n.contains("outputs show facets: dev-flake"))
    );
}

#[test]
fn a_facet_the_outputs_do_not_show_and_wrong_systems_exit_2_naming_the_key() {
    let toml = HARD
        .replace("[\"dev-flake\"]", "[\"package-set\"]")
        .replace("\"aarch64-linux\"", "\"aarch64-darwin\"");
    let v = verdict(&toml, Ok(flake()), git(Some(true), true, ""));
    assert_eq!(v.exit, Exit::ConfigInvalid);
    assert_eq!(keys(&v), ["facets[0]", "toolchain.systems"]);
}

#[test]
fn unreadable_outputs_and_an_unlisted_output_exit_3_naming_what_failed() {
    let g = git(Some(true), true, "");
    let v = verdict(
        HARD,
        Err("cannot read the flake's outputs: boom".into()),
        &g,
    );
    assert_eq!((v.exit, keys(&v)), (Exit::Environment, vec!["outputs"]));
    let mut s = flake();
    s.insert_error("packages", "bad".into());
    let v = verdict(HARD, Ok(s), &g);
    assert_eq!(
        (v.exit, keys(&v)),
        (Exit::Environment, vec!["outputs.packages"])
    );
}

#[test]
fn a_config_failure_wins_over_an_environment_failure() {
    let toml = HARD.replace("[\"dev-flake\"]", "[\"package-set\"]");
    let mut s = flake();
    s.insert_error("packages", "bad".into());
    let v = verdict(&toml, Ok(s), git(Some(true), true, ""));
    // The unlisted output stops the facet check, so nothing is a config failure here.
    assert_eq!(v.exit, Exit::Environment);
    let v = verdict(
        &HARD.replace("hard", "advisory"),
        Err("x".into()),
        git(Some(true), true, ""),
    );
    assert_eq!(v.exit, Exit::ConfigInvalid);
}

fn ratchet() -> String {
    HARD.replace("enforce = \"hard\"\n", "")
}

fn with_baseline(toml: &str) -> (tempfile::TempDir, String) {
    let (dir, nix) = setup(toml);
    fs::create_dir(dir.path().join(".nix-tops")).expect("mkdir");
    fs::write(dir.path().join(".nix-tops/baseline.json"), "{}").expect("write");
    (dir, nix)
}

fn ratchet_verdict(g: impl Fn(&[&str]) -> Option<GitRun>) -> nixtops::cmd::check::Verdict {
    let (dir, nix) = with_baseline(&ratchet());
    let outputs = || Ok(flake());
    let probes = Probes {
        outputs: &outputs,
        git: &g,
    };
    evaluate_with(
        &dir.path().join("nix-tops.toml"),
        "2026-11-18",
        &nix,
        Some(&probes),
    )
}

#[test]
fn under_ratchet_the_baseline_must_be_tracked_and_clean_against_head() {
    let ok = ratchet_verdict(git(Some(true), true, ""));
    assert_eq!((ok.exit, ok.problems.len()), (Exit::Pass, 0), "{ok:?}");
    for (g, why) in [
        (git(Some(false), true, ""), "not in a git repository"),
        (git(Some(true), false, ""), "not tracked"),
        (
            git(Some(true), true, " M .nix-tops/baseline.json\n"),
            "differs from HEAD",
        ),
    ] {
        let v = ratchet_verdict(g);
        assert_eq!((v.exit, keys(&v)), (Exit::ConfigInvalid, vec!["baseline"]));
        assert!(
            v.problems[0].message.contains(why),
            "{why}: {:?}",
            v.problems
        );
    }
    let v = ratchet_verdict(git(None, true, ""));
    assert_eq!((v.exit, keys(&v)), (Exit::Environment, vec!["baseline"]));
}

fn have_git() -> bool {
    Command::new("git").arg("--version").output().is_ok()
}

fn run_git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.invalid",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .status()
        .expect("git runs");
    assert!(status.success(), "git {args:?}");
}

#[test]
fn the_real_git_sees_a_committed_baseline_as_clean_and_an_edited_one_as_dirty() {
    if !have_git() {
        eprintln!("skipped: no git on this host");
        return;
    }
    let (dir, nix) = with_baseline(&ratchet());
    let config = dir.path().join("nix-tops.toml");
    let run = |d: &Path| {
        evaluate_with_outputs(&config, "2026-11-18", &nix, &flake()).exit == Exit::Pass
            && d.exists()
    };
    assert!(!run(dir.path()), "not a repository yet");
    run_git(dir.path(), &["init", "-q"]);
    assert_eq!(
        evaluate_with_outputs(&config, "2026-11-18", &nix, &flake()).exit,
        Exit::ConfigInvalid,
        "untracked"
    );
    run_git(dir.path(), &["add", ".nix-tops/baseline.json"]);
    run_git(dir.path(), &["commit", "-q", "-m", "baseline"]);
    assert!(run(dir.path()), "tracked and clean");
    fs::write(dir.path().join(".nix-tops/baseline.json"), "{\"x\":1}").expect("write");
    assert_eq!(
        evaluate_with_outputs(&config, "2026-11-18", &nix, &flake()).exit,
        Exit::ConfigInvalid,
        "dirty"
    );
}

#[test]
fn ratchet_without_a_baseline_file_fails_on_the_file_and_never_asks_git() {
    let (dir, nix) = setup(&ratchet());
    let panics =
        |_: &[&str]| -> Option<GitRun> { panic!("git is asked only when the baseline exists") };
    let outputs = || Ok(flake());
    let probes = Probes {
        outputs: &outputs,
        git: &panics,
    };
    let v = evaluate_with(
        &dir.path().join("nix-tops.toml"),
        "2026-11-18",
        &nix,
        Some(&probes),
    );
    assert_eq!((v.exit, keys(&v)), (Exit::ConfigInvalid, vec!["baseline"]));
}
