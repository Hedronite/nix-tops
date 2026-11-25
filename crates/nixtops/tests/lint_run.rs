//! Gate 2 (lint) integration tests with stand-in tools: direct checks, cross-system passthru,
//! kit-lib fallback, per-file counts in `report.json`, and failure under `hard`.

mod common;

use nixtops::{
    exit::Exit,
    gate::{self, Evaluator, Seams, Settings},
    inventory::Tools,
    report::{LayerId, render_text},
};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

type Arm<'a> = (&'a str, &'a str, &'a str, i32);
type ToolCounts<'a> = (&'a str, &'a str, &'a [(&'a str, usize)]);

const REPORT_SCHEMA: &str = include_str!("../../../schema/gate-report.schema.json");
const CORE_SCHEMA: &str = include_str!("../../../schema/core/core.schema.json");

const CONFIG: &str = "protocol = \"0.2.0\"\nfacets = [\"package-set\"]\nenforce = \"hard\"\n\n[toolchain]\nsystems = [\"x86_64-linux\"]\n\n[layers]\nbuild-test = { enabled = false, reason = \"not built yet\" }\ncoverage = { enabled = false, reason = \"not built yet\" }\nfacet-layers = { enabled = false, reason = \"not built yet\" }\nlaws = { enabled = false, reason = \"not built yet\" }\n";

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

fn script(dir: &Path, name: &str, arms: &[Arm]) -> PathBuf {
    let mut text = String::from("#!/bin/sh\ncase \"$*\" in\n");
    for (pattern, stdout, stderr, code) in arms {
        text.push_str(&format!(
            "*'{pattern}'*)\ncat <<'__OUT__'\n{stdout}\n__OUT__\ncat >&2 <<'__ERR__'\n{stderr}\n__ERR__\nexit {code};;\n"
        ));
    }
    text.push_str("*) echo \"no arm for: $*\" >&2; exit 99;;\nesac\n");
    let path = dir.join(name);
    common::write_executable(&path, &text);
    path
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

struct Rig {
    dir: tempfile::TempDir,
    tools_dir: tempfile::TempDir,
    nix: PathBuf,
}

impl Rig {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        git(root, &["init", "-q", "."]);
        git(root, &["config", "user.email", "a@b"]);
        git(root, &["config", "user.name", "n"]);
        fs::write(root.join("flake.nix"), "{ outputs = _: { }; }\n").unwrap();
        fs::write(root.join("flake.lock"), "{}\n").unwrap();
        fs::write(root.join("nix-tops.toml"), CONFIG).unwrap();
        git(root, &["add", "-A"]);
        git(root, &["commit", "-q", "-m", "init"]);
        let tools_dir = tempfile::tempdir().unwrap();
        let nix = script(
            tools_dir.path(),
            "nix",
            &[("--version", "nix (Nix) 2.35.2", "", 0)],
        );
        Self {
            dir,
            tools_dir,
            nix,
        }
    }

    fn settings(&self) -> Settings {
        Settings {
            root: self.dir.path().to_path_buf(),
            keep_going: false,
            only: None,
            system: None,
            evaluator: Evaluator::Pinned,
            ci: false,
            today: "2026-11-25".into(),
            nix: self.nix.display().to_string(),
            writing_baseline: false,
        }
    }

    fn write_findings(&self, counts: &[ToolCounts<'_>]) -> String {
        let mut arr = String::from("[");
        for (idx, (tool, version, files)) in counts.iter().enumerate() {
            let mut file_map = serde_json::Map::new();
            for (path, n) in *files {
                let items: Vec<Value> = (0..*n)
                    .map(|line| serde_json::json!({"line": line + 1}))
                    .collect();
                file_map.insert((*path).to_string(), Value::Array(items));
            }
            let payload = serde_json::json!({
                "tool": tool,
                "version": version,
                "config_hash": format!("sha256-{tool}"),
                "files": file_map,
            });
            let out_file = self.tools_dir.path().join(format!("{tool}.json"));
            fs::write(&out_file, serde_json::to_string(&payload).unwrap()).unwrap();
            if idx > 0 {
                arr.push(',');
            }
            arr.push_str(&format!(
                r#"{{"outputs":{{"out":"{}"}}}}"#,
                out_file.display()
            ));
        }
        arr.push(']');
        arr
    }

    fn tools(&self, arms: &[Arm]) -> Tools {
        let judge = script(self.tools_dir.path(), "judge", arms);
        let nej = script(
            self.tools_dir.path(),
            "nej",
            &[(
                "--flake",
                r#"{"attr":"default","attrPath":["default"],"drvPath":"/nix/store/d.drv","name":"p-1"}"#,
                "",
                0,
            )],
        );
        Tools {
            judge,
            eval_jobs: nej,
            nixpkgs: "github:NixOS/nixpkgs/pin".into(),
        }
    }
}

fn base_arms<'a>() -> Vec<Arm<'a>> {
    vec![
        ("flake check", "ok", "", 0),
        ("hash file", "sha256-LOCK", "", 0),
        ("config show system", "x86_64-linux", "", 0),
        (
            "flake'*'metadata'*'--no-update-lock-file",
            r#"{"url":"fake:locked"}"#,
            "",
            0,
        ),
        (r##"#. --apply"##, r#"["packages"]"#, "", 0),
        (r##"#."packages" --apply"##, r#"["x86_64-linux"]"#, "", 0),
    ]
}

const FINDINGS_NAMES: &str = r#"["nixtops-nixfmt-findings","nixtops-statix-findings","nixtops-deadnix-findings","nixtops-gitleaks-findings"]"#;

#[test]
fn clean_lint_passes_and_records_tool_versions_and_counts_in_report() {
    let rig = Rig::new();
    let build_json = rig.write_findings(&[
        ("nixfmt", "1.5.0", &[]),
        ("statix", "0.5.8", &[]),
        ("deadnix", "1.3.2", &[]),
        ("gitleaks", "8.30.1", &[]),
    ]);
    let mut arms = base_arms();
    arms.push((r##"#."checks" --apply"##, r#"["x86_64-linux"]"#, "", 0));
    arms.push((
        r##"#."checks"."x86_64-linux" --apply"##,
        FINDINGS_NAMES,
        "",
        0,
    ));
    arms.push(("build'*'--no-link", &build_json, "", 0));

    let result = gate::run(&rig.settings(), Some(rig.tools(&arms)), &Seams::default());
    assert_eq!(result.exit, Exit::Pass);
    let report = serde_json::to_value(&result.report).unwrap();
    assert_valid(&report);
    assert_eq!(report["lint"]["system"], "x86_64-linux");
    assert_eq!(report["lint"]["tools"].as_array().unwrap().len(), 4);
    assert_eq!(report["lint"]["tools"][2]["tool"], "deadnix");
    assert_eq!(report["lint"]["tools"][2]["version"], "1.3.2");
    let text = render_text(&report);
    assert!(text.contains("lint (x86_64-linux)"), "{text}");
    assert!(text.contains("deadnix 1.3.2: 0 finding(s)"), "{text}");
}

#[test]
fn findings_fail_under_hard_stop_eval_and_record_per_file_counts() {
    let rig = Rig::new();
    let build_json = rig.write_findings(&[
        ("nixfmt", "1.5.0", &[]),
        ("statix", "0.5.8", &[("nix/a.nix", 2)]),
        ("deadnix", "1.3.2", &[("flake.nix", 1)]),
        ("gitleaks", "8.30.1", &[]),
    ]);
    let mut arms = base_arms();
    arms.push((r##"#."checks" --apply"##, r#"["x86_64-linux"]"#, "", 0));
    arms.push((
        r##"#."checks"."x86_64-linux" --apply"##,
        FINDINGS_NAMES,
        "",
        0,
    ));
    arms.push(("build'*'--no-link", &build_json, "", 0));

    let result = gate::run(&rig.settings(), Some(rig.tools(&arms)), &Seams::default());
    assert_eq!(result.exit, Exit::GateFailed);
    let report = serde_json::to_value(&result.report).unwrap();
    assert_valid(&report);
    let lint_layer = report["layers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["id"] == "lint")
        .unwrap();
    assert_eq!(lint_layer["verdict"], "Refuse");
    let reason = lint_layer["reason"].as_str().unwrap();
    assert!(reason.contains("statix: nix/a.nix (2)"), "{reason}");
    assert!(reason.contains("deadnix: flake.nix (1)"), "{reason}");
    let eval_layer = report["layers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["id"] == "eval")
        .unwrap();
    assert_eq!(eval_layer["verdict"], "not-run");
    assert_eq!(report["lint"]["tools"][1]["files"]["nix/a.nix"], 2);
    assert_eq!(report["lint"]["tools"][2]["files"]["flake.nix"], 1);
}

#[test]
fn cross_system_passthru_and_kit_lib_fallbacks_work() {
    let rig = Rig::new();
    let build_json = rig.write_findings(&[
        ("nixfmt", "1.5.0", &[]),
        ("statix", "0.5.8", &[]),
        ("deadnix", "1.3.2", &[]),
        ("gitleaks", "8.30.1", &[]),
    ]);
    let mut passthru_arms = base_arms();
    passthru_arms.push((r##"#."checks" --apply"##, r#"["aarch64-darwin"]"#, "", 0));
    passthru_arms.push((
        r##"#."checks"."aarch64-darwin" --apply"##,
        FINDINGS_NAMES,
        "",
        0,
    ));
    passthru_arms.push((
        "flake'*'prefetch",
        r#"{"storePath":"/nix/store/s","hash":"sha256-h"}"#,
        "",
        0,
    ));
    passthru_arms.push(("passthru.forSystem", &build_json, "", 0));

    let res = gate::run(
        &rig.settings(),
        Some(rig.tools(&passthru_arms)),
        &Seams::default(),
    );
    assert_eq!(res.exit, Exit::Pass);
    assert_eq!(res.report.lint.as_ref().unwrap().system, "x86_64-linux");
    assert_eq!(res.report.lint.as_ref().unwrap().tools.len(), 4);

    let mut kit_arms = base_arms();
    kit_arms.push((r##"#."checks" --apply"##, "", "error: no checks", 1));
    kit_arms.push((
        "flake'*'prefetch",
        r#"{"storePath":"/nix/store/s","hash":"sha256-h"}"#,
        "",
        0,
    ));
    kit_arms.push(("mkChecks", &build_json, "", 0));

    let res_kit = gate::run(
        &rig.settings(),
        Some(rig.tools(&kit_arms)),
        &Seams::default(),
    );
    assert_eq!(res_kit.exit, Exit::Pass);
    assert_eq!(res_kit.report.lint.as_ref().unwrap().system, "x86_64-linux");
    assert_eq!(res_kit.report.lint.as_ref().unwrap().tools.len(), 4);
}

#[test]
fn failed_nix_build_and_only_lint_are_handled_cleanly() {
    let rig = Rig::new();
    let mut fail_arms = base_arms();
    fail_arms.push((r##"#."checks" --apply"##, r#"["x86_64-linux"]"#, "", 0));
    fail_arms.push((
        r##"#."checks"."x86_64-linux" --apply"##,
        FINDINGS_NAMES,
        "",
        0,
    ));
    fail_arms.push(("build'*'--no-link", "", "error: build broke\n", 1));

    let res = gate::run(
        &rig.settings(),
        Some(rig.tools(&fail_arms)),
        &Seams::default(),
    );
    assert_eq!(res.exit, Exit::GateFailed);
    let report = serde_json::to_value(&res.report).unwrap();
    assert_valid(&report);
    assert!(
        report["layers"][1]["reason"]
            .as_str()
            .unwrap()
            .contains("error: build broke")
    );

    let build_json = rig.write_findings(&[
        ("nixfmt", "1.5.0", &[]),
        ("statix", "0.5.8", &[]),
        ("deadnix", "1.3.2", &[]),
        ("gitleaks", "8.30.1", &[]),
    ]);
    let mut only_arms = base_arms();
    only_arms.push((r##"#."checks" --apply"##, r#"["x86_64-linux"]"#, "", 0));
    only_arms.push((
        r##"#."checks"."x86_64-linux" --apply"##,
        FINDINGS_NAMES,
        "",
        0,
    ));
    only_arms.push(("build'*'--no-link", &build_json, "", 0));

    let settings = Settings {
        only: Some(LayerId::Lint),
        ..rig.settings()
    };
    let only_res = gate::run(&settings, Some(rig.tools(&only_arms)), &Seams::default());
    assert_eq!(only_res.exit, Exit::GateFailed);
    assert_eq!(only_res.report.layers[1].verdict.name(), "pass");
    assert_eq!(only_res.report.layers[0].verdict.name(), "not-run");
}

#[test]
fn bad_findings_json_and_prefetch_error_fail_lint() {
    let rig = Rig::new();
    let bad_file = rig.tools_dir.path().join("bad.json");
    fs::write(&bad_file, "not json").unwrap();
    let bad_build = format!(
        r#"[{{"outputs":{{"out":"{}"}}}},{{"outputs":{{"out":"{}"}}}},{{"outputs":{{"out":"{}"}}}},{{"outputs":{{"out":"{}"}}}}]"#,
        bad_file.display(),
        bad_file.display(),
        bad_file.display(),
        bad_file.display()
    );
    let mut arms = base_arms();
    arms.push((r##"#."checks" --apply"##, r#"["x86_64-linux"]"#, "", 0));
    arms.push((
        r##"#."checks"."x86_64-linux" --apply"##,
        FINDINGS_NAMES,
        "",
        0,
    ));
    arms.push(("build'*'--no-link", &bad_build, "", 0));
    let res = gate::run(&rig.settings(), Some(rig.tools(&arms)), &Seams::default());
    assert_eq!(res.exit, Exit::GateFailed);
    assert!(
        res.report.layers[1]
            .reason
            .as_deref()
            .unwrap()
            .contains("not JSON")
    );

    let mut pf_arms = base_arms();
    pf_arms.push((r##"#."checks" --apply"##, "", "error: none", 1));
    pf_arms.push((
        "flake'*'prefetch",
        "",
        "error: prefetch failed
",
        1,
    ));
    let res_pf = gate::run(
        &rig.settings(),
        Some(rig.tools(&pf_arms)),
        &Seams::default(),
    );
    assert_eq!(res_pf.exit, Exit::GateFailed);
    assert!(
        res_pf.report.layers[1]
            .reason
            .as_deref()
            .unwrap()
            .contains("prefetch failed")
    );
}

#[test]
fn host_system_wins_over_other_system_when_both_are_listed_and_bootstrap_skips_missing_baseline() {
    let rig = Rig::new();
    let build_json = rig.write_findings(&[
        ("nixfmt", "1.5.0", &[]),
        ("statix", "0.5.8", &[]),
        ("deadnix", "1.3.2", &[("flake.nix", 1)]),
        ("gitleaks", "8.30.1", &[]),
    ]);
    let mut arms = base_arms();
    arms.push((
        r##"#."checks" --apply"##,
        r#"["aarch64-darwin","x86_64-linux"]"#,
        "",
        0,
    ));
    arms.push((
        r##"#."checks"."aarch64-darwin" --apply"##,
        FINDINGS_NAMES,
        "",
        0,
    ));
    arms.push((
        r##"#."checks"."x86_64-linux" --apply"##,
        FINDINGS_NAMES,
        "",
        0,
    ));
    arms.push(("build'*'--no-link", &build_json, "", 0));

    let res = gate::run(&rig.settings(), Some(rig.tools(&arms)), &Seams::default());
    assert_eq!(res.exit, Exit::GateFailed);
    assert_eq!(res.report.lint.as_ref().unwrap().tools.len(), 4);

    // Bootstrap with ratchet and missing baseline succeeds when writing_baseline = true.
    let ratchet_cfg = CONFIG.replace(r#"enforce = "hard""#, r#"enforce = "ratchet""#);
    fs::write(rig.dir.path().join("nix-tops.toml"), ratchet_cfg).unwrap();
    git(rig.dir.path(), &["add", "-A"]);
    git(rig.dir.path(), &["commit", "-q", "-m", "ratchet"]);
    let boot = Settings {
        writing_baseline: true,
        ..rig.settings()
    };
    let res_boot = gate::run(&boot, Some(rig.tools(&arms)), &Seams::default());
    assert_eq!(res_boot.exit, Exit::Pass);
}
