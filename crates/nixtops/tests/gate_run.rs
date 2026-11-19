//! The gate run with stand-in tools: a shell script for the judge and one for `nix-eval-jobs`
//! answering from a table, a real `git` repository in a temporary directory, and the real config
//! check. Gates 1 and 3, the stop rule, the ledger, the diagnostics table and the report.

mod common;

use nixtops::{
    exit::Exit,
    gate::{self, Evaluator, Seams, Settings, diag},
    inventory::Tools,
    report::LayerId,
};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

type Arm<'a> = (&'a str, &'a str, &'a str, i32);

fn script(dir: &Path, name: &str, arms: &[Arm]) -> PathBuf {
    let mut text = String::from("#!/bin/sh\ncase \"$*\" in\n");
    for (pattern, stdout, stderr, code) in arms {
        text.push_str(&format!("*'{pattern}'*)\ncat <<'__OUT__'\n{stdout}\n__OUT__\ncat >&2 <<'__ERR__'\n{stderr}\n__ERR__\nexit {code};;\n"));
    }
    text.push_str("*) echo \"no arm for: $*\" >&2; exit 99;;\nesac\n");
    let path = dir.join(name);
    common::write_executable(&path, &text);
    path
}

/// A valid config with every layer that is not built in this version switched off, as the kit's
/// own config does.
const CONFIG: &str = "protocol = \"0.2.0\"\nfacets = [\"package-set\"]\nenforce = \"hard\"\n\n[toolchain]\nsystems = [\"x86_64-linux\"]\n\n[layers]\nlint = { enabled = false, reason = \"not built in M1\" }\nbuild-test = { enabled = false, reason = \"not built in M1\" }\ncoverage = { enabled = false, reason = \"not built in M1\" }\nfacet-layers = { enabled = false, reason = \"not built in M1\" }\nlaws = { enabled = false, reason = \"not built in M1\" }\n";

/// The same config with no layer switched off: the five unbuilt layers are then `not-run`.
const CONFIG_ALL_ON: &str = "protocol = \"0.2.0\"\nfacets = [\"package-set\"]\nenforce = \"hard\"\n\n[toolchain]\nsystems = [\"x86_64-linux\"]\n";
const NEJ_OK: &str =
    r#"{"attr":"default","attrPath":["default"],"drvPath":"/nix/store/d.drv","name":"p-1"}"#;

/// The judge's answers for a one-package flake that evaluates cleanly.
fn judge_arms<'a>() -> Vec<Arm<'a>> {
    vec![
        ("hash file", "sha256-LOCK", "", 0),
        ("config show system", "x86_64-linux", "", 0),
        (
            "flake'*'metadata'*'--no-update-lock-file",
            r#"{"url":"fake:locked"}"#,
            "",
            0,
        ),
        (r##"#. --apply"##, r#"["packages"]"#, "", 0),
        (r##"#"packages" --apply"##, r#"["x86_64-linux"]"#, "", 0),
    ]
}

/// `flake check` arms go first so they win over the metadata arm that shares a flag.
fn with_check<'a>(check: Arm<'a>, mut rest: Vec<Arm<'a>>) -> Vec<Arm<'a>> {
    let mut all = vec![check];
    all.append(&mut rest);
    all
}

const CHECK_OK: Arm = ("flake'*'check", "all checks passed!", "", 0);

struct Rig {
    dir: tempfile::TempDir,
    nix: PathBuf,
    tools_dir: tempfile::TempDir,
}

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

/// A committed flake repository with a valid config, a lock and a fake host `nix` for the floor.
fn rig() -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q", "."]);
    git(root, &["config", "user.email", "a@b"]);
    git(root, &["config", "user.name", "n"]);
    fs::write(root.join("flake.nix"), "{ outputs = _: { }; }\n").unwrap();
    fs::write(root.join("flake.lock"), "{}\n").unwrap();
    fs::write(root.join("nix-tops.toml"), CONFIG).unwrap();
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", "x"]);
    let tools_dir = tempfile::tempdir().unwrap();
    let nix = script(
        tools_dir.path(),
        "nix",
        &[("--version", "nix (Nix) 2.35.2", "", 0)],
    );
    Rig {
        dir,
        nix,
        tools_dir,
    }
}

impl Rig {
    fn root(&self) -> &Path {
        self.dir.path()
    }

    fn tools(&self, judge_arms: &[Arm], nej_arms: &[Arm]) -> Tools {
        let judge = script(self.tools_dir.path(), "judge", judge_arms);
        let eval_jobs = script(self.tools_dir.path(), "nej", nej_arms);
        Tools {
            judge,
            eval_jobs,
            nixpkgs: "github:NixOS/nixpkgs/pin".to_string(),
        }
    }

    fn settings(&self) -> Settings {
        Settings {
            root: self.root().to_path_buf(),
            keep_going: false,
            only: None,
            system: None,
            evaluator: Evaluator::Pinned,
            ci: false,
            today: "2026-11-18".to_string(),
            nix: self.nix.display().to_string(),
        }
    }

    /// Replaces `nix-tops.toml` and commits it.
    fn set_config(&self, text: &str) {
        self.write("nix-tops.toml", text);
        git(self.root(), &["add", "-A"]);
        git(self.root(), &["commit", "-q", "-m", "config"]);
    }

    fn write(&self, name: &str, text: &str) {
        fs::write(self.root().join(name), text).unwrap();
    }
}

const NEJ_PACKAGES: [Arm; 1] = [("fake:locked#packages.x86_64-linux", NEJ_OK, "", 0)];

/// No seam: the diagnostics table is the file and the layers come from the config.
fn switched() -> Seams {
    Seams::default()
}

fn layer<'a>(report: &'a Value, id: &str) -> &'a Value {
    report["layers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["id"] == id)
        .unwrap()
}

