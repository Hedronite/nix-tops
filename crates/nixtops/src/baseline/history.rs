//! What git says about the baseline: whether `nixtops baseline` may write (an existing one must
//! be the committed one, a missing one must never have been committed, and the tree must be what
//! `HEAD` has), and the committed version before the current one.

use crate::config::baseline::{self, Git, State};

/// One git run's trimmed output, `None` when it failed.
fn output(git: Git<'_>, args: &[&str]) -> Option<String> {
    let run = git(args)?;
    run.ok.then(|| run.stdout.trim().to_string())
}

fn present(git: Git<'_>, path: &str) -> Result<(), String> {
    match baseline::state(git, path) {
        State::Clean => Ok(()),
        State::NoGit => Err("git cannot be run".into()),
        State::NotRepo => Err("not a git repository".into()),
        State::Untracked => Err(format!("{path} is not tracked by git; commit it first")),
        State::Dirty => Err(format!(
            "{path} differs from HEAD; restore it or commit it first"
        )),
    }
}

fn absent(git: Git<'_>, path: &str) -> Result<(), String> {
    let Some(shallow) = output(git, &["rev-parse", "--is-shallow-repository"]) else {
        return Err("not a git repository; its history cannot be read".into());
    };
    if shallow != "false" {
        return Err(format!(
            "this clone is shallow, so a deleted {path} cannot be ruled out; \
             run git fetch --unshallow"
        ));
    }
    if output(git, &["rev-parse", "--verify", "--quiet", "HEAD"]).is_none() {
        return Ok(());
    }
    match output(git, &["rev-list", "-1", "HEAD", "--", path]) {
        Some(commit) if commit.is_empty() => Ok(()),
        Some(_) => Err(format!(
            "git history has {path} and it is gone; deleting a baseline is not a reset, restore it"
        )),
        None => Err("git history cannot be read".into()),
    }
}

/// Why `baseline` must not write `path`, if it must not.
pub fn may_write(git: Git<'_>, path: &str, exists: bool) -> Result<(), String> {
    if exists {
        present(git, path)
    } else {
        absent(git, path)
    }
}

/// The commits that touched `path`, newest first.
fn commits(git: Git<'_>, path: &str) -> Vec<String> {
    let spec = format!(":(literal){path}");
    let log = output(git, &["log", "--format=%H", "--", &spec]).unwrap_or_default();
    log.lines().map(str::to_string).collect()
}

fn object(commit: &str, path: &str) -> String {
    format!("{commit}:./{path}")
}

fn exists_at(git: Git<'_>, commit: &str, path: &str) -> bool {
    output(git, &["cat-file", "-e", &object(commit, path)]).is_some()
}

/// `Err` names what differs from `HEAD`, untracked files included and ignored ones not. Paths in
/// `allowed` (relative to the flake root) may differ: the baseline is then taken from the tree
/// `HEAD` has.
pub fn tree_is_head(git: Git<'_>, allowed: &[&str]) -> Result<(), String> {
    let unreadable = || "git status cannot be read".to_string();
    let prefix = output(git, &["rev-parse", "--show-prefix"]).ok_or_else(unreadable)?;
    let status = git(&[
        "status",
        "--porcelain=v1",
        "-z",
        "--no-renames",
        "--untracked-files=all",
    ])
    .filter(|run| run.ok)
    .ok_or_else(unreadable)?;
    let allowed: Vec<String> = allowed.iter().map(|p| format!("{prefix}{p}")).collect();
    let entries = status.stdout.split('\0').filter(|entry| !entry.is_empty());
    let differing: Vec<&str> = entries
        .map(|entry| entry.get(3..).unwrap_or(entry))
        .filter(|path| !allowed.iter().any(|a| a == path))
        .collect();
    if differing.is_empty() {
        return Ok(());
    }
    let shown: Vec<&str> = differing.iter().take(5).copied().collect();
    Err(format!(
        "{} path(s) differ from HEAD ({}); commit or remove them first, so the baseline is \
         taken from what HEAD has",
        differing.len(),
        shown.join(", ")
    ))
}

