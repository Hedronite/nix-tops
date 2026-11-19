//! `nixtops outputs` on the command line.

use std::process::Command;

#[test]
fn a_build_without_a_pinned_judge_says_so_and_exits_3() {
    if option_env!("NIXTOPS_JUDGE").is_some_and(|judge| !judge.is_empty()) {
        return; // a kit build has a pinned judge; the other branch is the plain `cargo` build
    }
    let out = Command::new(env!("CARGO_BIN_EXE_nixtops"))
        .args(["outputs", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        err,
        "nixtops outputs: this build has no pinned judge (built outside the kit's flake)\n"
    );
}

#[test]
fn the_flake_argument_defaults_to_the_current_directory_and_json_is_a_flag() {
    let out = Command::new(env!("CARGO_BIN_EXE_nixtops"))
        .args(["outputs", "--help"])
        .output()
        .unwrap();
    let help = String::from_utf8_lossy(&out.stdout);
    assert!(
        help.contains("[FLAKE]") && help.contains("--json"),
        "{help}"
    );
}