fn verdict(report: &Value, id: &str) -> String {
    layer(report, id)["verdict"].as_str().unwrap().to_string()
}

fn reason(report: &Value, id: &str) -> String {
    layer(report, id)["reason"]
        .as_str()
        .unwrap_or("")
        .to_string()
}

const REPORT_SCHEMA: &str = include_str!("../../../schema/gate-report.schema.json");
const CORE_SCHEMA: &str = include_str!("../../../schema/core/core.schema.json");

/// The report validates against `schema/gate-report.schema.json`, `$ref`s into `core` resolved
/// from the embedded file.
fn assert_valid(report: &Value) {
    let core: Value = serde_json::from_str(CORE_SCHEMA).unwrap();
    let schema: Value = serde_json::from_str(REPORT_SCHEMA).unwrap();
    let base = "https://nixtops.invalid/schema/";
    let registry = jsonschema::Registry::new()
        .add(
            format!("{base}core/core.schema.json"),
            jsonschema::Resource::from_contents(core),
        )
        .unwrap()
        .prepare()
        .unwrap();
    let validator = jsonschema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .with_base_uri(format!("{base}x.json"))
        .with_registry(&registry)
        .build(&schema)
        .unwrap();
    let errors: Vec<String> = validator
        .iter_errors(report)
        .map(|e| e.to_string())
        .collect();
    assert!(errors.is_empty(), "{errors:?}\n{report}");
}

fn run_json(_rig: &Rig, settings: &Settings, tools: Tools, seams: &Seams) -> (Exit, Value) {
    let result = gate::run(settings, Some(tools), seams);
    let value = serde_json::to_value(&result.report).unwrap();
    assert_valid(&value);
    (result.exit, value)
}

#[test]
fn a_clean_flake_passes_gates_one_and_three_and_the_report_says_so() {
    let rig = rig();
    let tools = rig.tools(&with_check(CHECK_OK, judge_arms()), &NEJ_PACKAGES);
    let (exit, report) = run_json(&rig, &rig.settings(), tools, &switched());
    assert_eq!(exit, Exit::Pass);
    assert_eq!(verdict(&report, "source"), "pass");
    assert_eq!(verdict(&report, "eval"), "pass");
    for id in ["lint", "build-test", "coverage", "facet-layers", "laws"] {
        assert_eq!(verdict(&report, id), "skip", "{id}");
        assert_eq!(reason(&report, id), "not built in M1");
    }
    assert_eq!(report["passed"], true);
    assert_eq!(
        report["outputs"],
        serde_json::json!([{"attr": "packages.x86_64-linux.default", "level": "evaluated"}])
    );
    assert_eq!(report["facets"], serde_json::json!(["package-set"]));
    assert_eq!(
        report["baseline_identity"],
        serde_json::json!({"implementation": "cppnix", "version": report["baseline_identity"]["version"], "system": "x86_64-linux", "lock_hash": "sha256-LOCK"})
    );
    assert_eq!(report["enforce"], "hard");
    assert_eq!(report["protocol"], nixtops::build_info::PROTOCOL);
    assert_eq!(report["counts"]["verdicts"]["skip"], 5);
    assert_eq!(report["counts"]["failed"], 0);
}

#[test]
fn the_report_is_written_where_the_config_says_and_by_default() {
    let rig = rig();
    let tools = rig.tools(&with_check(CHECK_OK, judge_arms()), &NEJ_PACKAGES);
    let _ = gate::run(&rig.settings(), Some(tools), &switched());
    let default = rig.root().join(".nix-tops/report.json");
    let written: Value = serde_json::from_str(&fs::read_to_string(default).unwrap()).unwrap();
    assert_eq!(written["passed"], true);
    rig.write(
        "nix-tops.toml",
        &format!("{CONFIG}\n[report]\npath = \"out/r.json\"\n"),
    );
    git(rig.root(), &["add", "-A"]);
    git(rig.root(), &["commit", "-q", "-m", "p"]);
    let tools = rig.tools(&with_check(CHECK_OK, judge_arms()), &NEJ_PACKAGES);
    let _ = gate::run(&rig.settings(), Some(tools), &switched());
    assert!(rig.root().join("out/r.json").is_file());
}

#[test]
fn layers_that_are_on_and_not_built_are_not_run_and_fail_the_gate() {
    let rig = rig();
    rig.set_config(CONFIG_ALL_ON);
    let tools = rig.tools(&with_check(CHECK_OK, judge_arms()), &NEJ_PACKAGES);
    let (exit, report) = run_json(&rig, &rig.settings(), tools, &Seams::default());
    assert_eq!(exit, Exit::GateFailed);
    assert_eq!(verdict(&report, "eval"), "pass");
    for id in ["lint", "build-test", "coverage", "facet-layers", "laws"] {
        assert_eq!(verdict(&report, id), "not-run", "{id}");
        assert_eq!(reason(&report, id), "not built in this version");
    }
    assert_eq!(report["counts"]["failed"], 5);
    assert_eq!(report["passed"], false);
}

#[test]
fn a_layer_switched_off_in_the_config_is_skip_with_the_configured_reason() {
    let rig = rig();
    rig.set_config(&format!(
        "{CONFIG_ALL_ON}\n[layers]\nlint = {{ enabled = false, reason = \"from the file\" }}\n"
    ));
    let tools = rig.tools(&with_check(CHECK_OK, judge_arms()), &NEJ_PACKAGES);
    let (exit, report) = run_json(&rig, &rig.settings(), tools, &Seams::default());
    assert_eq!(
        (
            verdict(&report, "lint").as_str(),
            reason(&report, "lint").as_str()
        ),
        ("skip", "from the file")
    );
    for id in ["build-test", "coverage", "facet-layers", "laws"] {
        assert_eq!(verdict(&report, id), "not-run", "{id}");
    }
    assert_eq!(exit, Exit::GateFailed);
    assert_eq!(report["counts"]["verdicts"]["skip"], 1);
}