/// The version of the baseline before the one now committed.
#[derive(Debug, PartialEq, Eq)]
pub struct Prior {
    /// The commit that wrote the earlier version.
    pub commit: String,
    pub text: String,
}

/// `None` when `path` was written once. A commit that deleted it is passed over, so deleting and
/// adding again is still held against what was there.
pub fn prior(git: Git<'_>, path: &str) -> Result<Option<Prior>, String> {
    let mut earlier = commits(git, path).into_iter().skip(1);
    let Some(commit) = earlier.find(|commit| exists_at(git, commit, path)) else {
        return Ok(None);
    };
    let run = git(&["show", &object(&commit, path)]).filter(|run| run.ok);
    let text = run
        .map(|run| run.stdout)
        .ok_or_else(|| format!("cannot read {path} at {commit}"))?;
    Ok(Some(Prior { commit, text }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::baseline::GitRun;

    fn answer(ok: bool, stdout: &str) -> Option<GitRun> {
        Some(GitRun {
            ok,
            stdout: stdout.into(),
        })
    }

    /// A git that answers by the first two arguments.
    fn fake(
        shallow: Option<GitRun>,
        head: bool,
        log: Option<&'static str>,
    ) -> impl Fn(&[&str]) -> Option<GitRun> {
        let shallow = shallow.map(|r| (r.ok, r.stdout));
        move |args: &[&str]| match (args[0], args[1]) {
            ("rev-parse", "--is-shallow-repository") => {
                shallow.as_ref().and_then(|(ok, out)| answer(*ok, out))
            }
            ("rev-parse", "--verify") => answer(head, ""),
            ("rev-list", _) => match log {
                Some(text) => answer(true, text),
                None => answer(false, ""),
            },
            other => panic!("unexpected {other:?}"),
        }
    }

    fn why(git: Git<'_>) -> String {
        may_write(git, "b.json", false).expect_err("refused")
    }

    #[test]
    fn a_first_baseline_needs_a_full_history_without_the_path() {
        assert_eq!(
            may_write(&fake(answer(true, "false\n"), true, Some("")), "b", false),
            Ok(())
        );
        assert_eq!(
            may_write(&fake(answer(true, "false\n"), false, None), "b", false),
            Ok(())
        );
        assert!(why(&fake(answer(true, "false"), true, Some("abc\n"))).contains("not a reset"));
        assert!(why(&fake(answer(true, "true"), true, Some(""))).contains("shallow"));
        assert!(why(&fake(answer(true, "false"), true, None)).contains("cannot be read"));
        assert!(why(&fake(answer(false, ""), true, Some(""))).contains("not a git repository"));
        assert!(why(&fake(None, true, Some(""))).contains("not a git repository"));
    }

    fn real(dir: &std::path::Path, args: &[&str]) {
        let mut all = vec!["-c", "user.name=n", "-c", "user.email=a@b"];
        all.extend_from_slice(args);
        let ok = baseline::real_git(dir)(&all).is_some_and(|run| run.ok);
        assert!(ok, "git {args:?}");
    }

    fn save(dir: &std::path::Path, files: &[(&str, &str)]) {
        for (name, text) in files {
            std::fs::write(dir.join(name), text).expect("write");
        }
        real(dir, &["add", "-A"]);
        real(dir, &["commit", "-q", "--allow-empty", "-m", "c"]);
    }

    #[test]
    fn only_the_allowed_paths_may_differ_from_head() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        real(root, &["init", "-q", "."]);
        std::fs::create_dir(root.join("sub")).expect("mkdir");
        save(
            root,
            &[
                (".gitignore", "ignored\n"),
                ("sub/a", "1"),
                ("sub/gone", "1"),
            ],
        );
        let flake = root.join("sub");
        let git = baseline::real_git(&flake);
        let check = || tree_is_head(&git, &["b.json", "out/report.json"]);
        std::fs::write(flake.join("ignored"), "x").expect("write");
        std::fs::write(flake.join("b.json"), "x").expect("write");
        std::fs::create_dir(flake.join("out")).expect("mkdir");
        std::fs::write(flake.join("out/report.json"), "x").expect("write");
        assert_eq!(check(), Ok(()));
        std::fs::write(flake.join("a"), "2").expect("write");
        assert!(
            check()
                .unwrap_err()
                .starts_with("1 path(s) differ from HEAD (sub/a)")
        );
        real(root, &["add", "sub/a"]);
        assert!(check().unwrap_err().contains("sub/a"), "staged");
        real(root, &["reset", "-q", "--hard"]);
        std::fs::remove_file(flake.join("gone")).expect("remove");
        assert!(check().unwrap_err().contains("sub/gone"), "deleted");
        real(root, &["reset", "-q", "--hard"]);
        std::fs::write(root.join("outside"), "x").expect("write");
        assert!(
            check().unwrap_err().contains("outside"),
            "untracked above the flake"
        );
        for n in 0..6 {
            std::fs::write(flake.join(format!("n{n}")), "x").expect("write");
        }
        let why = check().unwrap_err();
        assert!(why.starts_with("7 path(s)") && !why.contains("n5"), "{why}");
        assert!(
            tree_is_head(&|_: &[&str]| None, &[])
                .unwrap_err()
                .contains("cannot be read")
        );
        let no_status = |args: &[&str]| answer(args[0] == "rev-parse", "");
        assert!(
            tree_is_head(&no_status, &[])
                .unwrap_err()
                .contains("cannot be read")
        );
    }

    #[test]
    fn the_prior_version_is_the_last_one_before_the_current_even_across_a_deletion() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        real(root, &["init", "-q", "."]);
        let git = baseline::real_git(root);
        save(root, &[("b", "one")]);
        assert_eq!(prior(&git, "b"), Ok(None));
        let first = commits(&git, "b").remove(0);
        save(root, &[("other", "x")]);
        save(root, &[("b", "two")]);
        let got = prior(&git, "b").expect("reads").expect("a prior");
        assert_eq!(
            (got.commit.as_str(), got.text.as_str()),
            (first.as_str(), "one")
        );
        real(root, &["rm", "-q", "b"]);
        save(root, &[]);
        save(root, &[("b", "three")]);
        assert_eq!(
            prior(&git, "b").expect("reads").expect("a prior").text,
            "two"
        );
        let broken = |args: &[&str]| match args[0] {
            "log" => answer(true, "c2\nc1\n"),
            "cat-file" => answer(true, ""),
            _ => answer(false, ""),
        };
        assert!(
            prior(&broken, "b")
                .unwrap_err()
                .contains("cannot read b at c1")
        );
    }

    #[test]
    fn an_existing_baseline_must_be_tracked_and_clean() {
        let git = |inside: bool, tracked: bool, status: &'static str| {
            move |args: &[&str]| match args[0] {
                "rev-parse" => answer(inside, "true"),
                "ls-files" => answer(tracked, ""),
                _ => answer(true, status),
            }
        };
        let run = |g: Git<'_>| may_write(g, "b.json", true);
        assert_eq!(run(&git(true, true, "")), Ok(()));
        assert!(
            run(&git(true, true, " M b.json"))
                .unwrap_err()
                .contains("differs")
        );
        assert!(
            run(&git(true, false, ""))
                .unwrap_err()
                .contains("not tracked")
        );
        assert!(
            run(&git(false, true, ""))
                .unwrap_err()
                .contains("not a git")
        );
        assert!(
            run(&|_: &[&str]| None)
                .unwrap_err()
                .contains("cannot be run")
        );
    }
}
