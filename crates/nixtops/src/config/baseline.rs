//! Under `ratchet` the baseline is tracked and clean against `HEAD` (section 6.8). Git is
//! reached through a function so the states can be tested without a repository.

use super::{Outcome, Problem};
use std::{path::Path, process::Command};

/// The result of one git run: whether it succeeded and what it printed.
pub struct GitRun {
    pub ok: bool,
    pub stdout: String,
}

/// Runs `git <args>`; `None` when git cannot be run at all.
pub type Git<'a> = &'a dyn Fn(&[&str]) -> Option<GitRun>;

/// `git -C dir <args>`.
pub fn real_git(dir: &Path) -> impl Fn(&[&str]) -> Option<GitRun> {
    let dir = dir.to_path_buf();
    move |args: &[&str]| {
        let out = crate::exec::output(Command::new("git").arg("-C").arg(&dir).args(args)).ok()?;
        Some(GitRun {
            ok: out.status.success(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum State {
    Clean,
    NoGit,
    NotRepo,
    Untracked,
    Dirty,
}

pub fn state(git: Git<'_>, path: &str) -> State {
    let Some(inside) = git(&["rev-parse", "--is-inside-work-tree"]) else {
        return State::NoGit;
    };
    if !inside.ok {
        return State::NotRepo;
    }
    match git(&["ls-files", "--error-unmatch", "--", path]) {
        Some(tracked) if tracked.ok => {}
        _ => return State::Untracked,
    }
    match git(&["status", "--porcelain", "--", path]) {
        Some(status) if status.stdout.trim().is_empty() && status.ok => State::Clean,
        _ => State::Dirty,
    }
}

/// The failure for a state, if any: git missing is an environment failure (exit 3), the rest
/// are config failures (exit 2).
pub fn findings(git: Git<'_>, path: &str) -> Outcome {
    let mut out = Outcome::default();
    let problem = |message: String| Problem::new("baseline".into(), message);
    match state(git, path) {
        State::Clean => out
            .notes
            .push(format!("baseline {path}: tracked and clean")),
        State::NoGit => out.environment.push(problem(
            "git is not available to check that the baseline is clean".into(),
        )),
        State::NotRepo => out.problems.push(problem(format!(
            "{path} is not in a git repository; the baseline must be tracked and clean"
        ))),
        State::Untracked => out.problems.push(problem(format!(
            "{path} is not tracked by git; commit it (section 6.8)"
        ))),
        State::Dirty => out.problems.push(problem(format!(
            "{path} differs from HEAD; commit or restore it (section 6.8)"
        ))),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(ok: bool, stdout: &str) -> Option<GitRun> {
        Some(GitRun {
            ok,
            stdout: stdout.into(),
        })
    }

    /// A fake git: each answer is for the call of that name.
    fn fake(
        inside: Option<bool>,
        tracked: bool,
        status: &'static str,
    ) -> impl Fn(&[&str]) -> Option<GitRun> {
        move |args: &[&str]| match args[0] {
            "rev-parse" => inside.and_then(|ok| run(ok, "true\n")),
            "ls-files" => run(tracked, ""),
            _ => run(true, status),
        }
    }

    #[test]
    fn every_state_is_reached() {
        assert_eq!(state(&fake(None, true, ""), "b"), State::NoGit);
        assert_eq!(state(&fake(Some(false), true, ""), "b"), State::NotRepo);
        assert_eq!(state(&fake(Some(true), false, ""), "b"), State::Untracked);
        assert_eq!(state(&fake(Some(true), true, " M b\n"), "b"), State::Dirty);
        assert_eq!(state(&fake(Some(true), true, "\n"), "b"), State::Clean);
    }

    #[test]
    fn a_failed_status_is_not_clean() {
        let git = |args: &[&str]| match args[0] {
            "status" => run(false, ""),
            _ => run(true, ""),
        };
        assert_eq!(state(&git, "b"), State::Dirty);
    }

    #[test]
    fn only_a_missing_git_is_an_environment_failure() {
        let f = findings(&fake(None, true, ""), "b");
        assert_eq!((f.problems.len(), f.environment.len()), (0, 1));
        for git in [
            fake(Some(false), true, ""),
            fake(Some(true), false, ""),
            fake(Some(true), true, "M"),
        ] {
            let f = findings(&git, "b");
            assert_eq!((f.problems.len(), f.environment.len()), (1, 0));
            assert_eq!(f.problems[0].key, "baseline");
        }
        let clean = findings(&fake(Some(true), true, ""), "b");
        assert_eq!(
            (
                clean.problems.len(),
                clean.environment.len(),
                clean.notes.len()
            ),
            (0, 0, 1)
        );
    }

    #[test]
    fn the_real_git_reports_a_directory_that_is_not_a_repository() {
        let dir = tempfile::tempdir().expect("tempdir");
        let git = real_git(dir.path());
        match git(&["rev-parse", "--is-inside-work-tree"]) {
            None => {} // no git on this host: the NoGit path
            Some(run) => assert!(!run.ok || !run.stdout.is_empty()),
        }
    }
}
