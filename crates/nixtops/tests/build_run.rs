//! Stand-in tests for gate 4 (`gate::build`), `gate::system`, `sandbox`, `deferred`, and
//! schema/text rendering of gate 4 reports.

mod common;

use nixtops::{
    gate::{build, eval, system},
    inventory::{Backend, Inventory, Kind, Level, Record, Session, Structure, Tools},
    report::{
        ALL_LAYERS, BaselineIdentity, Layer, LayerId, OutputEntry, Parts, Report, Verdict,
        render_text,
    },
};
use serde_json::Value;
use std::path::{Path, PathBuf};

type Arm<'a> = (&'a str, &'a str, &'a str, i32);

const REPORT_SCHEMA: &str = include_str!("../../../schema/gate-report.schema.json");
const CORE_SCHEMA: &str = include_str!("../../../schema/core/core.schema.json");

fn script(dir: &Path, name: &str, arms: &[Arm]) -> PathBuf {
    let mut text = String::from("#!/bin/sh\ncase \"$*\" in\n");
    for (pattern, stdout, stderr, code) in arms {
        text.push_str(&format!(
            "*\x27{pattern}\x27*)\ncat <<\x27__OUT__\x27\n{stdout}\n__OUT__\ncat >&2 <<\x27__ERR__\x27\n{stderr}\n__ERR__\nexit {code};;\n"
        ));
    }
    text.push_str("*) echo \"no arm for: $*\" >&2; exit 99;;\nesac\n");
    let path = dir.join(name);
    common::write_executable(&path, &text);
    path
}

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

fn mk_session(
    dir: &Path,
    nix_arms: &[Arm],
    judge_arms: &[Arm],
    nej_arms: &[Arm],
) -> (PathBuf, Session) {
    let nix = script(dir, "nix", nix_arms);
    let judge = script(dir, "judge", judge_arms);
    let eval_jobs = script(dir, "nej", nej_arms);
    let tools = Tools {
        judge,
        eval_jobs,
        nixpkgs: "github:NixOS/nixpkgs/pin".into(),
    };
    let session = Session::new(tools).unwrap();
    (nix, session)
}

fn out(attr: &str) -> OutputEntry {
    OutputEntry {
        attr: attr.to_string(),
        level: "evaluated",
        deferred: None,
    }
}

#[test]
fn linux_requires_sandbox_true_and_reports_validate_against_schema() {
    let dir = tempfile::tempdir().unwrap();
    let (nix, session) = mk_session(
        dir.path(),
        &[(
            "config show --json",
            r#"{"sandbox":{"value":false}}"#,
            "",
            0,
        )],
        &[],
        &[],
    );
    let mut outputs = vec![out("packages.x86_64-linux.default")];
    let input = build::Input {
        flake_ref: ".",
        nix_bin: &nix.display().to_string(),
        host_system: "x86_64-linux",
        selected_system: None,
        enforce: "hard",
        ci: true,
    };
    let outcome = build::run(&session, &input, &mut outputs);
    assert_eq!(outcome.layer.verdict, Verdict::Refuse);
    assert_eq!(outcome.sandbox.as_deref(), Some("false"));
    assert!(
        outcome
            .layer
            .reason
            .as_deref()
            .unwrap()
            .contains("requires sandbox = true")
    );

    let report = Report::finish(Parts {
        protocol: nixtops::build_info::PROTOCOL.into(),
        enforce: "hard".into(),
        baseline_identity: BaselineIdentity {
            implementation: "cppnix".into(),
            version: "2.35.6".into(),
            system: "x86_64-linux".into(),
            lock_hash: "sha256-x".into(),
        },
        facets: vec!["package-set".into()],
        layers: ALL_LAYERS
            .into_iter()
            .map(|id| {
                if id == LayerId::BuildTest {
                    outcome.layer.clone()
                } else {
                    Layer::new(id, Verdict::Pass, None)
                }
            })
            .collect(),
        outputs,
        warnings: vec![],
        acks: vec![],
        lint: None,
    })
    .with_sandbox(outcome.sandbox);

    let json = serde_json::to_value(&report).unwrap();
    assert_valid(&json);
    assert_eq!(json["sandbox"], "false");
    let text = render_text(&json);
    assert!(text.contains("sandbox: false"), "{text}");
}

