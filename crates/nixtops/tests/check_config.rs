//! `nixtops check`: schema failures and the semantic checks, through the binary. Every failure
//! exits 2 and names its key; a host below the daemon floor exits 3.

mod common;
use common::{fake_nix, stderr, stdout};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
use tempfile::tempdir;

const GOOD: &str = "protocol = \"0.2.0\"\nfacets = [\"dev-flake\"]\nenforce = \"hard\"\n\n[toolchain]\nsystems = [\"x86_64-linux\"]\n";

fn check(dir: &Path, nix_line: &str) -> Output {
    let nix = fake_nix(dir, nix_line);
    Command::new(env!("CARGO_BIN_EXE_nixtops"))
        .arg("check")
        .current_dir(dir)
        .env("NIXTOPS_NIX", nix)
        .output()
        .expect("nixtops runs")
}

fn with(toml: &str) -> (tempfile::TempDir, Output) {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("nix-tops.toml"), toml).expect("write");
    let out = check(dir.path(), "nix (Nix) 2.35.2");
    (dir, out)
}

fn fails_naming(toml: &str, key: &str) {
    let (_dir, out) = with(toml);
    assert_eq!(out.status.code(), Some(2), "{toml}\n{}", stderr(&out));
    assert!(
        stderr(&out).contains(&format!("nix-tops.toml: {key}:")),
        "want key {key} in:\n{}",
        stderr(&out)
    );
}

#[test]
fn a_good_config_passes_and_reports_each_facet() {
    let (_dir, out) = with(GOOD);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(
        text.contains("facet dev-flake: profile found") && text.contains("nix-tops.toml: ok"),
        "{text}"
    );
}

#[test]
fn an_unknown_key_a_wrong_type_and_an_unknown_facet_each_exit_2_naming_the_key() {
    fails_naming(&format!("bogus = 1\n{GOOD}"), "bogus");
    fails_naming(&format!("{GOOD}bogus = 1\n"), "toolchain.bogus");
    fails_naming(
        &format!("{GOOD}\n[eval]\nsurprise = true\n"),
        "eval.surprise",
    );
    fails_naming(
        &GOOD.replace("enforce = \"hard\"", "enforce = 3"),
        "enforce",
    );
    fails_naming(
        &GOOD.replace("\"dev-flake\"", "\"not-a-facet\""),
        "facets[0]",
    );
    fails_naming(&GOOD.replace("0.2.0", "0.1.0"), "protocol");
}

#[test]
fn facet_is_refused_and_missing_facets_or_systems_are_named() {
    fails_naming(
        &GOOD.replace("facets = [", "facet = \"dev-flake\"\nfacets = ["),
        "facet",
    );
    fails_naming(
        "protocol = \"0.2.0\"\n[toolchain]\nsystems = [\"x86_64-linux\"]\n",
        "facets",
    );
    fails_naming(
        "protocol = \"0.2.0\"\nfacets = [\"dev-flake\"]\n",
        "toolchain",
    );
    fails_naming(
        &GOOD.replace("x86_64-linux", "x86_64-darwin"),
        "toolchain.systems[0]",
    );
}

#[test]
fn broken_toml_and_a_missing_file_exit_2() {
    fails_naming("protocol = ", "(file)");
    let dir = tempdir().expect("tempdir");
    let out = check(dir.path(), "nix (Nix) 2.35.2");
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("(file)"));
}

#[test]
fn advisory_is_refused_naming_enforce_and_the_missing_key() {
    fails_naming(&GOOD.replace("hard", "advisory"), "enforce");
    let (_d, out) = with(&GOOD.replace("hard", "advisory"));
    assert!(stderr(&out).contains("no exemption key"));
}

#[test]
fn ratchet_needs_the_baseline_file() {
    let ratchet = GOOD.replace("enforce = \"hard\"\n", "");
    fails_naming(&ratchet, "baseline");
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("nix-tops.toml"), &ratchet).expect("write");
    fs::create_dir(dir.path().join(".nix-tops")).expect("mkdir");
    fs::write(dir.path().join(".nix-tops/baseline.json"), "{}").expect("write");
    assert_eq!(check(dir.path(), "nix (Nix) 2.35.2").status.code(), Some(0));
}

#[test]
fn an_expired_ledger_entry_fails_and_a_future_one_passes() {
    let entry = |until: &str| {
        format!("{GOOD}\n[[warnings.allow]]\nmatch = \"x\"\nreason = \"r\"\nuntil = {until}\n")
    };
    fails_naming(&entry("\"2000-01-01\""), "warnings.allow[0].until");
    fails_naming(&entry("2000-01-01"), "warnings.allow[0].until");
    let (_d, ok) = with(&entry("\"2999-12-31\""));
    assert_eq!(ok.status.code(), Some(0), "{}", stderr(&ok));
    fails_naming(
        &format!("{GOOD}\n[[warnings.allow]]\nmatch = \"x\"\nuntil = \"2999-12-31\"\n"),
        "warnings.allow[0].reason",
    );
}

fn lock(follows: bool) -> String {
    let inner = if follows {
        "[\"nixpkgs\"]"
    } else {
        "\"nixpkgs_2\""
    };
    format!(
        "{{\"root\":\"root\",\"version\":7,\"nodes\":{{\"root\":{{\"inputs\":{{\"nix-tops\":\"nix-tops\",\"nixpkgs\":\"nixpkgs\"}}}},\"nix-tops\":{{\"inputs\":{{\"nixpkgs\":{inner}}}}},\"nixpkgs\":{{}}}}}}"
    )
}

#[test]
fn a_kit_input_that_follows_nixpkgs_fails_and_one_that_does_not_passes() {
    for (follows, code) in [(true, 2), (false, 0)] {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join("nix-tops.toml"), GOOD).expect("write");
        fs::write(dir.path().join("flake.lock"), lock(follows)).expect("write");
        let out = check(dir.path(), "nix (Nix) 2.35.2");
        assert_eq!(out.status.code(), Some(code), "{}", stderr(&out));
        if follows {
            assert!(stderr(&out).contains("inputs.nix-tops.inputs.nixpkgs"));
        }
    }
}

#[test]
fn a_host_below_the_daemon_floor_exits_3_and_a_config_failure_wins() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("nix-tops.toml"), GOOD).expect("write");
    let low = check(dir.path(), "nix (Nix) 2.23.4");
    assert_eq!(low.status.code(), Some(3));
    assert!(stdout(&low).contains("daemon floor not met"));
    fs::write(
        dir.path().join("nix-tops.toml"),
        format!("{GOOD}bogus = 1\n"),
    )
    .expect("write");
    assert_eq!(check(dir.path(), "nix (Nix) 2.23.4").status.code(), Some(2));
}

#[test]
fn the_schema_in_the_binary_is_the_file_in_the_repo() {
    let on_disk =
        fs::read_to_string(common::repo_root().join("schema/nix-tops.schema.json")).expect("read");
    assert_eq!(nixtops::config::schema::CONFIG_SCHEMA, on_disk);
}