#[test]
fn every_layer_that_can_be_switched_off_is_skip_with_its_own_reason() {
    let rig = rig();
    let layers = ["lint", "build-test", "coverage", "facet-layers", "laws"];
    let lines: String = layers
        .iter()
        .map(|id| format!("{id} = {{ enabled = false, reason = \"why {id}\" }}\n"))
        .collect();
    rig.set_config(&format!("{CONFIG_ALL_ON}\n[layers]\n{lines}"));
    let tools = rig.tools(&with_check(CHECK_OK, judge_arms()), &NEJ_PACKAGES);
    let (exit, report) = run_json(&rig, &rig.settings(), tools, &Seams::default());
    assert_eq!(exit, Exit::Pass);
    for id in layers {
        assert_eq!(verdict(&report, id), "skip", "{id}");
        assert_eq!(reason(&report, id), format!("why {id}"));
    }
    assert_eq!(report["counts"]["verdicts"]["skip"], 5);
    assert_eq!(report["counts"]["failed"], 0);
}

#[test]
fn a_config_that_switches_off_gate_one_or_gate_three_is_invalid() {
    for key in ["source", "eval"] {
        let rig = rig();
        rig.set_config(&format!(
            "{CONFIG}\n[layers.{key}]\nenabled = false\nreason = \"no\"\n"
        ));
        let tools = rig.tools(&with_check(CHECK_OK, judge_arms()), &NEJ_PACKAGES);
        let result = gate::run(&rig.settings(), Some(tools), &switched());
        assert_eq!(result.exit, Exit::ConfigInvalid, "{key}");
        assert!(
            result.problems.iter().any(|p| p.contains("layers")),
            "{:?}",
            result.problems
        );
    }
}

#[test]
fn a_reason_that_is_only_whitespace_is_no_reason_and_exit_two() {
    let rig = rig();
    rig.set_config(&format!(
        "{CONFIG_ALL_ON}\n[layers]\nlaws = {{ enabled = false, reason = \"  \" }}\n"
    ));
    let tools = rig.tools(&with_check(CHECK_OK, judge_arms()), &NEJ_PACKAGES);
    let result = gate::run(&rig.settings(), Some(tools), &switched());
    assert_eq!(result.exit, Exit::ConfigInvalid);
    assert_eq!(
        result.problems,
        ["nix-tops.toml: layers.laws.reason: a reason that is only whitespace is no reason"]
    );
    let report = serde_json::to_value(&result.report).unwrap();
    assert_valid(&report);
    assert_eq!(verdict(&report, "source"), "not-run");
}

#[test]
fn a_gate_with_the_laws_layer_switched_off_says_laws_are_not_enforced() {
    let rig = rig();
    let tools = rig.tools(&with_check(CHECK_OK, judge_arms()), &NEJ_PACKAGES);
    let result = gate::run(&rig.settings(), Some(tools), &switched());
    let text = nixtops::report::render_text(&serde_json::to_value(&result.report).unwrap());
    assert!(text.contains("laws are not enforced in this run"), "{text}");
}

#[test]
fn an_untracked_nix_file_fails_gate_one_names_n04_and_stops_gate_three() {
    let rig = rig();
    rig.write("extra.nix", "{ }\n");
    let tools = rig.tools(&with_check(CHECK_OK, judge_arms()), &NEJ_PACKAGES);
    let (exit, report) = run_json(&rig, &rig.settings(), tools, &switched());
    assert_eq!(exit, Exit::GateFailed);
    assert_eq!(verdict(&report, "source"), "Refuse");
    assert!(reason(&report, "source").contains("N-04: extra.nix is not tracked by git"));
    assert_eq!(verdict(&report, "eval"), "not-run");
    assert_eq!(
        reason(&report, "eval"),
        "stopped: source failed; use --keep-going"
    );
    assert_eq!(report["outputs"], serde_json::json!([]));
}

#[test]
fn keep_going_runs_gate_three_after_gate_one_failed() {
    let rig = rig();
    rig.write("extra.nix", "{ }\n");
    let tools = rig.tools(&with_check(CHECK_OK, judge_arms()), &NEJ_PACKAGES);
    let settings = Settings {
        keep_going: true,
        ..rig.settings()
    };
    let (exit, report) = run_json(&rig, &settings, tools, &switched());
    assert_eq!(exit, Exit::GateFailed);
    assert_eq!(verdict(&report, "source"), "Refuse");
    assert_eq!(verdict(&report, "eval"), "pass");
}

#[test]
fn a_staged_not_committed_file_counts_as_untracked_and_direnv_is_exempt() {
    let rig = rig();
    rig.write("staged.nix", "{ }\n");
    git(rig.root(), &["add", "staged.nix"]);
    fs::create_dir_all(rig.root().join(".direnv")).unwrap();
    rig.write(".direnv/x.nix", "{ }\n");
    let tools = rig.tools(&with_check(CHECK_OK, judge_arms()), &NEJ_PACKAGES);
    let (_, report) = run_json(&rig, &rig.settings(), tools, &switched());
    assert!(reason(&report, "source").contains("staged.nix is not tracked by git"));
    assert!(!reason(&report, "source").contains(".direnv"));
}

#[test]
fn an_ignored_nix_file_counts_too() {
    let rig = rig();
    rig.write(".gitignore", "hidden.nix\n");
    rig.write("hidden.nix", "{ }\n");
    git(rig.root(), &["add", ".gitignore"]);
    git(rig.root(), &["commit", "-q", "-m", "i"]);
    let tools = rig.tools(&with_check(CHECK_OK, judge_arms()), &NEJ_PACKAGES);
    let (_, report) = run_json(&rig, &rig.settings(), tools, &switched());
    assert!(reason(&report, "source").contains("hidden.nix is not tracked by git"));
}

