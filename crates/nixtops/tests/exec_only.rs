//! Every program the runner starts goes through `exec::output`, which retries a program that is
//! "text file busy" (the stand-in tool race). A `Command` run any other way would bring the race
//! back with no test noticing, so this scans the source and names the file and line.

use std::{
    fs,
    path::{Path, PathBuf},
};

/// Calls that run a `Command` without the retry: the method forms, the function-call forms
/// (`Command::output(&mut c)`), and `CommandExt::exec`, which replaces the process.
const FORBIDDEN: [&str; 8] = [
    ".output()",
    ".spawn()",
    ".status()",
    "Command::output(",
    "Command::spawn(",
    "Command::status(",
    ".exec(",
    "CommandExt::exec(",
];

fn rust_files(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, found);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            found.push(path);
        }
    }
}

/// The lines of `source` that call a forbidden method in code (a comment does not count), with
/// their 1-based numbers.
fn offences(source: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    for (index, line) in source.lines().enumerate() {
        let code = line.split("//").next().unwrap_or("");
        if FORBIDDEN.iter().any(|call| code.contains(call)) {
            found.push((index + 1, line.trim().to_string()));
        }
    }
    found
}

#[test]
fn no_command_is_run_outside_exec() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files(&src, &mut files);
    assert!(files.len() > 10, "the scan found {} files", files.len());
    let mut bad = Vec::new();
    for file in files
        .iter()
        .filter(|f| f.file_name().is_some_and(|n| n != "exec.rs"))
    {
        for (line, text) in offences(&fs::read_to_string(file).unwrap()) {
            bad.push(format!(
                "{}:{line}: {text}",
                file.strip_prefix(&src).unwrap().display()
            ));
        }
    }
    assert!(
        bad.is_empty(),
        "run these through exec::output:\n{}",
        bad.join("\n")
    );
}

#[test]
fn the_scan_finds_what_it_forbids_and_ignores_what_it_does_not() {
    let source = "let a = Command::new(\"x\").output();\nlet b = c.spawn();\nlet d = c.status();\n// c.output() in a comment\nlet e = out.status.success();\nlet f = crate::exec::output(&mut c);\nlet g = Command::output(&mut c);\nlet h = std::process::Command::spawn(&mut c);\nlet i = Command::status(&mut c);\nlet j = c.exec();\nlet k = CommandExt::exec(&mut c);\n";
    let lines: Vec<usize> = offences(source).into_iter().map(|(n, _)| n).collect();
    assert_eq!(lines, [1, 2, 3, 7, 8, 9, 10, 11]);
}
