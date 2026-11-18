//! Property: for every non-empty set of facets that have a profile, what `init` writes passes
//! the schema, is byte-identical on a second run, and survives being merged into an existing
//! `AGENTS.md` any number of times.

mod common;
use nixtops::config::{self, render};
use std::{fs, process::Command};
use tempfile::tempdir;

const FACETS: [&str; 6] = [
    "dev-flake",
    "package-set",
    "nixos-module",
    "flake-lib",
    "nixos-host",
    "home-config",
];

fn subsets() -> Vec<Vec<String>> {
    (1u32..(1 << FACETS.len()))
        .map(|mask| {
            FACETS
                .iter()
                .enumerate()
                .filter(|(i, _)| mask & (1 << i) != 0)
                .map(|(_, f)| f.to_string())
                .collect()
        })
        .collect()
}

#[test]
fn property_written_config_passes_the_schema_for_every_facet_set() {
    let systems: Vec<String> = render::SYSTEMS.iter().map(|s| s.to_string()).collect();
    for set in subsets() {
        let text = render::config_toml(&set, &systems);
        assert!(config::load(&text).is_ok(), "{set:?}\n{text}");
        assert_eq!(text, render::config_toml(&set, &systems), "{set:?}");
    }
}

#[test]
fn property_agents_merge_is_idempotent_and_leaves_no_field_for_every_facet_set() {
    let systems: Vec<String> = render::SYSTEMS.iter().map(|s| s.to_string()).collect();
    for set in subsets() {
        let section = render::agents_section(&set, &systems, "ratchet");
        assert!(
            !section.contains("{FACETS}") && !section.contains("{BUILT_FLOOR}"),
            "{set:?}"
        );
        let once = render::merge_agents(Some("# mine\n"), &section);
        assert_eq!(render::merge_agents(Some(&once), &section), once, "{set:?}");
        assert!(once.starts_with("# mine\n"), "{set:?}");
    }
}

#[test]
fn property_init_run_twice_with_force_is_byte_identical_for_each_single_facet() {
    for facet in FACETS {
        let dir = tempdir().expect("tempdir");
        let run = || {
            Command::new(env!("CARGO_BIN_EXE_nixtops"))
                .args(["init", "--facet", facet, "--force"])
                .current_dir(dir.path())
                .output()
                .expect("runs")
        };
        assert_eq!(run().status.code(), Some(0), "{facet}");
        let read = || fs::read_to_string(dir.path().join("AGENTS.md")).expect("agents");
        let first = read();
        assert_eq!(run().status.code(), Some(0), "{facet}");
        assert_eq!(first, read(), "{facet}");
    }
}