#[test]
fn builds_host_outputs_configurations_and_defers_other_systems() {
    let dir = tempfile::tempdir().unwrap();
    let (nix, session) = mk_session(
        dir.path(),
        &[(
            "config show --json",
            r#"{"sandbox":{"value":"relaxed"}}"#,
            "",
            0,
        )],
        &[
            ("flake metadata --json", r#"{"url":"fake:locked"}"#, "", 0),
            ("nixosConfigurations", r#""x86_64-linux""#, "", 0),
            ("darwinConfigurations", r#""aarch64-darwin""#, "", 0),
            ("homeConfigurations", r#""aarch64-darwin""#, "", 0),
            ("flake check", "ok", "", 0),
            (
                "build --no-link --no-update-lock-file fake:locked#",
                "",
                "",
                0,
            ),
        ],
        &[],
    );
    let mut outputs = vec![
        out("checks.aarch64-darwin.unit"),
        out("packages.aarch64-darwin.default"),
        out("devShells.aarch64-darwin.default"),
        out("darwinConfigurations.mac"),
        out("homeConfigurations.user"),
        out("packages.x86_64-linux.default"),
        out("nixosConfigurations.vm"),
    ];
    let input = build::Input {
        flake_ref: ".",
        nix_bin: &nix.display().to_string(),
        host_system: "aarch64-darwin",
        selected_system: None,
        enforce: "hard",
        ci: false,
    };
    let outcome = build::run(&session, &input, &mut outputs);
    assert_eq!(
        outcome.layer.verdict,
        Verdict::Pass,
        "{:?}",
        outcome.layer.reason
    );
    assert_eq!(outcome.sandbox.as_deref(), Some("relaxed"));
    for entry in outputs.iter().take(5) {
        assert_eq!(entry.level, "built", "{}", entry.attr);
        assert_eq!(entry.deferred, None);
    }
    assert_eq!(outputs[5].level, "evaluated");
    assert_eq!(outputs[5].deferred, Some(true));
    assert_eq!(outputs[6].level, "evaluated");
    assert_eq!(outputs[6].deferred, Some(true));

    // In CI under hard, the same deferred outputs fail the run.
    let mut ci_outputs = vec![out("packages.x86_64-linux.default")];
    let ci_input = build::Input { ci: true, ..input };
    let ci_outcome = build::run(&session, &ci_input, &mut ci_outputs);
    assert_eq!(ci_outcome.layer.verdict, Verdict::Refuse);
    assert_eq!(ci_outputs[0].deferred, Some(true));
}

#[test]
fn empty_outputs_collects_from_inventory_for_only_build_test_and_propagates_errors() {
    let dir = tempfile::tempdir().unwrap();
    let (nix, mut session) = mk_session(
        dir.path(),
        &[("config show --json", r#"{"sandbox":{"value":true}}"#, "", 0)],
        &[
            ("flake metadata --json", r#"{"url":"fake:locked"}"#, "", 0),
            (r##"#. --apply"##, r#"["packages"]"#, "", 0),
            (r##"#."packages" --apply"##, r#"["aarch64-linux"]"#, "", 0),
            ("flake check", "ok", "", 0),
        ],
        &[(
            "fake:locked#packages.aarch64-linux",
            r#"{"attr":"default","attrPath":["default"],"drvPath":"/nix/store/d.drv","name":"p"}"#,
            "",
            0,
        )],
    );
    session.set_backend(Backend::Jobs);
    let mut outputs = Vec::new();
    let input = build::Input {
        flake_ref: ".",
        nix_bin: &nix.display().to_string(),
        host_system: "aarch64-darwin",
        selected_system: None,
        enforce: "hard",
        ci: false,
    };
    let outcome = build::run(&session, &input, &mut outputs);
    assert_eq!(outcome.layer.verdict, Verdict::Deferred);
    assert_eq!(outputs.len(), 1);
    assert_eq!(outputs[0].deferred, Some(true));

    // Check report text rendering with deferred outputs.
    let report = Report::finish(Parts {
        protocol: nixtops::build_info::PROTOCOL.into(),
        enforce: "hard".into(),
        baseline_identity: BaselineIdentity {
            implementation: "cppnix".into(),
            version: "2.35.6".into(),
            system: "aarch64-darwin".into(),
            lock_hash: "sha256-x".into(),
        },
        facets: vec!["package-set".into()],
        layers: ALL_LAYERS
            .into_iter()
            .map(|id| {
                if id == LayerId::BuildTest {
                    outcome.layer.clone()
                } else {
                    Layer::new(id, Verdict::Pass, None)
                }
            })
            .collect(),
        outputs,
        warnings: vec![],
        acks: vec![],
        lint: None,
    })
    .with_sandbox(outcome.sandbox);
    let json = serde_json::to_value(&report).unwrap();
    assert_valid(&json);
    assert_eq!(json["counts"]["verdicts"]["deferred"], 1);
    let text = render_text(&json);
    assert!(text.contains("1 outputs evaluated (1 deferred)"), "{text}");
    assert!(text.contains("sandbox: true"), "{text}");

    // Ensure q9 message-text does not affect structured check_failed_for_system.
    let inv = Ok(Inventory {
        flake: "fake:locked".into(),
        structure: Structure::default(),
        records: vec![
            Record::failed(
                "checks.aarch64-linux.t",
                Kind::Check,
                Some("aarch64-linux"),
                "error: only 'aarch64-darwin' supported".into(),
            ),
            Record::new(
                "checks.aarch64-darwin.t",
                Kind::Check,
                Some("aarch64-darwin"),
                Level::Evaluated,
            ),
        ],
    });
    assert!(!system::check_failed_for_system(
        false,
        &inv,
        Some("aarch64-darwin")
    ));
    let _ = eval::evaluation_warnings("");
}

#[test]
fn failing_check_or_build_or_unreadable_sandbox_refuses() {
    let dir = tempfile::tempdir().unwrap();
    let (bad_nix, bad_session) = mk_session(
        dir.path(),
        &[("config show --json", "", "err", 1)],
        &[("config show sandbox", "", "err", 1)],
        &[],
    );
    let mut outputs = Vec::new();
    let bad_input = build::Input {
        flake_ref: ".",
        nix_bin: &bad_nix.display().to_string(),
        host_system: "aarch64-darwin",
        selected_system: None,
        enforce: "hard",
        ci: false,
    };
    let r1 = build::run(&bad_session, &bad_input, &mut outputs);
    assert_eq!(r1.layer.verdict, Verdict::Refuse);
    assert_eq!(r1.sandbox, None);

    let (ok_nix, fail_check_session) = mk_session(
        dir.path(),
        &[(
            "config show --json",
            r#"{"sandbox":{"value":false}}"#,
            "",
            0,
        )],
        &[("flake check", "", "error: check failed", 1)],
        &[],
    );
    let mut check_outputs = vec![out("checks.aarch64-darwin.fail")];
    let r2 = build::run(
        &fail_check_session,
        &build::Input {
            nix_bin: &ok_nix.display().to_string(),
            ..bad_input
        },
        &mut check_outputs,
    );
    assert_eq!(r2.layer.verdict, Verdict::Refuse);
    assert!(
        r2.layer
            .reason
            .unwrap()
            .contains("flake check build failed")
    );
    assert_eq!(check_outputs[0].level, "evaluated");

    let (_, fail_pkg_session) = mk_session(
        dir.path(),
        &[(
            "config show --json",
            r#"{"sandbox":{"value":false}}"#,
            "",
            0,
        )],
        &[
            ("flake metadata --json", r#"{"url":"fake:locked"}"#, "", 0),
            ("flake check", "ok", "", 0),
            ("build --no-link", "", "error: builder failed", 1),
        ],
        &[],
    );
    let mut pkg_outputs = vec![out("packages.aarch64-darwin.default")];
    let r3 = build::run(
        &fail_pkg_session,
        &build::Input {
            nix_bin: &ok_nix.display().to_string(),
            ..bad_input
        },
        &mut pkg_outputs,
    );
    assert_eq!(r3.layer.verdict, Verdict::Refuse);
    assert!(r3.layer.reason.unwrap().contains("build failed"));
    assert_eq!(pkg_outputs[0].level, "evaluated");
}
