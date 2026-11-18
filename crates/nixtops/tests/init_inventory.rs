//! `init` with the outputs read: facets and systems come from the flake, narrowed by `--facet`.

use nixtops::{
    cmd::init::{Args, init_with},
    config::{self, outputs::Read},
    inventory::Structure,
};
use std::{cell::Cell, fs};
use tempfile::tempdir;

fn flake() -> Structure {
    let mut s = Structure::default();
    s.insert(
        "packages",
        vec!["x86_64-linux".into(), "aarch64-linux".into()],
    );
    s.insert(
        "devShells",
        vec!["aarch64-linux".into(), "x86_64-linux".into()],
    );
    s
}

fn args(facets: &[&str], force: bool) -> Args {
    Args {
        facets: facets.iter().map(|f| f.to_string()).collect(),
        force,
    }
}

#[test]
fn without_facet_it_writes_every_derived_facet_and_the_exported_systems() {
    let dir = tempdir().expect("tempdir");
    let out = init_with(dir.path(), &args(&[], false), &|| Read::Found(flake()));
    assert_eq!(out.exit.code(), 0, "{}", out.stderr);
    let toml = fs::read_to_string(dir.path().join("nix-tops.toml")).expect("written");
    assert!(
        toml.contains("facets = [\"dev-flake\", \"package-set\"]"),
        "{toml}"
    );
    assert!(
        toml.contains("systems = [\"aarch64-linux\", \"x86_64-linux\"]"),
        "{toml}"
    );
    assert!(config::load(&toml).is_ok());
}

#[test]
fn facet_narrows_and_may_only_name_a_facet_the_outputs_show() {
    let dir = tempdir().expect("tempdir");
    let out = init_with(dir.path(), &args(&["package-set"], false), &|| {
        Read::Found(flake())
    });
    assert_eq!(out.exit.code(), 0);
    let toml = fs::read_to_string(dir.path().join("nix-tops.toml")).expect("written");
    assert!(toml.contains("facets = [\"package-set\"]"));
    let other = tempdir().expect("tempdir");
    let bad = init_with(other.path(), &args(&["nixos-module"], false), &|| {
        Read::Found(flake())
    });
    assert_eq!(bad.exit.code(), 2);
    assert!(
        bad.stderr
            .contains("do not show it (they show: dev-flake, package-set)"),
        "{}",
        bad.stderr
    );
    assert_eq!(fs::read_dir(other.path()).expect("ls").count(), 0);
}

#[test]
fn cheap_refusals_come_before_the_outputs_are_read() {
    let dir = tempdir().expect("tempdir");
    let reads = Cell::new(0);
    let read = || {
        reads.set(reads.get() + 1);
        Read::Found(flake())
    };
    assert_eq!(
        init_with(dir.path(), &args(&["image"], false), &read)
            .exit
            .code(),
        2
    );
    assert_eq!(
        init_with(dir.path(), &args(&["nope"], false), &read)
            .exit
            .code(),
        2
    );
    assert_eq!(
        init_with(dir.path(), &args(&[], false), &read).exit.code(),
        0
    );
    assert_eq!(reads.get(), 1);
    assert_eq!(
        init_with(dir.path(), &args(&[], false), &read).exit.code(),
        4
    );
    assert_eq!(reads.get(), 1, "an overwrite is refused before the read");
}

#[test]
fn unreadable_outputs_exit_3_and_an_unlisted_output_exit_3() {
    let dir = tempdir().expect("tempdir");
    let out = init_with(dir.path(), &args(&[], false), &|| {
        Read::Failed("boom".into())
    });
    assert_eq!(out.exit.code(), 3);
    assert!(out.stderr.contains("boom"));
    let mut s = flake();
    s.insert_error("checks", "bad".into());
    let out = init_with(dir.path(), &args(&[], false), &|| Read::Found(s.clone()));
    assert_eq!(out.exit.code(), 3);
    assert!(out.stderr.contains("checks"));
    assert_eq!(fs::read_dir(dir.path()).expect("ls").count(), 0);
}

#[test]
fn a_flake_with_no_facet_or_an_unsupported_system_is_refused() {
    let dir = tempdir().expect("tempdir");
    let empty = Structure::default();
    assert_eq!(
        init_with(dir.path(), &args(&[], false), &|| Read::Found(
            empty.clone()
        ))
        .exit
        .code(),
        2
    );
    let mut s = Structure::default();
    s.insert("packages", vec!["x86_64-darwin".into()]);
    let out = init_with(dir.path(), &args(&[], false), &|| Read::Found(s.clone()));
    assert_eq!(out.exit.code(), 2);
    assert!(out.stderr.contains("x86_64-darwin"));
}

#[test]
fn a_repeated_facet_is_written_once() {
    let dir = tempdir().expect("tempdir");
    let out = init_with(
        dir.path(),
        &args(&["dev-flake", "dev-flake"], false),
        &|| Read::Found(flake()),
    );
    assert_eq!(out.exit.code(), 0);
    let toml = fs::read_to_string(dir.path().join("nix-tops.toml")).expect("written");
    assert!(toml.contains("facets = [\"dev-flake\"]"));
}
