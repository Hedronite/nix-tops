//! The programs the inventory runs, and how it runs them: the kit's pinned CppNix (the judge) and
//! `nix-eval-jobs`, both with gate 3's options, each run with an empty evaluation cache so an
//! earlier failure is never replayed from it.

use std::{
    fs, io,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU32, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

/// What a program wrote and whether it succeeded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
}

/// The pinned programs. `nixpkgs` is the kit's locked nixpkgs as a flake reference.
#[derive(Clone, Debug)]
pub struct Tools {
    pub judge: PathBuf,
    pub eval_jobs: PathBuf,
    pub nixpkgs: String,
}

impl Tools {
    /// The tools named by `judge`, `eval_jobs` and `nixpkgs`, or `None` when any is empty (a
    /// build outside the kit's flake has none of them).
    pub fn from_parts(judge: &str, eval_jobs: &str, nixpkgs: &str) -> Option<Self> {
        let complete = ![judge, eval_jobs, nixpkgs]
            .iter()
            .any(|value| value.is_empty());
        complete.then(|| Self {
            judge: PathBuf::from(judge),
            eval_jobs: PathBuf::from(eval_jobs),
            nixpkgs: nixpkgs.to_string(),
        })
    }
}

/// Gate 3's evaluation options (protocol section 3.1): warnings fatal (unless a collect re-run asks
/// for them to be listed), no import from derivation.
fn eval_options(abort_on_warn: bool) -> Vec<String> {
    let abort = if abort_on_warn { "true" } else { "false" };
    let options = [
        "--option",
        "abort-on-warn",
        abort,
        "--option",
        "allow-import-from-derivation",
        "false",
    ];
    options.iter().map(|option| option.to_string()).collect()
}

/// The Determinate-only settings the gate turns off (protocol section 2).
const DETERMINATE_OPTIONS: [&str; 6] = [
    "--option",
    "lazy-trees",
    "false",
    "--option",
    "eval-cores",
    "1",
];

/// How per-derivation outputs are listed: with `nix-eval-jobs` (the kit's lock), or one `nix eval`
/// per attribute with the judge (used when the judge is the host's Nix, which `nix-eval-jobs`
/// does not link).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Jobs,
    PerAttribute,
}

/// A run with its own cache directory and scratch space, removed when it is dropped.
pub struct Session {
    tools: Tools,
    dir: PathBuf,
    abort_on_warn: bool,
    backend: Backend,
    determinate: bool,
}

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn unique_dir() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let serial = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("nixtops-{}-{nanos}-{serial}", std::process::id()))
}

fn run_program(program: &Path, args: &[String], cache: &Path) -> Run {
    let output = crate::exec::output(
        Command::new(program)
            .args(args)
            .env("XDG_CACHE_HOME", cache),
    );
    match output {
        Ok(out) => Run {
            ok: out.status.success(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        },
        Err(err) => Run {
            ok: false,
            stdout: String::new(),
            stderr: format!("cannot run {}: {err}", program.display()),
        },
    }
}

impl Session {
    pub fn new(tools: Tools) -> io::Result<Self> {
        let dir = unique_dir();
        fs::create_dir_all(dir.join("cache"))?;
        Ok(Self {
            tools,
            dir,
            abort_on_warn: true,
            backend: Backend::Jobs,
            determinate: false,
        })
    }

    /// Whether a warning is fatal in the runs that follow. True except in a collect re-run.
    pub fn set_abort_on_warn(&mut self, abort_on_warn: bool) {
        self.abort_on_warn = abort_on_warn;
    }

    /// Turns the Determinate-only settings off in the runs that follow (`lazy-trees false`,
    /// `eval-cores 1`, protocol section 2). For a judge that is the host's Determinate Nix.
    pub fn set_determinate(&mut self, determinate: bool) {
        self.determinate = determinate;
    }

    pub fn set_backend(&mut self, backend: Backend) {
        self.backend = backend;
    }

    pub fn backend(&self) -> Backend {
        self.backend
    }

    pub fn nixpkgs(&self) -> &str {
        &self.tools.nixpkgs
    }

    /// A directory for this run's scratch files (template copies).
    pub fn scratch(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    /// `nix <subcommand> <rest> <gate 3 options>` with the judge.
    pub fn judge(&self, subcommand: &[&str], rest: &[&str]) -> Run {
        let mut args = vec![
            "--extra-experimental-features".to_string(),
            "nix-command flakes".to_string(),
        ];
        args.extend(subcommand.iter().chain(rest).map(|a| a.to_string()));
        args.push("--no-accept-flake-config".to_string());
        args.extend(eval_options(self.abort_on_warn));
        if self.determinate {
            args.extend(DETERMINATE_OPTIONS.iter().map(|a| a.to_string()));
        }
        run_program(&self.tools.judge, &args, &self.dir.join("cache"))
    }

    /// `nix-eval-jobs --flake <target> --meta <gate 3 options>`.
    pub fn eval_jobs(&self, target: &str) -> Run {
        // nix-eval-jobs takes no --extra-experimental-features; the same setting goes by --option.
        let features = [
            "--option",
            "extra-experimental-features",
            "nix-command flakes",
        ];
        let mut args: Vec<String> = ["--flake", target, "--meta", "--workers", "1"]
            .iter()
            .map(|a| a.to_string())
            .collect();
        args.extend(features.iter().map(|a| a.to_string()));
        args.extend(eval_options(self.abort_on_warn));
        run_program(&self.tools.eval_jobs, &args, &self.dir.join("cache"))
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_need_all_three_parts() {
        let t = Tools::from_parts("/j", "/e", "ref").unwrap();
        assert_eq!(
            (t.judge, t.eval_jobs, t.nixpkgs),
            (PathBuf::from("/j"), PathBuf::from("/e"), "ref".to_string())
        );
        assert!(Tools::from_parts("", "/e", "ref").is_none());
        assert!(Tools::from_parts("/j", "", "ref").is_none());
        assert!(Tools::from_parts("/j", "/e", "").is_none());
    }

    #[test]
    fn unique_dirs_differ() {
        assert_ne!(unique_dir(), unique_dir());
    }

    #[test]
    fn a_missing_program_is_a_failed_run_that_says_so() {
        let run = run_program(Path::new("/nonexistent/prog"), &[], Path::new("/tmp"));
        assert!(!run.ok && run.stdout.is_empty());
        assert!(run.stderr.starts_with("cannot run /nonexistent/prog"));
    }

    #[test]
    fn a_program_runs_with_the_cache_in_its_environment() {
        let args = [
            "-c".to_string(),
            "echo $XDG_CACHE_HOME; echo err >&2; exit 0".to_string(),
        ];
        let run = run_program(Path::new("/bin/sh"), &args, Path::new("/cache/x"));
        assert!(run.ok);
        assert_eq!(
            (run.stdout.as_str(), run.stderr.as_str()),
            ("/cache/x\n", "err\n")
        );
        let fail = run_program(
            Path::new("/bin/sh"),
            &["-c".to_string(), "exit 3".to_string()],
            Path::new("/c"),
        );
        assert!(!fail.ok);
    }

    #[test]
    fn a_session_cleans_up_its_directory() {
        let tools = Tools {
            judge: "j".into(),
            eval_jobs: "e".into(),
            nixpkgs: "n".into(),
        };
        let session = Session::new(tools).unwrap();
        let dir = session.dir.clone();
        assert!(dir.join("cache").is_dir());
        assert_eq!(session.nixpkgs(), "n");
        assert!(session.scratch("t").starts_with(&dir));
        drop(session);
        assert!(!dir.exists());
    }
}
