//! Gate 1, source (protocol section 3.1, N-04 and N-05): every `.nix` file under the flake root
//! is tracked by git, `flake.lock` is committed and current, and in CI the tree is clean. Only
//! `git` is asked about files; the lock is judged by the pinned Nix with `--no-update-lock-file`.

use crate::{
    inventory::Session,
    report::{Layer, LayerId, Verdict},
};
use std::{path::Path, process::Command};

/// What the source gate looks at.
pub struct Input<'a> {
    pub root: &'a Path,
    pub flake_ref: &'a str,
    /// Set when running in CI: the working tree must equal `HEAD`.
    pub ci: bool,
}

/// A rule the source gate found broken.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub law: &'static str,
    pub message: String,
}

impl Finding {
    fn new(law: &'static str, message: String) -> Self {
        Self { law, message }
    }
}

fn git(root: &Path, args: &[&str]) -> Option<(bool, String)> {
    let out = crate::exec::output(Command::new("git").arg("-C").arg(root).args(args)).ok()?;
    Some((
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    ))
}

/// A `.nix` path outside `.direnv`.
fn is_source_nix(path: &str) -> bool {
    path.ends_with(".nix") && !path.split('/').any(|part| part == ".direnv")
}

/// Untracked and staged-not-committed `.nix` files from `git status --porcelain=v1 -z`. A file
/// that is staged and not committed counts as untracked (N-04).
pub fn untracked_from_status(status_z: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut fields = status_z.split('\0').filter(|f| !f.is_empty());
    while let Some(field) = fields.next() {
        let (code, path) = field.split_at(field.len().min(3));
        let (index, tree) = (
            code.chars().next().unwrap_or(' '),
            code.chars().nth(1).unwrap_or(' '),
        );
        if matches!(index, 'R' | 'C') {
            fields.next();
        }
        let new = (index, tree) == ('?', '?') || (index == 'A' && !matches!(tree, 'D'));
        if new && is_source_nix(path) {
            found.push(path.to_string());
        }
    }
    found
}

/// Ignored `.nix` files from `git ls-files --others --ignored --exclude-standard -z`: nix does not
/// see them either, so a flake that imports one fails.
pub fn ignored_nix(listing_z: &str) -> Vec<String> {
    listing_z
        .split('\0')
        .filter(|p| is_source_nix(p))
        .map(str::to_string)
        .collect()
}

fn untracked_findings(root: &Path) -> Vec<Finding> {
    let status = git(root, &["status", "--porcelain=v1", "-z", "--", "."])
        .map(|(_, out)| out)
        .unwrap_or_default();
    let ignored = git(
        root,
        &[
            "ls-files",
            "--others",
            "--ignored",
            "--exclude-standard",
            "-z",
            "--",
            ".",
        ],
    )
    .map(|(_, o)| o)
    .unwrap_or_default();
    let paths = untracked_from_status(&status)
        .into_iter()
        .chain(ignored_nix(&ignored));
    paths
        .map(|path| Finding::new("N-04", format!("{path} is not tracked by git")))
        .collect()
}

fn lock_committed_findings(root: &Path) -> Vec<Finding> {
    let tracked =
        git(root, &["ls-files", "--error-unmatch", "flake.lock"]).is_some_and(|(ok, _)| ok);
    if !tracked {
        return vec![Finding::new(
            "N-05",
            "flake.lock is not committed".to_string(),
        )];
    }
    let clean =
        git(root, &["diff", "--quiet", "HEAD", "--", "flake.lock"]).is_some_and(|(ok, _)| ok);
    if clean {
        Vec::new()
    } else {
        vec![Finding::new(
            "N-05",
            "flake.lock has uncommitted changes".to_string(),
        )]
    }
}

fn lock_current_findings(session: &Session, input: &Input) -> Vec<Finding> {
    let run = session.judge(
        &[
            "flake",
            "metadata",
            "--json",
            "--no-update-lock-file",
            input.flake_ref,
        ],
        &[],
    );
    if run.ok {
        return Vec::new();
    }
    let first = run
        .stderr
        .lines()
        .find(|l| l.contains("error") || l.contains("lock"))
        .unwrap_or("evaluation with --no-update-lock-file failed");
    vec![Finding::new(
        "N-05",
        format!("flake.lock is not current: {}", first.trim()),
    )]
}

fn clean_tree_findings(root: &Path, ci: bool) -> Vec<Finding> {
    if !ci {
        return Vec::new();
    }
    let status = git(root, &["status", "--porcelain=v1", "-z", "--", "."])
        .map(|(_, out)| out)
        .unwrap_or_default();
    if status.is_empty() {
        Vec::new()
    } else {
        vec![Finding::new(
            "N-04",
            "the working tree is not clean in CI".to_string(),
        )]
    }
}

/// Every finding of the source gate.
pub fn findings(session: &Session, input: &Input) -> Vec<Finding> {
    if !git(input.root, &["rev-parse", "--show-toplevel"]).is_some_and(|(ok, _)| ok) {
        return vec![Finding::new(
            "N-04",
            "the flake root is not inside a git repository".to_string(),
        )];
    }
    let mut all = untracked_findings(input.root);
    all.extend(lock_committed_findings(input.root));
    all.extend(lock_current_findings(session, input));
    all.extend(clean_tree_findings(input.root, input.ci));
    all
}

/// The layer for a set of findings.
pub fn layer(found: &[Finding]) -> Layer {
    if found.is_empty() {
        return Layer::new(LayerId::Source, Verdict::Pass, None);
    }
    let lines: Vec<String> = found
        .iter()
        .map(|f| format!("{}: {}", f.law, f.message))
        .collect();
    Layer::new(LayerId::Source, Verdict::Refuse, Some(lines.join("\n")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untracked_and_staged_nix_files_are_found_and_direnv_is_exempt() {
        let status = "?? extra.nix\0A  staged.nix\0AM both.nix\0 M tracked.nix\0?? .direnv/x.nix\0?? notes.txt\0?? sub/dir/y.nix\0";
        assert_eq!(
            untracked_from_status(status),
            ["extra.nix", "staged.nix", "both.nix", "sub/dir/y.nix"]
        );
    }

    #[test]
    fn a_rename_skips_its_source_path_and_a_staged_delete_is_not_new() {
        let status = "R  new.nix\0old.nix\0AD gone.nix\0?? last.nix\0";
        assert_eq!(untracked_from_status(status), ["last.nix"]);
    }

    #[test]
    fn ignored_nix_files_count_except_under_direnv() {
        assert_eq!(
            ignored_nix("a.nix\0.direnv/b.nix\0c.txt\0d/e.nix\0"),
            ["a.nix", "d/e.nix"]
        );
        assert!(ignored_nix("").is_empty());
    }

    #[test]
    fn the_layer_is_pass_without_findings_and_refuse_naming_each_law_with_them() {
        assert_eq!(layer(&[]).verdict, Verdict::Pass);
        let l = layer(&[
            Finding::new("N-04", "a.nix is not tracked by git".into()),
            Finding::new("N-05", "lock".into()),
        ]);
        assert_eq!(l.verdict, Verdict::Refuse);
        assert_eq!(
            l.reason.as_deref(),
            Some("N-04: a.nix is not tracked by git\nN-05: lock")
        );
    }
}
