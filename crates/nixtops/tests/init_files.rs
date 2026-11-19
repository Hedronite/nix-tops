//! `nixtops init`: what it writes, what it refuses, and that a second run changes nothing.

mod common;

/// What a command printed, shaped like a process output so the tests read the same.
struct Status(u8);
impl Status {
    fn code(&self) -> Option<i32> {
        Some(i32::from(self.0))
    }
}
struct Res {
    status: Status,
    stdout: String,
    stderr: String,
}
fn from_output(o: nixtops::exit::Output) -> Res {
    Res {
        status: Status(o.exit.code()),
        stdout: o.stdout,
        stderr: o.stderr,
    }
}
fn stdout(r: &Res) -> String {
    r.stdout.clone()
}
fn stderr(r: &Res) -> String {
    r.stderr.clone()
}

#[derive(Debug, Parser)]
#[command(name = "nixtops")]
struct Cli {
    #[command(subcommand)]
    command: cmd::Command,
}
use clap::Parser;
use nixtops::cmd;
use std::{fs, path::Path};
use tempfile::tempdir;

fn init(dir: &Path, args: &[&str]) -> Res {
    let mut full = vec!["init"];
    full.extend(args);
    let parsed = Cli::try_parse_from(std::iter::once("nixtops").chain(full)).expect("args parse");
    let cmd::Command::Setup(cmd::Setup::Init(args)) = parsed.command else {
        panic!("not init")
    };
    from_output(cmd::init::init_in(dir, &args))
}

fn read(dir: &Path, rel: &str) -> String {
    fs::read_to_string(dir.join(rel)).unwrap_or_else(|_| panic!("{rel} exists"))
}

#[test]
fn it_writes_the_toml_the_workflow_and_the_agents_section_and_prints_the_flake_lines() {
    let dir = tempdir().expect("tempdir");
    let out = init(dir.path(), &["--facet", "package-set"]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert!(read(dir.path(), "nix-tops.toml").contains("facets = [\"package-set\"]"));
    assert!(read(dir.path(), ".github/workflows/nix-tops.yml").contains("gate --max-tier 1"));
    let agents = read(dir.path(), "AGENTS.md");
    assert!(agents.contains("<!-- nix-tops:begin -->") && agents.contains("`100%` built"));
    let text = stdout(&out);
    assert!(
        text.contains("never edits it") && text.contains("apps = perSystem"),
        "{text}"
    );
}

#[test]
fn it_never_touches_flake_nix_and_never_writes_a_baseline() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("flake.nix"), "{ }\n").expect("write");
    assert_eq!(
        init(dir.path(), &["--facet", "dev-flake"]).status.code(),
        Some(0)
    );
    assert_eq!(read(dir.path(), "flake.nix"), "{ }\n");
    assert!(!dir.path().join(".nix-tops").exists());
}

#[test]
fn it_refuses_to_overwrite_without_force_and_a_forced_second_run_is_byte_identical() {
    let dir = tempdir().expect("tempdir");
    assert_eq!(
        init(dir.path(), &["--facet", "dev-flake"]).status.code(),
        Some(0)
    );
    let files = [
        "nix-tops.toml",
        ".github/workflows/nix-tops.yml",
        "AGENTS.md",
    ];
    let first: Vec<String> = files.iter().map(|f| read(dir.path(), f)).collect();
    let refused = init(dir.path(), &["--facet", "dev-flake"]);
    assert_eq!(refused.status.code(), Some(4));
    assert!(stderr(&refused).contains("--force"));
    assert_eq!(
        init(dir.path(), &["--facet", "dev-flake", "--force"])
            .status
            .code(),
        Some(0)
    );
    let second: Vec<String> = files.iter().map(|f| read(dir.path(), f)).collect();
    assert_eq!(first, second);
}

#[test]
fn it_replaces_only_between_the_markers_and_keeps_the_rest_of_agents_md() {
    let dir = tempdir().expect("tempdir");
    fs::write(
        dir.path().join("AGENTS.md"),
        "# mine\n\n<!-- nix-tops:begin -->\nOLD-SECTION-TEXT\n<!-- nix-tops:end -->\n\nmy tail\n",
    )
    .expect("write");
    assert_eq!(
        init(dir.path(), &["--facet", "flake-lib"]).status.code(),
        Some(0)
    );
    let agents = read(dir.path(), "AGENTS.md");
    assert!(
        agents.starts_with("# mine\n\n<!-- nix-tops:begin -->"),
        "{agents}"
    );
    assert!(
        agents.ends_with("<!-- nix-tops:end -->\n\nmy tail\n")
            && !agents.contains("OLD-SECTION-TEXT")
    );
}

#[test]
fn a_bad_or_missing_facet_exits_2_and_writes_nothing() {
    for args in [&["--facet", "nope"][..], &[][..], &["--facet", "image"][..]] {
        let dir = tempdir().expect("tempdir");
        let out = init(dir.path(), args);
        assert_eq!(out.status.code(), Some(2), "{args:?} {}", stderr(&out));
        assert_eq!(fs::read_dir(dir.path()).expect("ls").count(), 0, "{args:?}");
    }
}

#[test]
fn what_init_writes_passes_check() {
    let dir = tempdir().expect("tempdir");
    assert_eq!(
        init(
            dir.path(),
            &["--facet", "nixos-module", "--facet", "package-set"]
        )
        .status
        .code(),
        Some(0)
    );
    let nix = common::fake_nix(dir.path(), "nix (Nix) 2.35.2");
    fs::create_dir(dir.path().join(".nix-tops")).expect("mkdir");
    fs::write(dir.path().join(".nix-tops/baseline.json"), "{}").expect("write");
    let out = from_output(nixtops::cmd::check::check_at(
        &dir.path().join("nix-tops.toml"),
        &nixtops::config::semantic::today_utc(),
        nix.to_str().expect("utf8"),
    ));
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
}

#[test]
fn a_directory_that_cannot_be_made_or_a_file_that_cannot_be_written_exits_3() {
    let blocked = tempdir().expect("tempdir");
    fs::write(
        blocked.path().join(".github"),
        "a file where the workflow directory goes",
    )
    .expect("write");
    let out = init(blocked.path(), &["--facet", "dev-flake"]);
    assert_eq!(out.status.code(), Some(3), "{}", stderr(&out));
    assert!(stderr(&out).contains("cannot create"));

    let unwritable = tempdir().expect("tempdir");
    fs::create_dir(unwritable.path().join("nix-tops.toml")).expect("mkdir");
    let out = init(unwritable.path(), &["--facet", "dev-flake", "--force"]);
    assert_eq!(out.status.code(), Some(3), "{}", stderr(&out));
    assert!(stderr(&out).contains("cannot write"));
}

#[test]
fn without_a_pinned_judge_it_says_the_systems_are_not_verified() {
    let dir = tempdir().expect("tempdir");
    let out = init(dir.path(), &["--facet", "dev-flake"]);
    assert_eq!(out.status.code(), Some(0));
    let text = stdout(&out);
    assert!(
        text.contains("not verified") && text.contains("nixtops check will verify them"),
        "{text}"
    );
}
