//! `init` output for each facet equals the files under `tests/goldens/<facet>/`. To regenerate
//! after an intended change, run with `NIXTOPS_UPDATE_GOLDENS=1` (test code only; CI never sets it).

mod common;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use tempfile::tempdir;

const FACETS: [&str; 6] = [
    "dev-flake",
    "package-set",
    "nixos-module",
    "flake-lib",
    "nixos-host",
    "home-config",
];
const FILES: [&str; 3] = [
    "nix-tops.toml",
    ".github/workflows/nix-tops.yml",
    "AGENTS.md",
];

fn golden_dir(facet: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/goldens")
        .join(facet)
}

#[test]
fn init_matches_the_goldens() {
    let update = std::env::var_os("NIXTOPS_UPDATE_GOLDENS").is_some();
    for facet in FACETS {
        let dir = tempdir().expect("tempdir");
        let out = Command::new(env!("CARGO_BIN_EXE_nixtops"))
            .args(["init", "--facet", facet])
            .current_dir(dir.path())
            .output()
            .expect("runs");
        assert_eq!(out.status.code(), Some(0), "{facet}");
        for file in FILES {
            let got = fs::read_to_string(dir.path().join(file)).expect("written");
            let want_path = golden_dir(facet).join(file);
            if update {
                fs::create_dir_all(want_path.parent().expect("parent")).expect("mkdir");
                fs::write(&want_path, &got).expect("write golden");
            }
            let want = fs::read_to_string(&want_path)
                .unwrap_or_else(|_| panic!("golden {facet}/{file} exists"));
            assert_eq!(got, want, "{facet}/{file} differs from its golden");
        }
    }
}