#[test]
fn a_lock_that_is_not_committed_or_is_modified_fails_n05() {
    let rig = rig();
    rig.write("flake.lock", "{\"changed\":1}\n");
    let tools = rig.tools(&with_check(CHECK_OK, judge_arms()), &NEJ_PACKAGES);
    let (_, report) = run_json(&rig, &rig.settings(), tools, &switched());
    assert!(
        reason(&report, "source").contains("N-05: flake.lock has uncommitted changes"),
        "{}",
        reason(&report, "source")
    );
    git(rig.root(), &["rm", "-q", "--cached", "flake.lock"]);
    let tools = rig.tools(&with_check(CHECK_OK, judge_arms()), &NEJ_PACKAGES);
    let (_, report) = run_json(&rig, &rig.settings(), tools, &switched());
    assert!(reason(&report, "source").contains("N-05: flake.lock is not committed"));
}

#[test]
fn a_lock_that_is_not_current_fails_n05_with_the_nix_error() {
    let rig = rig();
    let mut arms = vec![("flake'*'metadata", "", "error: lock file needs updating", 1)];
    arms.extend(judge_arms());
    let tools = rig.tools(&with_check(CHECK_OK, arms), &NEJ_PACKAGES);
    let (_, report) = run_json(&rig, &rig.settings(), tools, &switched());
    assert!(
        reason(&report, "source")
            .contains("N-05: flake.lock is not current: error: lock file needs updating")
    );
}

#[test]
fn in_ci_the_tree_must_be_clean_and_outside_ci_it_need_not_be() {
    let rig = rig();
    rig.write("notes.txt", "x\n");
    let tools = rig.tools(&with_check(CHECK_OK, judge_arms()), &NEJ_PACKAGES);
    let (_, report) = run_json(&rig, &rig.settings(), tools, &switched());
    assert_eq!(verdict(&report, "source"), "pass");
    let tools = rig.tools(&with_check(CHECK_OK, judge_arms()), &NEJ_PACKAGES);
    let ci = Settings {
        ci: true,
        ..rig.settings()
    };
    let (_, report) = run_json(&rig, &ci, tools, &switched());
    assert!(reason(&report, "source").contains("N-04: the working tree is not clean in CI"));
}

#[test]
fn a_directory_outside_git_fails_gate_one() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("nix-tops.toml"), CONFIG).unwrap();
    let rig = rig();
    let tools = rig.tools(&with_check(CHECK_OK, judge_arms()), &NEJ_PACKAGES);
    let settings = Settings {
        root: dir.path().to_path_buf(),
        ..rig.settings()
    };
    let (_, report) = run_json(&rig, &settings, tools, &switched());
    assert!(reason(&report, "source").contains("not inside a git repository"));
}

#[test]
fn a_failing_flake_check_refuses_gate_three_with_the_error() {
    let rig = rig();
    let check = (
        "flake'*'check",
        "",
        "checking x\nerror: attribute 'foo' missing\n  at flake.nix:3",
        1,
    );
    let tools = rig.tools(&with_check(check, judge_arms()), &NEJ_PACKAGES);
    let (exit, report) = run_json(&rig, &rig.settings(), tools, &switched());
    assert_eq!(exit, Exit::GateFailed);
    assert_eq!(verdict(&report, "eval"), "Refuse");
    assert!(
        reason(&report, "eval")
            .contains("flake check failed: error: attribute 'foo' missing | at flake.nix:3")
    );
}

#[test]
fn an_output_error_is_named_and_the_others_still_count() {
    let rig = rig();
    let nej = [(
        "fake:locked#packages.x86_64-linux",
        r#"{"attr":"a","attrPath":["a"],"drvPath":"/d","name":"a"}
{"attr":"b","attrPath":["b"],"error":"error: b broke"}"#,
        "",
        0,
    )];
    let tools = rig.tools(&with_check(CHECK_OK, judge_arms()), &nej);
    let (_, report) = run_json(&rig, &rig.settings(), tools, &switched());
    assert_eq!(verdict(&report, "eval"), "Refuse");
    assert!(reason(&report, "eval").contains("packages.x86_64-linux.b failed: error: b broke"));
    assert_eq!(
        report["outputs"],
        serde_json::json!([{"attr": "packages.x86_64-linux.a", "level": "evaluated"}])
    );
}

