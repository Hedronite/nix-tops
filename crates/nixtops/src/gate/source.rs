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

fn is_nix(path: &str) -> bool {
    path.ends_with(".nix")
}

/// A `.nix` path under an ignored `.direnv` (the one exemption of N-04: nix does not see an
/// ignored directory either, and a tool such as direnv writes there). `.direnvx` is not it.
fn under_direnv(path: &str) -> bool {
    path.split('/').any(|part| part == ".direnv")
}

/// Untracked and staged-not-committed `.nix` files from
/// `git status --porcelain=v1 -z --untracked-files=all`. A file that is staged and not committed
/// counts as untracked (N-04): an added file, and the new path of a staged rename or copy (with
/// `-z` the new path comes first, the old one after it). An untracked `.direnv` that is not
/// ignored is not exempt: only an ignored one is (see [`ignored_nix`]).
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
        let new = (index, tree) == ('?', '?')
            || (matches!(index, 'A' | 'R' | 'C') && !matches!(tree, 'D'));
        if new && is_nix(path) {
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
        .filter(|p| is_nix(p) && !under_direnv(p))
        .map(str::to_string)
        .collect()
}

fn untracked_findings(root: &Path) -> Vec<Finding> {
    let status = git(
        root,
        &[
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--",
            ".",
        ],
    )
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

/// Whether the flake declares inputs, from `nix flake metadata --json`: the root node of its lock
/// lists them. Nix writes no `flake.lock` for a flake without inputs. When the answer is not
/// there (no `locks` key), the flake is taken to have inputs, so a missing lock still fails.
pub fn declares_inputs(metadata_json: &str) -> bool {
    let Ok(metadata) = serde_json::from_str::<serde_json::Value>(metadata_json) else {
        return true;
    };
    let Some(locks) = metadata.get("locks") else {
        return true;
    };
    let root = locks.get("root").and_then(|r| r.as_str()).unwrap_or("root");
    let inputs = locks.pointer(&format!("/nodes/{root}/inputs"));
    inputs
        .and_then(|i| i.as_object())
        .is_some_and(|i| !i.is_empty())
}

/// N-05, committed: `flake.lock` is tracked and unmodified. A flake without inputs has no lock to
/// commit; that is accepted, and the note says why.
fn lock_committed(session: &Session, input: &Input) -> (Vec<Finding>, Vec<String>) {
    let root = input.root;
    let tracked =
        git(root, &["ls-files", "--error-unmatch", "flake.lock"]).is_some_and(|(ok, _)| ok);
    if !tracked {
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
        if run.ok && !declares_inputs(&run.stdout) {
            let note =
                "no flake.lock: the flake declares no inputs, so Nix writes none".to_string();
            return (Vec::new(), vec![note]);
        }
        let finding = Finding::new("N-05", "flake.lock is not committed".to_string());
        return (vec![finding], Vec::new());
    }
    let clean =
        git(root, &["diff", "--quiet", "HEAD", "--", "flake.lock"]).is_some_and(|(ok, _)| ok);
    if clean {
        return (Vec::new(), Vec::new());
    }
    let finding = Finding::new("N-05", "flake.lock has uncommitted changes".to_string());
    (vec![finding], Vec::new())
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

/// What the source gate found, and the notes for a pass that needs one.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    pub findings: Vec<Finding>,
    pub notes: Vec<String>,
}

/// Every finding of the source gate.
pub fn findings(session: &Session, input: &Input) -> Outcome {
    if !git(input.root, &["rev-parse", "--show-toplevel"]).is_some_and(|(ok, _)| ok) {
        let finding = Finding::new(
            "N-04",
            "the flake root is not inside a git repository".to_string(),
        );
        return Outcome {
            findings: vec![finding],
            notes: Vec::new(),
        };
    }
    let mut all = untracked_findings(input.root);
    let (committed, notes) = lock_committed(session, input);
    all.extend(committed);
    all.extend(lock_current_findings(session, input));
    all.extend(clean_tree_findings(input.root, input.ci));
    Outcome {
        findings: all,
        notes,
    }
}

/// The layer for what was found: `pass` (with the notes as its reason, if any) or `Refuse`
/// naming every finding.
pub fn layer(outcome: &Outcome) -> Layer {
    if outcome.findings.is_empty() {
        let reason = (!outcome.notes.is_empty()).then(|| outcome.notes.join("; "));
        return Layer::new(LayerId::Source, Verdict::Pass, reason);
    }
    let lines: Vec<String> = outcome
        .findings
        .iter()
        .map(|f| format!("{}: {}", f.law, f.message))
        .collect();
    Layer::new(LayerId::Source, Verdict::Refuse, Some(lines.join("\n")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untracked_staged_and_renamed_nix_files_are_found() {
        let status = "?? extra.nix\0A  staged.nix\0AM both.nix\0 M tracked.nix\0?? notes.txt\0?? sub/dir/y.nix\0?? newdir/z.nix\0";
        assert_eq!(
            untracked_from_status(status),
            [
                "extra.nix",
                "staged.nix",
                "both.nix",
                "sub/dir/y.nix",
                "newdir/z.nix"
            ]
        );
    }

    #[test]
    fn an_untracked_direnv_is_not_exempt_and_direnvx_never_was() {
        let status = "?? .direnv/x.nix\0?? .direnvx/x.nix\0";
        assert_eq!(
            untracked_from_status(status),
            [".direnv/x.nix", ".direnvx/x.nix"]
        );
    }

    #[test]
    fn a_staged_rename_or_copy_counts_by_its_new_path_and_a_staged_delete_is_not_new() {
        let status = "R  new.nix\0old.nix\0C  copy.nix\0src.nix\0RM renamed.nix\0was.nix\0R  doc.txt\0old.txt\0AD gone.nix\0?? last.nix\0";
        assert_eq!(
            untracked_from_status(status),
            ["new.nix", "copy.nix", "renamed.nix", "last.nix"]
        );
    }

    #[test]
    fn ignored_nix_files_count_except_under_an_ignored_direnv() {
        assert_eq!(
            ignored_nix("a.nix\0.direnv/b.nix\0c.txt\0d/e.nix\0.direnvx/f.nix\0"),
            ["a.nix", "d/e.nix", ".direnvx/f.nix"]
        );
        assert!(ignored_nix("").is_empty());
    }

    #[test]
    fn a_flake_declares_inputs_when_the_root_of_its_lock_lists_some() {
        let with = r#"{"locks":{"nodes":{"root":{"inputs":{"nixpkgs":"nixpkgs"}}},"root":"root","version":7}}"#;
        let without = r#"{"locks":{"nodes":{"root":{}},"root":"root","version":7}}"#;
        let empty = r#"{"locks":{"nodes":{"root":{"inputs":{}}},"root":"root"}}"#;
        assert!(declares_inputs(with));
        assert!(!declares_inputs(without));
        assert!(!declares_inputs(empty));
        // Not knowing is taken as "has inputs": a missing lock still fails.
        assert!(declares_inputs(r#"{"url":"u"}"#));
        assert!(declares_inputs("not json"));
    }

    #[test]
    fn the_layer_is_pass_without_findings_and_refuse_naming_each_law_with_them() {
        assert_eq!(layer(&Outcome::default()).verdict, Verdict::Pass);
        assert_eq!(layer(&Outcome::default()).reason, None);
        let noted = Outcome {
            findings: vec![],
            notes: vec!["a".into(), "b".into()],
        };
        assert_eq!(layer(&noted).reason.as_deref(), Some("a; b"));
        let l = layer(&Outcome {
            findings: vec![
                Finding::new("N-04", "a.nix is not tracked by git".into()),
                Finding::new("N-05", "lock".into()),
            ],
            notes: vec!["ignored when refusing".into()],
        });
        assert_eq!(l.verdict, Verdict::Refuse);
        assert_eq!(
            l.reason.as_deref(),
            Some("N-04: a.nix is not tracked by git\nN-05: lock")
        );
    }
}
