//! Helpers shared by the integration tests.
#![allow(dead_code)]

use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output},
};

pub fn nixtops(args: &[&str]) -> Output {
    // CI is set on a CI runner; the gate reads it (a dry run is refused, the tree must be clean),
    // so a test that does not mean to depend on it clears it for the child.
    Command::new(env!("CARGO_BIN_EXE_nixtops"))
        .args(args)
        .env_remove("CI")
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

/// Writes an executable file so that no other thread can exec it while a write handle is open:
/// the text goes to a temporary name, is synced and closed, and only then renamed into place. A
/// fork in another test thread cannot inherit a handle to a file that is already finished.
pub fn write_executable(path: &Path, text: &str) {
    use std::io::Write;
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    let mut file = fs::File::create(&temporary).expect("create");
    file.write_all(text.as_bytes()).expect("write");
    file.sync_all().expect("sync");
    drop(file);
    fs::set_permissions(&temporary, fs::Permissions::from_mode(0o755)).expect("chmod");
    fs::rename(&temporary, path).expect("rename");
}

/// A shell script that stands in for `nix`: prints `version_line` for `--version`, and a store
/// and config answer for the two `nix <experimental flags> ...` calls.
pub fn fake_nix(dir: &Path, version_line: &str) -> PathBuf {
    let path = dir.join("nix");
    let script = format!(
        "#!/bin/sh\ncase \"$*\" in\n  --version) echo '{version_line}' ;;\n  *store*info*) echo '{{\"url\":\"daemon\",\"version\":\"9.9.9\"}}' ;;\n  *config*show*) echo '{{\"sandbox\":{{\"value\":true}},\"experimental-features\":{{\"value\":[\"nix-command\",\"flakes\"]}},\"substituters\":{{\"value\":[\"https://cache.example/\"]}},\"builders\":{{\"value\":\"\"}},\"lazy-trees\":{{\"value\":false}}}}' ;;\n  *) exit 1 ;;\nesac\n"
    );
    write_executable(&path, &script);
    path
}
