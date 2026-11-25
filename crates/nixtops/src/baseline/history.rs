//! What git says before `nixtops baseline` may write. An existing baseline must be the committed
//! one, and a missing one must never have been committed: deleting it is not a reset.

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
