//! `ratchet`, `advisory` and `nixtops baseline` through the real runner, git and files, with
//! stand-ins for Nix: the multi-step stories a single-run fixture cannot tell.

mod common;

use nixtops::{
    baseline::{file, sha256},
    cmd,
    exit::{Exit, Output},
    gate::{self, Evaluator, Seams, Settings},
    inventory::Tools,
    report::render_text,
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const TOOLS: [&str; 4] = ["deadnix", "gitleaks", "nixfmt", "statix"];
const BASELINE: &str = ".nix-tops/baseline.json";

const CONFIG: &str = "protocol = \"0.2.0\"\nfacets = [\"package-set\"]\nenforce = \"ratchet\"\n\n[toolchain]\nsystems = [\"x86_64-linux\"]\n\n[layers]\nbuild-test = { enabled = false, reason = \"not built yet\" }\ncoverage = { enabled = false, reason = \"not built yet\" }\nfacet-layers = { enabled = false, reason = \"not built yet\" }\nlaws = { enabled = false, reason = \"not built yet\" }\n";

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap()
        .status
        .success();
    assert!(ok, "git {args:?}");
}

/// The judge and the evaluator. The judge prints `build.json` for the findings build.
fn write_stand_ins(dir: &Path) {
    let names: Vec<String> = TOOLS
        .iter()
        .map(|t| format!("nixtops-{t}-findings"))
        .collect();
    let names = serde_json::to_string(&names).unwrap();
    let arms = [
        ("flake check", "ok"),
        ("hash file", "sha256-LOCK"),
        ("config show system", "x86_64-linux"),
        (
            "flake'*'metadata'*'--no-update-lock-file",
            r#"{"url":"fake:locked"}"#,
        ),
        (r##"#. --apply"##, r#"["packages"]"#),
        (r##"#."packages" --apply"##, r#"["x86_64-linux"]"#),
        (r##"#."checks" --apply"##, r#"["x86_64-linux"]"#),
        (r##"#."checks"."x86_64-linux" --apply"##, names.as_str()),
    ];
    let mut script = String::from("#!/bin/sh\ncase \"$*\" in\n");
    for (pattern, out) in arms {
        script.push_str(&format!(
            "*'{pattern}'*)\ncat <<'__OUT__'\n{out}\n__OUT__\nexit 0;;\n"
        ));
    }
    let build = dir.join("build.json");
    script.push_str(&format!(
        "*'build'*'--no-link'*) cat '{}'; exit 0;;\n",
        build.display()
    ));
    script.push_str("*) echo \"no arm for: $*\" >&2; exit 99;;\nesac\n");
    common::write_executable(&dir.join("judge"), &script);
    common::write_executable(
        &dir.join("nej"),
        "#!/bin/sh\necho '{\"attr\":\"default\",\"attrPath\":[\"default\"],\"drvPath\":\"/nix/store/d.drv\",\"name\":\"p-1\"}'\n",
    );
}

struct Rig {
    dir: tempfile::TempDir,
    stand_ins: tempfile::TempDir,
    nix: PathBuf,
}

impl Rig {
    fn new(config: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        git(root, &["init", "-q", "."]);
        git(root, &["config", "user.email", "a@b"]);
        git(root, &["config", "user.name", "n"]);
        fs::write(root.join("flake.nix"), "{ outputs = _: { }; }\n").unwrap();
        fs::write(root.join("flake.lock"), "{}\n").unwrap();
        fs::write(root.join(".gitignore"), ".nix-tops/report.json\n").unwrap();
        fs::write(root.join("nix-tops.toml"), config).unwrap();
        let stand_ins = tempfile::tempdir().unwrap();
        let nix = stand_ins.path().join("nix");
        common::write_executable(&nix, "#!/bin/sh\necho 'nix (Nix) 2.35.2'\n");
        write_stand_ins(stand_ins.path());
        let rig = Self {
            dir,
            stand_ins,
            nix,
        };
        rig.commit("init");
        rig
    }

    fn root(&self) -> &Path {
        self.dir.path()
    }

    fn write(&self, name: &str, text: &str) {
        fs::write(self.root().join(name), text).unwrap();
    }

    fn commit(&self, message: &str) {
        git(self.root(), &["add", "-A"]);
        git(
            self.root(),
            &["commit", "-q", "--allow-empty", "-m", message],
        );
    }

    /// The hash `mkChecks` would take of the files as they are now, with no `exclude`.
    fn hash(&self, tool: &str) -> String {
        let read = |name: &str| fs::read_to_string(self.root().join(name)).ok();
        let body = match tool {
            "deadnix" => "fail=true".to_string(),
            "gitleaks" => "default|".to_string(),
            "nixfmt" => "default-nixfmt".to_string(),
            _ => read("statix.toml").unwrap_or_else(|| "default".into()),
        };
        sha256::hex(format!("{body}\nexclude:").as_bytes())
    }

    /// A judge whose findings are `deadnix` findings in `flake.nix`.
    fn tools(&self, deadnix: usize) -> Tools {
        self.tools_finding(&[("deadnix", "flake.nix", deadnix)])
    }

    /// A judge with these findings: tool, file and count. Only the data it prints changes; the
    /// executables are the ones written with the rig.
    fn tools_finding(&self, found: &[(&str, &str, usize)]) -> Tools {
        let mut outputs = Vec::new();
        for tool in TOOLS {
            let mut files = serde_json::Map::new();
            for (_, file, count) in found.iter().filter(|(t, _, n)| *t == tool && *n > 0) {
                let items: Vec<usize> = (0..*count).collect();
                files.insert((*file).to_string(), items.into());
            }
            let payload = serde_json::json!({
                "tool": tool, "version": "1", "config_hash": self.hash(tool), "files": files,
            });
            let path = self.stand_ins.path().join(format!("{tool}.json"));
            fs::write(&path, payload.to_string()).unwrap();
            outputs.push(serde_json::json!({"outputs": {"out": path}}));
        }
        let build = serde_json::Value::Array(outputs).to_string();
        fs::write(self.stand_ins.path().join("build.json"), build).unwrap();
        Tools {
            judge: self.stand_ins.path().join("judge"),
            eval_jobs: self.stand_ins.path().join("nej"),
            nixpkgs: "github:NixOS/nixpkgs/pin".into(),
        }
    }

    fn settings(&self, writing_baseline: bool) -> Settings {
        Settings {
            root: self.root().to_path_buf(),
            keep_going: false,
            only: None,
            system: None,
            evaluator: Evaluator::Pinned,
            ci: false,
            today: "2026-11-25".into(),
            nix: self.nix.display().to_string(),
            writing_baseline,
        }
    }

    /// The exit code of `nixtops gate` and everything it would say.
    fn gate(&self, deadnix: usize) -> (Exit, String) {
        let got = gate::run(
            &self.settings(false),
            Some(self.tools(deadnix)),
            &Seams::default(),
        );
        let report = serde_json::to_value(&got.report).unwrap();
        let text = format!("{}{}", render_text(&report), got.problems.join("\n"));
        (got.exit, text)
    }

    fn baseline(&self, deadnix: usize) -> Output {
        cmd::baseline::run_with(
            &self.settings(true),
            Some(self.tools(deadnix)),
            &Seams::default(),
        )
    }

    fn recorded(&self) -> Option<usize> {
        file::load(&self.root().join(BASELINE))
            .unwrap()
            .map(|b| b.total())
    }

    /// A committed baseline over `deadnix` findings.
    fn based(deadnix: usize) -> Self {
        let rig = Self::new(CONFIG);
        let out = rig.baseline(deadnix);
        assert_eq!(out.exit, Exit::Pass, "{}{}", out.stdout, out.stderr);
        rig.commit("baseline");
        rig
    }
}

#[test]
fn the_roadmap_exit() {
    let rig = Rig::new(&CONFIG.replace("ratchet", "hard"));
    let (exit, text) = rig.gate(1);
    assert_eq!(exit, Exit::GateFailed, "{text}");
    assert!(text.contains("deadnix: flake.nix (1)"), "{text}");

    rig.write("nix-tops.toml", CONFIG);
    rig.commit("ratchet");
    let (exit, text) = rig.gate(1);
    assert_eq!(exit, Exit::ConfigInvalid, "{text}");
    assert!(text.contains("does not exist"), "{text}");

    let out = rig.baseline(1);
    assert_eq!(out.exit, Exit::Pass, "{}{}", out.stdout, out.stderr);
    assert!(
        out.stdout
            .contains("1 finding(s) from 4 lint tool(s), 0 ack(s)"),
        "{}",
        out.stdout
    );
    assert_eq!(rig.recorded(), Some(1));
    let (exit, text) = rig.gate(1);
    assert_eq!(exit, Exit::ConfigInvalid, "uncommitted: {text}");
    assert!(text.contains("not tracked"), "{text}");

    rig.commit("baseline");
    let (exit, text) = rig.gate(1);
    assert_eq!(exit, Exit::Pass, "{text}");

    let (exit, text) = rig.gate(2);
    assert_eq!(exit, Exit::GateFailed, "{text}");
    assert!(
        text.contains("deadnix: flake.nix: 2 finding(s), baseline 1"),
        "{text}"
    );
    let out = rig.baseline(2);
    assert_eq!(out.exit, Exit::GateFailed, "{}", out.stderr);
    assert_eq!(rig.recorded(), Some(1));

    // Fewer findings pass, and the next baseline locks the lower count in.
    assert_eq!(rig.gate(0).0, Exit::Pass);
    assert_eq!(rig.baseline(0).exit, Exit::Pass);
    assert_eq!(rig.recorded(), Some(0));
}

#[test]
fn a_deleted_baseline_fails_the_gate_and_cannot_be_written_again() {
    let rig = Rig::based(0);
    fs::remove_file(rig.root().join(BASELINE)).unwrap();
    for stage in ["uncommitted", "committed"] {
        let (exit, text) = rig.gate(0);
        assert_eq!(exit, Exit::ConfigInvalid, "{stage}: {text}");
        let out = rig.baseline(0);
        assert_eq!(out.exit, Exit::Refused, "{stage}");
        assert!(out.stderr.contains("not a reset"), "{}", out.stderr);
        assert_eq!(rig.recorded(), None, "{stage}");
        rig.commit("delete");
    }
}

#[test]
fn a_hand_edited_baseline_fails_the_gate_and_is_not_overwritten() {
    let rig = Rig::based(0);
    let path = rig.root().join(BASELINE);
    let edited = fs::read_to_string(&path)
        .unwrap()
        .replace("\"files\": {}", "\"files\": {\"flake.nix\": 9}");
    fs::write(&path, &edited).unwrap();
    let (exit, text) = rig.gate(1);
    assert_eq!(exit, Exit::ConfigInvalid, "{text}");
    let out = rig.baseline(1);
    assert_eq!(out.exit, Exit::Refused, "{}", out.stderr);
    assert!(out.stderr.contains("differs from HEAD"), "{}", out.stderr);
    assert_eq!(fs::read_to_string(&path).unwrap(), edited);

    // The same edit in a mode that never asks git is still not laundered by `baseline`.
    rig.write("nix-tops.toml", &CONFIG.replace("ratchet", "hard"));
    assert_eq!(rig.baseline(0).exit, Exit::Refused);

    // Committed but not a baseline: the gate says so.
    rig.write("nix-tops.toml", CONFIG);
    fs::write(&path, "{}\n").unwrap();
    rig.commit("broken");
    let (exit, text) = rig.gate(0);
    assert_eq!(exit, Exit::GateFailed, "{text}");
    assert!(text.contains("not a valid baseline"), "{text}");
    let out = rig.baseline(0);
    assert_eq!(out.exit, Exit::GateFailed);
    assert!(
        out.stderr.contains("not a valid baseline"),
        "{}",
        out.stderr
    );
}

#[test]
fn a_narrower_statix_config_fails_and_a_wider_one_passes_and_asks_for_a_baseline() {
    let rig = Rig::based(0);
    rig.write("statix.toml", "disabled = [\"unquoted_uri\"]\n");
    rig.commit("narrower");
    let (exit, text) = rig.gate(0);
    assert_eq!(exit, Exit::GateFailed, "{text}");
    assert!(text.contains("statix: the config is narrower"), "{text}");
    let out = rig.baseline(0);
    assert_eq!(out.exit, Exit::GateFailed, "{}", out.stderr);

    let narrow = Rig::new(CONFIG);
    narrow.write("statix.toml", "disabled = [\"unquoted_uri\"]\n");
    narrow.commit("narrow from the start");
    assert_eq!(narrow.baseline(0).exit, Exit::Pass);
    narrow.commit("baseline");
    narrow.write("statix.toml", "disabled = []\n");
    narrow.commit("wider");
    let (exit, text) = narrow.gate(0);
    assert_eq!(exit, Exit::Pass, "{text}");
    assert!(
        text.contains("re-baseline: statix: the config is wider than the baseline's"),
        "{text}"
    );
    assert_eq!(narrow.baseline(0).exit, Exit::Pass);
}

#[test]
fn baseline_writes_nothing_after_a_failing_run() {
    let rig = Rig::new(&CONFIG.replace("ratchet", "hard"));
    let out = rig.baseline(1);
    assert_eq!(out.exit, Exit::GateFailed);
    assert!(
        out.stderr.contains("the gate did not pass"),
        "{}",
        out.stderr
    );
    assert!(
        out.stdout.contains("deadnix: flake.nix (1)"),
        "{}",
        out.stdout
    );
    assert_eq!(rig.recorded(), None);

    // A config that does not load keeps its own exit code.
    rig.write("nix-tops.toml", "protocol = ");
    let out = rig.baseline(0);
    assert_eq!(out.exit, Exit::ConfigInvalid);
    assert!(out.stderr.contains("(file)"), "{}", out.stderr);
    assert_eq!(rig.recorded(), None);
}

#[test]
fn advisory_runs_exit_0_say_they_are_not_enforced_and_cannot_loosen_the_baseline() {
    let advisory = |until: &str| {
        format!(
            "{}\n[advisory]\nreason = \"adopting\"\nuntil = \"{until}\"\n",
            CONFIG.replace("ratchet", "advisory")
        )
    };
    let rig = Rig::based(0);
    rig.write("nix-tops.toml", &advisory("2026-11-25"));
    rig.commit("advisory");
    let (exit, text) = rig.gate(3);
    assert_eq!(exit, Exit::Pass, "{text}");
    assert!(text.contains("run is not enforced"), "{text}");
    let out = rig.baseline(3);
    assert_eq!(out.exit, Exit::GateFailed, "{}", out.stdout);
    assert!(
        out.stderr.contains("looser than the baseline"),
        "{}",
        out.stderr
    );
    assert_eq!(rig.recorded(), Some(0));

    rig.write("nix-tops.toml", &advisory("2026-11-24"));
    let (exit, text) = rig.gate(0);
    assert_eq!(exit, Exit::ConfigInvalid, "{text}");
    assert!(text.contains("advisory.until"), "{text}");
}

#[test]
fn a_shallow_clone_or_no_repository_cannot_write_a_first_baseline() {
    let rig = Rig::new(CONFIG);
    rig.commit("second");
    let clone = tempfile::tempdir().unwrap();
    let from = format!("file://{}", rig.root().display());
    git(
        clone.path(),
        &["clone", "-q", "--depth", "1", &from, "shallow"],
    );
    let shallow = Settings {
        root: clone.path().join("shallow"),
        ..rig.settings(true)
    };
    let out = cmd::baseline::run_with(&shallow, Some(rig.tools(0)), &Seams::default());
    assert_eq!(out.exit, Exit::Refused, "{}", out.stderr);
    assert!(out.stderr.contains("shallow"), "{}", out.stderr);

    fs::remove_dir_all(rig.root().join(".git")).unwrap();
    let out = rig.baseline(0);
    assert_eq!(out.exit, Exit::Refused);
    assert!(
        out.stderr.contains("not a git repository"),
        "{}",
        out.stderr
    );
}

/// A rig whose baseline was taken with a lint disabled and one deadnix finding.
fn narrow_rig() -> Rig {
    let rig = Rig::new(CONFIG);
    rig.write("statix.toml", "disabled = [\"empty_pattern\"]\n");
    rig.write("old.nix", "{ }\n");
    rig.write("edited.nix", "{ }\n");
    rig.commit("narrow from the start");
    assert_eq!(rig.baseline(0).exit, Exit::Pass);
    rig.commit("baseline");
    rig.write("statix.toml", "disabled = []\n");
    rig
}

impl Rig {
    fn gate_finding(&self, found: &[(&str, &str, usize)]) -> (Exit, String) {
        let got = gate::run(
            &self.settings(false),
            Some(self.tools_finding(found)),
            &Seams::default(),
        );
        let report = serde_json::to_value(&got.report).unwrap();
        let text = format!("{}{}", render_text(&report), got.problems.join("\n"));
        (got.exit, text)
    }

    fn baseline_finding(&self, found: &[(&str, &str, usize)]) -> Output {
        cmd::baseline::run_with(
            &self.settings(true),
            Some(self.tools_finding(found)),
            &Seams::default(),
        )
    }
}

#[test]
fn a_wider_config_without_a_rise_passes_and_the_baseline_is_written_again() {
    let rig = narrow_rig();
    rig.commit("widen");
    let (exit, text) = rig.gate_finding(&[]);
    assert_eq!(exit, Exit::Pass, "{text}");
    assert!(
        text.contains("re-baseline: statix: the config is wider than the baseline's"),
        "{text}"
    );
    let before = fs::read_to_string(rig.root().join(BASELINE)).unwrap();
    let out = rig.baseline_finding(&[]);
    assert_eq!(out.exit, Exit::Pass, "{}", out.stderr);
    assert_ne!(
        fs::read_to_string(rig.root().join(BASELINE)).unwrap(),
        before
    );
    rig.commit("re-baseline");
    let (exit, text) = rig.gate_finding(&[]);
    assert_eq!(exit, Exit::Pass, "{text}");
    assert!(!text.contains("re-baseline:"), "{text}");
}

/// No commit that touches the baseline makes a later rise acceptable: not a reformat, a mode
/// change, a count lowered by hand, or a file carried along with it.
#[test]
fn a_wider_config_excuses_no_rise_whatever_the_history() {
    use std::os::unix::fs::PermissionsExt;
    let rig = narrow_rig();
    git(rig.root(), &["checkout", "-q", "--", "statix.toml"]);
    let path = rig.root().join(BASELINE);
    let text = fs::read_to_string(&path).unwrap();
    rig.write("s1.nix", "{ }\n");
    fs::write(&path, text.replace("  ", "    ")).unwrap();
    rig.commit("reformat, carrying a file");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    rig.commit("mode");
    rig.write("statix.toml", "disabled = []\n");
    rig.commit("widen");
    for file in ["old.nix", "s1.nix", "new.nix"] {
        let found = [("statix", file, 1)];
        let (exit, text) = rig.gate_finding(&found);
        assert_eq!(exit, Exit::GateFailed, "{file}: {text}");
        let named = format!("statix: {file}: 1 finding(s), baseline 0");
        assert!(text.contains(&named), "{text}");
        let out = rig.baseline_finding(&found);
        assert_eq!(out.exit, Exit::GateFailed, "{file}: {}", out.stderr);
        assert_eq!(rig.recorded(), Some(0), "{file}");
    }
}

#[test]
fn a_tool_added_since_the_baseline_starts_at_zero() {
    let off = CONFIG.replace(
        "[layers]\n",
        "[layers]\nlint = { enabled = false, reason = \"later\" }\n",
    );
    let rig = Rig::new(&off);
    assert_eq!(rig.baseline(0).exit, Exit::Pass);
    rig.commit("baseline without lint");
    rig.write("nix-tops.toml", CONFIG);
    rig.commit("lint on");
    let (exit, text) = rig.gate(0);
    assert_eq!(exit, Exit::Pass, "{text}");
    assert!(text.contains("deadnix: not in the baseline"), "{text}");
    let (exit, text) = rig.gate(1);
    assert_eq!(exit, Exit::GateFailed, "{text}");
    assert!(
        text.contains("deadnix: flake.nix: 1 finding(s), baseline 0"),
        "{text}"
    );
    assert_eq!(rig.baseline(1).exit, Exit::GateFailed);
    assert_eq!(rig.baseline(0).exit, Exit::Pass);
}

#[test]
fn a_committed_hand_edit_that_raises_a_count_fails_the_gate() {
    let rig = Rig::based(1);
    let path = rig.root().join(BASELINE);
    let edit = |from: &str, to: &str| {
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains(from), "{text}");
        fs::write(&path, text.replace(from, to)).unwrap();
    };
    edit("\"flake.nix\": 1", "\"flake.nix\": 5");
    rig.commit("hand edit");
    let (exit, text) = rig.gate(3);
    assert_eq!(exit, Exit::GateFailed, "{text}");
    assert!(text.contains("is looser than its version at"), "{text}");
    assert!(
        text.contains("deadnix: flake.nix: 5 finding(s), baseline 1"),
        "{text}"
    );
    let out = rig.baseline(3);
    assert_eq!(out.exit, Exit::GateFailed, "{}", out.stderr);
    assert_eq!(rig.recorded(), Some(5), "nothing is written");

    // A committed lower count is only stricter.
    edit("\"flake.nix\": 5", "\"flake.nix\": 1");
    rig.commit("back");
    edit("\"flake.nix\": 1", "\"flake.nix\": 0");
    assert!(fs::read_to_string(&path).unwrap().contains(": 0"));
    edit("\"flake.nix\": 0", "");
    rig.commit("lowered by hand");
    let (exit, text) = rig.gate(0);
    assert_eq!(exit, Exit::Pass, "{text}");
}

#[test]
fn an_unreadable_earlier_baseline_fails_closed() {
    let rig = Rig::based(0);
    let path = rig.root().join(BASELINE);
    let good = fs::read_to_string(&path).unwrap();
    fs::write(&path, "{}\n").unwrap();
    rig.commit("broken");
    fs::write(&path, good).unwrap();
    rig.commit("repaired by hand");
    let (exit, text) = rig.gate(0);
    assert_eq!(exit, Exit::GateFailed, "{text}");
    assert!(text.contains("is not a valid baseline"), "{text}");
}

#[test]
fn a_report_stopped_by_an_invalid_exemption_names_the_stated_mode() {
    let config = format!(
        "{}\n[advisory]\nreason = \"r\"\nuntil = \"2026-11-24\"\n",
        CONFIG.replace("ratchet", "advisory")
    );
    let rig = Rig::new(&config);
    let got = gate::run(&rig.settings(false), Some(rig.tools(0)), &Seams::default());
    assert_eq!(got.exit, Exit::ConfigInvalid);
    assert_eq!(got.report.enforce, "advisory");
}

#[test]
fn baseline_refuses_a_tree_that_differs_from_head() {
    let rig = Rig::new(CONFIG);
    rig.write("s1.nix", "{ }\n");
    rig.commit("a file");
    let hidden = rig.stand_ins.path().join("hidden.nix");
    let expect = |stage: &str| {
        let out = rig.baseline(0);
        assert_eq!(out.exit, Exit::Refused, "{stage}: {}", out.stderr);
        assert!(
            out.stderr.contains("differ from HEAD"),
            "{stage}: {}",
            out.stderr
        );
        assert_eq!(rig.recorded(), None, "{stage}");
    };
    // A file moved out of the way while the baseline is taken.
    fs::rename(rig.root().join("s1.nix"), &hidden).unwrap();
    expect("deleted");
    fs::rename(&hidden, rig.root().join("s1.nix")).unwrap();
    rig.write("s1.nix", "{ x = 1; }\n");
    expect("modified");
    git(rig.root(), &["add", "s1.nix"]);
    expect("staged");
    git(rig.root(), &["reset", "-q", "--hard"]);
    rig.write("notes.txt", "x");
    expect("untracked");
    fs::remove_file(rig.root().join("notes.txt")).unwrap();
    assert_eq!(rig.baseline(0).exit, Exit::Pass);
}

/// Under `advisory` the gate passes whatever is found, so `baseline`'s own comparison is the only
/// guard.
#[test]
fn under_advisory_baseline_still_refuses_a_rise_under_a_wider_config() {
    let rig = narrow_rig();
    let advisory = format!(
        "{}\n[advisory]\nreason = \"adopting\"\nuntil = \"2026-11-25\"\n",
        CONFIG.replace("ratchet", "advisory")
    );
    rig.write("nix-tops.toml", &advisory);
    rig.write("s1.nix", "{ }\n");
    rig.commit("widen under advisory");
    let out = rig.baseline_finding(&[("statix", "s1.nix", 1)]);
    assert_eq!(out.exit, Exit::GateFailed, "{}", out.stdout);
    assert!(
        out.stderr
            .contains("looser than the baseline: statix: s1.nix: 1 finding(s), baseline 0"),
        "{}",
        out.stderr
    );
    assert_eq!(rig.recorded(), Some(0));
    let out = rig.baseline_finding(&[]);
    assert_eq!(out.exit, Exit::Pass, "{}", out.stderr);
}

/// The report every run writes need not be ignored: it is not part of what a baseline is taken
/// from, at its default path or the one the config names.
#[test]
fn an_unignored_report_does_not_stop_baseline() {
    for (section, report) in [
        ("", ".nix-tops/report.json"),
        ("\n[report]\npath = \"out.json\"\n", "out.json"),
    ] {
        let rig = Rig::new(&format!("{CONFIG}{section}"));
        fs::remove_file(rig.root().join(".gitignore")).unwrap();
        rig.commit("nothing ignored");
        // Stopped for want of a baseline: its report goes to the default path.
        rig.gate(0);
        assert!(rig.root().join(".nix-tops/report.json").is_file());
        rig.write(report, "{}\n");
        let out = rig.baseline(0);
        assert_eq!(out.exit, Exit::Pass, "{report}: {}", out.stderr);
    }
}