#[test]
fn a_flake_with_no_outputs_does_not_pass_the_config_check_or_the_gate() {
    let rig = rig();
    let arms = vec![
        (r##"#. --apply"##, "[]", "", 0),
        (
            "flake'*'metadata'*'--no-update-lock-file",
            r#"{"url":"u"}"#,
            "",
            0,
        ),
        ("hash file", "h", "", 0),
        ("config show system", "s", "", 0),
    ];
    let tools = rig.tools(&with_check(CHECK_OK, arms), &[]);
    let result = gate::run(&rig.settings(), Some(tools), &switched());
    // The config lists a facet and a system that the outputs do not show: exit 2, every layer
    // not-run; the eval layer's own "no output evaluated" rule is the second line of defence.
    assert_eq!(result.exit, Exit::ConfigInvalid);
    let report = serde_json::to_value(&result.report).unwrap();
    assert_valid(&report);
    assert_eq!(verdict(&report, "eval"), "not-run");
    assert!(
        result
            .problems
            .iter()
            .any(|p| p.contains("the outputs do not show it"))
    );
}

#[test]
fn import_from_derivation_is_named_n07() {
    let rig = rig();
    let check = (
        "flake'*'check",
        "",
        "error: cannot build '/nix/store/x.drv' during evaluation because the option 'allow-import-from-derivation' is disabled",
        1,
    );
    let tools = rig.tools(&with_check(check, judge_arms()), &NEJ_PACKAGES);
    let (_, report) = run_json(&rig, &rig.settings(), tools, &switched());
    assert!(reason(&report, "eval").contains("N-07: import from derivation is off"));
}

const STRICT_STOPPED: Arm = (
    "flake'*'check'*'abort-on-warn true",
    "",
    "evaluation warning: the package has been renamed\nerror: aborting to reveal stack trace of warning, as abort-on-warn is set",
    1,
);
const LENIENT_WITH_WARNING: Arm = (
    "flake'*'check'*'abort-on-warn false",
    "",
    "evaluation warning: the package has been renamed\nevaluation warning: second warning",
    0,
);

fn warn_arms<'a>() -> Vec<Arm<'a>> {
    let mut arms = vec![LENIENT_WITH_WARNING, STRICT_STOPPED];
    arms.extend(judge_arms());
    arms
}

#[test]
fn a_warning_that_stops_the_run_is_collected_once_and_refused_without_a_ledger_entry() {
    let rig = rig();
    let tools = rig.tools(&warn_arms(), &NEJ_PACKAGES);
    let (exit, report) = run_json(&rig, &rig.settings(), tools, &switched());
    assert_eq!(exit, Exit::GateFailed);
    assert_eq!(verdict(&report, "eval"), "Refuse");
    let warnings = report["warnings"].as_array().unwrap();
    assert_eq!(warnings.len(), 2);
    assert_eq!(
        (
            warnings[0]["source"].as_str(),
            warnings[0]["verdict"].as_str()
        ),
        (Some("builtins-warn"), Some("Refuse"))
    );
    assert_eq!(warnings[1]["message"], "second warning");
    assert!(reason(&report, "eval").contains("builtins.warn: the package has been renamed"));
    assert_eq!(report["acks"]["count"], 0);
}

#[test]
fn every_collected_warning_matched_by_the_ledger_turns_the_failure_into_an_ack() {
    let rig = rig();
    rig.write("nix-tops.toml", &format!("{CONFIG}\n[[warnings.allow]]\nmatch = \"renamed\"\nreason = \"upstream\"\nuntil = \"2099-12-31\"\n\n[[warnings.allow]]\nmatch = \"second\"\nreason = \"also\"\nuntil = \"2099-12-31\"\n"));
    git(rig.root(), &["add", "-A"]);
    git(rig.root(), &["commit", "-q", "-m", "l"]);
    let tools = rig.tools(&warn_arms(), &NEJ_PACKAGES);
    let (exit, report) = run_json(&rig, &rig.settings(), tools, &switched());
    assert_eq!(exit, Exit::Pass);
    assert_eq!(verdict(&report, "eval"), "pass");
    let warnings = report["warnings"].as_array().unwrap();
    assert!(
        warnings
            .iter()
            .all(|w| w["verdict"] == "Ack" && w["entry"]["kind"] == "ledger")
    );
    assert_eq!(warnings[0]["entry"]["until"], "2099-12-31");
    assert_eq!(warnings[0]["entry"]["match"], "renamed");
    assert_eq!(report["acks"]["count"], 2);
    assert_eq!(report["counts"]["verdicts"]["ack"], 2);
}

#[test]
fn one_unmatched_warning_among_matched_ones_still_fails() {
    let rig = rig();
    rig.write("nix-tops.toml", &format!("{CONFIG}\n[[warnings.allow]]\nmatch = \"renamed\"\nreason = \"upstream\"\nuntil = \"2099-12-31\"\n"));
    git(rig.root(), &["add", "-A"]);
    git(rig.root(), &["commit", "-q", "-m", "l"]);
    let tools = rig.tools(&warn_arms(), &NEJ_PACKAGES);
    let (_, report) = run_json(&rig, &rig.settings(), tools, &switched());
    assert_eq!(verdict(&report, "eval"), "Refuse");
    assert_eq!(report["warnings"][0]["verdict"], "Ack");
    assert_eq!(report["warnings"][1]["verdict"], "Refuse");
}

#[test]
fn an_expired_ledger_entry_stops_the_run_in_the_config_check_with_exit_two() {
    let rig = rig();
    rig.write("nix-tops.toml", &format!("{CONFIG}\n[[warnings.allow]]\nmatch = \"renamed\"\nreason = \"upstream\"\nuntil = \"2020-01-01\"\n"));
    git(rig.root(), &["add", "-A"]);
    git(rig.root(), &["commit", "-q", "-m", "l"]);
    let tools = rig.tools(&warn_arms(), &NEJ_PACKAGES);
    let result = gate::run(&rig.settings(), Some(tools), &switched());
    assert_eq!(result.exit, Exit::ConfigInvalid);
    assert!(
        result.problems[0].contains("warnings.allow[0].until"),
        "{:?}",
        result.problems
    );
    let report = serde_json::to_value(&result.report).unwrap();
    assert_valid(&report);
    for id in [
        "source",
        "lint",
        "eval",
        "build-test",
        "coverage",
        "facet-layers",
        "laws",
    ] {
        assert_eq!(verdict(&report, id), "not-run", "{id}");
        assert!(reason(&report, id).contains("expired"));
    }
    assert_eq!(report["passed"], false);
    let written = rig.root().join(".nix-tops/report.json");
    assert!(written.is_file());
}

#[test]
fn an_invalid_config_is_exit_two_and_still_writes_a_report() {
    let rig = rig();
    rig.write("nix-tops.toml", "protocol = \"9.9.9\"\n");
    let tools = rig.tools(&with_check(CHECK_OK, judge_arms()), &NEJ_PACKAGES);
    let result = gate::run(&rig.settings(), Some(tools), &Seams::default());
    assert_eq!(result.exit, Exit::ConfigInvalid);
    assert!(rig.root().join(".nix-tops/report.json").is_file());
    assert_eq!(result.report.layers.len(), 7);
    assert_valid(&serde_json::to_value(&result.report).unwrap());
}

#[test]
fn a_host_below_the_floor_is_exit_three_before_any_layer() {
    let rig = rig();
    let old = script(
        rig.tools_dir.path(),
        "oldnix",
        &[("--version", "nix (Nix) 2.20.0", "", 0)],
    );
    let settings = Settings {
        nix: old.display().to_string(),
        ..rig.settings()
    };
    let tools = rig.tools(&with_check(CHECK_OK, judge_arms()), &NEJ_PACKAGES);
    let result = gate::run(&settings, Some(tools), &Seams::default());
    assert_eq!(result.exit, Exit::Environment);
    assert_eq!(
        serde_json::to_value(&result.report).unwrap()["layers"][2]["verdict"],
        "not-run"
    );
    // The reason is shown, and it is the floor, not a config key.
    assert_eq!(result.problems.len(), 1);
    assert!(result.problems[0].contains("daemon floor not met: CppNix 2.20.0 is below 2.24"));
    let missing = Settings {
        nix: "/nonexistent/nix".into(),
        ..rig.settings()
    };
    let tools = rig.tools(&with_check(CHECK_OK, judge_arms()), &NEJ_PACKAGES);
    let result = gate::run(&missing, Some(tools), &Seams::default());
    assert_eq!(result.exit, Exit::Environment);
    assert_eq!(result.problems.len(), 1);
    assert!(result.problems[0].contains("cannot read the version of `/nonexistent/nix`"));
}

#[test]
fn no_pinned_judge_is_an_environment_error_with_a_report() {
    let rig = rig();
    let result = gate::run(&rig.settings(), None, &Seams::default());
    assert_eq!(result.exit, Exit::Environment);
    assert!(result.problems[0].contains("no pinned judge"));
    assert!(rig.root().join(".nix-tops/report.json").is_file());
}

#[test]
fn only_runs_one_layer_and_the_rest_are_not_selected() {
    let rig = rig();
    let tools = rig.tools(&with_check(CHECK_OK, judge_arms()), &NEJ_PACKAGES);
    let settings = Settings {
        only: Some(LayerId::Eval),
        ..rig.settings()
    };
    let (exit, report) = run_json(&rig, &settings, tools, &switched());
    assert_eq!(exit, Exit::GateFailed);
    assert_eq!(verdict(&report, "eval"), "pass");
    assert_eq!(verdict(&report, "source"), "not-run");
    assert_eq!(reason(&report, "source"), "not selected: --only eval");
}

#[test]
fn the_system_filter_replaces_all_systems_and_keeps_only_that_systems_outputs() {
    let rig = rig();
    let arms = vec![
        (
            "flake'*'check'*'--system x86_64-linux",
            "all checks passed!",
            "",
            0,
        ),
        ("flake'*'check'*'--all-systems", "", "error: wrong scope", 1),
    ];
    let mut all = arms;
    all.extend(judge_arms());
    let tools = rig.tools(&all, &NEJ_PACKAGES);
    let settings = Settings {
        system: Some("x86_64-linux".into()),
        ..rig.settings()
    };
    let (_, report) = run_json(&rig, &settings, tools, &switched());
    assert_eq!(verdict(&report, "eval"), "pass");
    assert_eq!(report["outputs"].as_array().unwrap().len(), 1);
}

#[test]
fn an_unclassified_diagnostic_fails_and_a_table_entry_for_it_turns_it_into_a_report_line() {
    let rig = rig();
    let check = (
        "flake'*'check",
        "all checks passed!",
        "warning: ignoring untrusted flake configuration setting 'x'.\nPass '--accept-flake-config' to trust it",
        0,
    );
    let tools = rig.tools(&with_check(check, judge_arms()), &NEJ_PACKAGES);
    let (exit, report) = run_json(&rig, &rig.settings(), tools, &switched());
    assert_eq!(exit, Exit::GateFailed);
    assert_eq!(verdict(&report, "eval"), "Refuse");
    assert_eq!(report["warnings"][0]["source"], "diagnostic");
    assert!(
        report["warnings"][0]["message"]
            .as_str()
            .unwrap()
            .ends_with("[unclassified diagnostic]")
    );
    // The seam: the same line classified by an entry. Off by default; this is the test-only way in.
    let extra = diag::parse("[[diagnostic]]\nid = \"X-01\"\nimplementation = \"cppnix\"\neffect = \"report\"\nmatch = \"^warning: ignoring untrusted flake configuration setting '[^']+'\\\\.$\"\n").unwrap();
    let seams = Seams {
        extra_diagnostics: extra,
    };
    let tools = rig.tools(&with_check(check, judge_arms()), &NEJ_PACKAGES);
    let (exit, report) = run_json(&rig, &rig.settings(), tools, &seams);
    assert_eq!(exit, Exit::Pass);
    assert_eq!(report["warnings"][0]["verdict"], "pass");
    assert!(
        report["warnings"][0]["message"]
            .as_str()
            .unwrap()
            .ends_with("[X-01]")
    );
}

#[test]
fn a_fail_effect_entry_fails_the_gate() {
    let rig = rig();
    let check = (
        "flake'*'check",
        "all checks passed!",
        "warning: bad thing happened",
        0,
    );
    let extra = diag::parse("[[diagnostic]]\nid = \"X-02\"\nimplementation = \"cppnix\"\neffect = \"fail\"\nmatch = \"^warning: bad thing happened$\"\n").unwrap();
    let seams = Seams {
        extra_diagnostics: extra,
    };
    let tools = rig.tools(&with_check(check, judge_arms()), &NEJ_PACKAGES);
    let (_, report) = run_json(&rig, &rig.settings(), tools, &seams);
    assert_eq!(verdict(&report, "eval"), "Refuse");
    assert_eq!(report["warnings"][0]["verdict"], "Refuse");
}

#[test]
fn the_flake_path_does_not_appear_in_the_report() {
    let rig = rig();
    let root = rig.root().canonicalize().unwrap().display().to_string();
    let check = (
        "flake'*'check",
        "",
        &*format!("error: boom at {root}/flake.nix:1"),
        1,
    );
    let tools = rig.tools(&with_check(check, judge_arms()), &NEJ_PACKAGES);
    let (_, report) = run_json(&rig, &rig.settings(), tools, &switched());
    assert!(reason(&report, "eval").contains("at ./flake.nix:1"));
    assert!(!report.to_string().contains(&root));
}

fn config_arms<'a>(warnings: &'a str) -> Vec<Arm<'a>> {
    vec![
        (".config.warnings", warnings, "", 0),
        (r##"#"nixosConfigurations" --apply"##, r#"["host"]"#, "", 0),
        (
            "config.system.build.toplevel.drvPath",
            r#"{"drvPath":"/nix/store/h.drv","name":"nixos"}"#,
            "",
            0,
        ),
        (
            r##"#. --apply"##,
            r#"["nixosConfigurations","packages"]"#,
            "",
            0,
        ),
        ("hash file", "h", "", 0),
        ("config show system", "x86_64-linux", "", 0),
        (
            "flake'*'metadata'*'--no-update-lock-file",
            r#"{"url":"fake:locked"}"#,
            "",
            0,
        ),
        (r##"#"packages" --apply"##, r#"["x86_64-linux"]"#, "", 0),
    ]
}

#[test]
fn config_warnings_are_read_as_data_and_judged_like_the_others() {
    let rig = rig();
    let tools = rig.tools(
        &with_check(CHECK_OK, config_arms(r#"["option foo is deprecated"]"#)),
        &NEJ_PACKAGES,
    );
    let (_, report) = run_json(&rig, &rig.settings(), tools, &switched());
    assert_eq!(verdict(&report, "eval"), "Refuse");
    assert_eq!(report["warnings"].as_array().unwrap().len(), 1);
    assert_eq!(report["warnings"][0]["source"], "config-warnings");
    assert!(reason(&report, "eval").contains("config.warnings: option foo is deprecated"));
    rig.write("nix-tops.toml", &format!("{CONFIG}\n[[warnings.allow]]\nmatch = \"deprecated\"\nreason = \"r\"\nuntil = \"2099-12-31\"\n"));
    git(rig.root(), &["add", "-A"]);
    git(rig.root(), &["commit", "-q", "-m", "l"]);
    let tools = rig.tools(
        &with_check(CHECK_OK, config_arms(r#"["option foo is deprecated"]"#)),
        &NEJ_PACKAGES,
    );
    let (exit, report) = run_json(&rig, &rig.settings(), tools, &switched());
    assert_eq!(exit, Exit::Pass);
    assert_eq!(report["warnings"][0]["verdict"], "Ack");
    let tools = rig.tools(&with_check(CHECK_OK, config_arms("[]")), &NEJ_PACKAGES);
    let (_, report) = run_json(&rig, &rig.settings(), tools, &switched());
    assert_eq!(report["warnings"], serde_json::json!([]));
}

#[test]
fn the_host_evaluator_lists_derivations_one_attribute_at_a_time() {
    let rig = rig();
    let host_nix = script(
        rig.tools_dir.path(),
        "hostnix",
        &[
            ("--version", "nix (Determinate Nix 3.22.3) 2.35.2", "", 0),
            ("flake'*'check", "all checks passed!", "", 0),
            ("hash file", "sha256-LOCK", "", 0),
            ("config show system", "aarch64-darwin", "", 0),
            (
                "flake'*'metadata'*'--no-update-lock-file",
                r#"{"url":"fake:locked"}"#,
                "",
                0,
            ),
            (r##"#. --apply"##, r#"["packages"]"#, "", 0),
            (
                r##"#"packages"."x86_64-linux" --apply"##,
                r#"["a","b"]"#,
                "",
                0,
            ),
            (r##"#"packages" --apply"##, r#"["x86_64-linux"]"#, "", 0),
            (
                r##"#"packages"."x86_64-linux"."a" --apply"##,
                r#"{"drvPath":"/nix/store/a.drv","name":"a"}"#,
                "",
                0,
            ),
            (
                r##"#"packages"."x86_64-linux"."b" --apply"##,
                "",
                "error: b broke",
                1,
            ),
        ],
    );
    let settings = Settings {
        evaluator: Evaluator::Host,
        nix: host_nix.display().to_string(),
        ..rig.settings()
    };
    let tools = Tools {
        judge: PathBuf::from("/unused"),
        eval_jobs: PathBuf::from("/nonexistent/nej"),
        nixpkgs: "n".into(),
    };
    let (_, report) = run_json(&rig, &settings, tools, &switched());
    assert_eq!(report["baseline_identity"]["implementation"], "determinate");
    assert_eq!(report["baseline_identity"]["version"], "3.22.3");
    assert_eq!(report["baseline_identity"]["system"], "aarch64-darwin");
    assert_eq!(verdict(&report, "eval"), "Refuse");
    assert!(reason(&report, "eval").contains("packages.x86_64-linux.b failed: error: b broke"));
    assert_eq!(
        report["outputs"],
        serde_json::json!([{"attr": "packages.x86_64-linux.a", "level": "evaluated"}])
    );
}

#[test]
fn a_determinate_host_gets_its_only_settings_turned_off() {
    let rig = rig();
    let log = rig
        .root()
        .join("..")
        .join(format!("args-{}.log", std::process::id()));
    let body = format!(
        "#!/bin/sh\necho \"$*\" >> {}\ncase \"$*\" in\n*--version*) echo 'nix (Determinate Nix 3.22.3) 2.35.2';;\n*metadata*) echo '{{\"url\":\"u\"}}';;\n*'#. --apply'*) echo '[\"packages\"]';;\n*'\"packages\" --apply'*) echo '[\"x86_64-linux\"]';;\n*) echo ok;;\nesac\n",
        log.display()
    );
    let host = rig.tools_dir.path().join("detnix");
    common::write_executable(&host, &body);
    let settings = Settings {
        evaluator: Evaluator::Host,
        nix: host.display().to_string(),
        ..rig.settings()
    };
    let tools = Tools {
        judge: PathBuf::from("/unused"),
        eval_jobs: PathBuf::from("/unused"),
        nixpkgs: "n".into(),
    };
    let _ = gate::run(&settings, Some(tools), &switched());
    let calls = fs::read_to_string(&log).unwrap();
    let _ = fs::remove_file(&log);
    let checks: Vec<&str> = calls
        .lines()
        .filter(|l| l.contains("flake check"))
        .collect();
    assert_eq!(checks.len(), 1, "{calls}");
    assert!(
        checks[0].ends_with("--option lazy-trees false --option eval-cores 1")
            || checks[0].contains("--option lazy-trees false --option eval-cores 1"),
        "{}",
        checks[0]
    );
    assert!(
        checks[0]
            .contains("--option abort-on-warn true --option allow-import-from-derivation false")
    );
}

#[test]
fn a_failing_hash_or_system_probe_gives_none_and_unknown_not_a_guess() {
    let rig = rig();
    for (hash, system, want_hash, want_system) in [
        (
            ("hash file", "sha256-X", "", 1),
            ("config show system", "x86_64-linux", "", 0),
            "none",
            "x86_64-linux",
        ),
        (
            ("hash file", "", "", 0),
            ("config show system", "x86_64-linux", "", 0),
            "none",
            "x86_64-linux",
        ),
        (
            ("hash file", "sha256-X", "", 0),
            ("config show system", "linux", "", 1),
            "sha256-X",
            "unknown",
        ),
        (
            ("hash file", "sha256-X", "", 0),
            ("config show system", "", "", 0),
            "sha256-X",
            "unknown",
        ),
    ] {
        let mut arms = vec![CHECK_OK, hash, system];
        arms.extend(
            judge_arms()
                .into_iter()
                .filter(|a| a.0 != "hash file" && a.0 != "config show system"),
        );
        let tools = rig.tools(&arms, &NEJ_PACKAGES);
        let (_, report) = run_json(&rig, &rig.settings(), tools, &switched());
        assert_eq!(report["baseline_identity"]["lock_hash"], want_hash);
        assert_eq!(report["baseline_identity"]["system"], want_system);
    }
}

#[test]
fn a_lock_error_without_the_word_error_and_one_without_the_word_lock_both_surface() {
    let rig = rig();
    for stderr in ["the lock file is stale", "error: something else broke"] {
        let mut arms = vec![("flake'*'metadata", "", stderr, 1)];
        arms.extend(judge_arms());
        let tools = rig.tools(&with_check(CHECK_OK, arms), &NEJ_PACKAGES);
        let (_, report) = run_json(&rig, &rig.settings(), tools, &switched());
        assert!(
            reason(&report, "source").contains(&format!("flake.lock is not current: {stderr}")),
            "{}",
            reason(&report, "source")
        );
    }
    let mut arms = vec![("flake'*'metadata", "", "unrelated noise", 1)];
    arms.extend(judge_arms());
    let tools = rig.tools(&with_check(CHECK_OK, arms), &NEJ_PACKAGES);
    let (_, report) = run_json(&rig, &rig.settings(), tools, &switched());
    assert!(reason(&report, "source").contains("evaluation with --no-update-lock-file failed"));
}

#[test]
fn a_warning_that_stops_only_the_inventory_still_triggers_the_collect_rerun() {
    let rig = rig();
    let nej_strict = [
        (
            "abort-on-warn'*'true",
            r#"{"attr":"default","attrPath":["default"],"error":"error: aborting to reveal stack trace of warning, as abort-on-warn is set"}"#,
            "",
            0,
        ),
        (
            "abort-on-warn'*'false",
            r#"{"attr":"default","attrPath":["default"],"drvPath":"/d","name":"p"}"#,
            "",
            0,
        ),
    ];
    let tools = rig.tools(&with_check(CHECK_OK, judge_arms()), &nej_strict);
    let (_, report) = run_json(&rig, &rig.settings(), tools, &switched());
    assert_eq!(verdict(&report, "eval"), "pass");
    assert_eq!(report["outputs"].as_array().unwrap().len(), 1);
}

#[test]
fn import_from_derivation_named_only_in_an_output_error_is_still_n07() {
    let rig = rig();
    let nej = [(
        "fake:locked#packages",
        r#"{"attr":"default","attrPath":["default"],"error":"error: import-from-derivation is disabled"}"#,
        "",
        0,
    )];
    let tools = rig.tools(&with_check(CHECK_OK, judge_arms()), &nej);
    let (_, report) = run_json(&rig, &rig.settings(), tools, &switched());
    assert!(reason(&report, "eval").contains("N-07"));
}

#[test]
fn a_configuration_that_failed_is_not_asked_for_warnings_and_other_outputs_never_are() {
    let rig = rig();
    let mut arms = vec![
        (".config.warnings", r#"["asked"]"#, "", 0),
        (
            "config.system.build.toplevel.drvPath",
            "",
            "error: no toplevel",
            1,
        ),
    ];
    arms.extend(
        config_arms("[]")
            .into_iter()
            .filter(|a| !a.0.contains("toplevel") && a.0 != ".config.warnings"),
    );
    let tools = rig.tools(&with_check(CHECK_OK, arms), &NEJ_PACKAGES);
    let (_, report) = run_json(&rig, &rig.settings(), tools, &switched());
    assert_eq!(report["warnings"], serde_json::json!([]));
    assert!(
        reason(&report, "eval").contains("nixosConfigurations.host failed: error: no toplevel")
    );
}
