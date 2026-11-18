//! Helpers shared by the integration tests.
#![allow(dead_code)]

use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output},
};

pub fn nixtops(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_nixtops"))
        .args(args)
        .output()
        .expect("nixtops runs")
}

pub fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

pub fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A shell script that stands in for `nix`: prints `version_line` for `--version`, and a store
/// and config answer for the two `nix <experimental flags> ...` calls.
pub fn fake_nix(dir: &Path, version_line: &str) -> PathBuf {
    let path = dir.join("nix");
    let script = format!(
        "#!/bin/sh\ncase \"$*\" in\n  --version) echo '{version_line}' ;;\n  *store*info*) echo '{{\"url\":\"daemon\",\"version\":\"9.9.9\"}}' ;;\n  *config*show*) echo '{{\"sandbox\":{{\"value\":true}},\"experimental-features\":{{\"value\":[\"nix-command\",\"flakes\"]}},\"substituters\":{{\"value\":[\"https://cache.example/\"]}},\"builders\":{{\"value\":\"\"}},\"lazy-trees\":{{\"value\":false}}}}' ;;\n  *) exit 1 ;;\nesac\n"
    );
    fs::write(&path, script).expect("write fake nix");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod");
    path
}
