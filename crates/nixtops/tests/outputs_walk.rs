//! The inventory walk with stand-in tools: a shell script for the judge and one for
//! `nix-eval-jobs`, each answering from a table of (pattern, stdout, stderr, exit code).

use nixtops::{
    cmd::outputs::{self, Args},
    exit::Exit,
    inventory::{self, Facet, Session, Tools},
};
use serde_json::Value;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

type Arm<'a> = (&'a str, &'a str, &'a str, i32);

fn script(dir: &Path, name: &str, arms: &[Arm]) -> PathBuf {
    let mut text = String::from("#!/bin/sh\ncase \"$*\" in\n");
    for (pattern, stdout, stderr, code) in arms {
        text.push_str(&format!("*'{pattern}'*)\ncat <<'__OUT__'\n{stdout}\n__OUT__\ncat >&2 <<'__ERR__'\n{stderr}\n__ERR__\nexit {code};;\n"));
    }
    text.push_str("*) echo \"no arm for: $*\" >&2; exit 99;;\nesac\n");
    let path = dir.join(name);
    fs::write(&path, text).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

const NAMES: &str = r#"["apps","checks","devShells","fooBar","formatter","homeConfigurations","legacyPackages","lib","nixosConfigurations","nixosModules","overlays","packages","templates"]"#;

fn judge_arms<'a>(template_dir: &'a str) -> Vec<Arm<'a>> {
    vec![
        ("--no-write-lock-file", "{}", "", 0),
        (
            "--no-update-lock-file",
            r#"{"url":"fake:locked"}"#,
            "warning: unknown setting 'x'",
            0,
        ),
        ("removeAttrs", NAMES, "", 0),
        (
            r#"[ "packages" ]"#,
            r#"["aarch64-darwin","x86_64-linux"]"#,
            "",
            0,
        ),
        (r#"[ "checks" ]"#, r#"["x86_64-linux"]"#, "", 0),
        (r#"[ "devShells" ]"#, r#"["x86_64-linux"]"#, "", 0),
        (r#"[ "apps" ]"#, r#"["x86_64-linux"]"#, "", 0),
        (r#"[ "formatter" ]"#, r#"["x86_64-linux"]"#, "", 0),
        (r#"[ "legacyPackages" ]"#, r#"["x86_64-linux"]"#, "", 0),
        (r#"[ "overlays" ]"#, r#"["bad","default"]"#, "", 0),
        (r#"[ "nixosModules" ]"#, r#"["m"]"#, "", 0),
        (r#"[ "templates" ]"#, r#"["t"]"#, "", 0),
        (r#"[ "nixosConfigurations" ]"#, r#"["h"]"#, "", 0),
        (r#"[ "homeConfigurations" ]"#, r#"["u"]"#, "", 0),
        (
            r#"[ "apps" "x86_64-linux" ]"#,
            r#"["broken","default","noprog"]"#,
            "",
            0,
        ),
        (
            r#"[ "apps" "x86_64-linux" "default" ]"#,
            r#"{"type":"app","program":"/p"}"#,
            "",
            0,
        ),
        (
            r#"[ "apps" "x86_64-linux" "noprog" ]"#,
            r#"{"type":"app"}"#,
            "",
            0,
        ),
        (
            r#"[ "apps" "x86_64-linux" "broken" ]"#,
            r#"{"type":"derivation"}"#,
            "",
            0,
        ),
        (
            r#"[ "formatter" "x86_64-linux" ]"#,
            r#"{"drvPath":"/nix/store/f.drv","name":"nixfmt"}"#,
            "",
            0,
        ),
        (
            r#"[ "overlays" "bad" ]"#,
            "",
            "error: overlay bad\nunpacking 'x'",
            1,
        ),
        (r#"[ "overlays" "default" ]"#, r#"{"attrs":3}"#, "", 0),
        (r#"[ "nixosModules" "m" ]"#, r#"{"options":2}"#, "", 0),
        ("toString t.path", template_dir, "", 0),
        ("c.activationPackage.drvPath", "", "error: no activation", 1),
        (
            "c.config.system.build.toplevel.drvPath",
            r#"{"drvPath":"/nix/store/h.drv","name":"nixos-system"}"#,
            "",
            0,
        ),
    ]
}

const NEJ_ARMS: [Arm; 4] = [
    (
        "fake:locked#packages.x86_64-linux",
        concat!(
            r#"{"attr":"a","attrPath":["a"],"drvPath":"/nix/store/a.drv","name":"a-1","meta":{"description":"A","platforms":["x"]}}"#,
            "\n",
            r#"{"attr":"b","attrPath":["b"],"drvPath":"/nix/store/b.drv","name":"b-1"}"#
        ),
        "",
        0,
    ),
    (
        "fake:locked#packages.aarch64-darwin",
        "",
        "error: container failed",
        1,
    ),
    (
        "fake:locked#checks.x86_64-linux",
        r#"{"attr":"c","attrPath":["c"],"drvPath":"/nix/store/c.drv","name":"c-1"}"#,
        "",
        0,
    ),
    (
        "fake:locked#devShells.x86_64-linux",
        r#"{"attr":"default","attrPath":["default"],"error":"error: shell broke"}"#,
        "",
        0,
    ),
];

struct Rig {
    _dir: tempfile::TempDir,
    tools: Tools,
}

fn rig() -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let template = dir.path().join("tmpl");
    fs::create_dir_all(template.join("sub")).unwrap();
    fs::write(template.join("flake.nix"), "{}").unwrap();
    fs::write(template.join("sub/x"), "x").unwrap();
    let out = format!(r#"{{"path":"{}","description":"d"}}"#, template.display());
    let judge = script(dir.path(), "judge", &judge_arms(&out));
    let eval_jobs = script(dir.path(), "nej", &NEJ_ARMS);
    let tools = Tools {
        judge,
        eval_jobs,
        nixpkgs: "github:NixOS/nixpkgs/pin".to_string(),
    };
    Rig { _dir: dir, tools }
}

fn run_json(tools: Tools) -> Value {
    let out = outputs::run_with(
        tools,
        &Args {
            flake: ".".to_string(),
            json: true,
        },
    );
    assert_eq!(out.exit, Exit::Pass, "{}", out.stderr);
    serde_json::from_str(&out.stdout).unwrap()
}

fn find<'a>(report: &'a Value, attr: &str) -> &'a Value {
    report["outputs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["attr"] == attr)
        .unwrap_or_else(|| panic!("no {attr}"))
}

#[test]
fn every_output_kind_gets_a_record_with_its_level() {
    let r = run_json(rig().tools);
    let level = |attr: &str| find(&r, attr)["level"].as_str().unwrap().to_string();
    assert_eq!(level("packages.x86_64-linux.a"), "evaluated");
    assert_eq!(level("checks.x86_64-linux.c"), "evaluated");
    assert_eq!(level("formatter.x86_64-linux"), "evaluated");
    assert_eq!(level("apps.x86_64-linux.default"), "instantiated");
    assert_eq!(level("overlays.default"), "instantiated");
    assert_eq!(level("nixosModules.m"), "instantiated");
    assert_eq!(level("templates.t"), "instantiated");
    assert_eq!(level("nixosConfigurations.h"), "evaluated");
    assert_eq!(level("legacyPackages.x86_64-linux"), "listed");
    assert_eq!(level("lib"), "listed");
    assert_eq!(level("fooBar"), "listed");
}

#[test]
fn errors_are_kept_per_output_and_the_walk_goes_on() {
    let r = run_json(rig().tools);
    let error = |attr: &str| find(&r, attr)["error"].as_str().unwrap_or("").to_string();
    assert_eq!(error("packages.aarch64-darwin"), "error: container failed");
    assert_eq!(
        error("devShells.x86_64-linux.default"),
        "error: shell broke"
    );
    assert_eq!(error("overlays.bad"), "error: overlay bad");
    assert!(
        error("apps.x86_64-linux.noprog")
            .starts_with("not an app: type is Some(\"app\") and program is false")
    );
    assert!(
        error("apps.x86_64-linux.broken").starts_with("not an app: type is Some(\"derivation\")")
    );
    assert!(error("homeConfigurations.u").contains("no activation"));
    assert_eq!(find(&r, "packages.x86_64-linux.b")["level"], "evaluated");
    assert_eq!(r["summary"]["errors"], 6);
    assert_eq!(
        r["summary"]["outputs"],
        r["outputs"].as_array().unwrap().len()
    );
}

#[test]
fn a_package_record_carries_drv_path_name_system_and_pruned_meta() {
    let r = run_json(rig().tools);
    let a = find(&r, "packages.x86_64-linux.a");
    assert_eq!(a["drv_path"], "/nix/store/a.drv");
    assert_eq!(a["name"], "a-1");
    assert_eq!(a["system"], "x86_64-linux");
    assert_eq!(a["kind"], "package");
    assert_eq!(a["coverage"], "not-evaluated");
    assert_eq!(a["meta"], serde_json::json!({"description": "A"}));
    assert_eq!(
        find(&r, "nixosConfigurations.h")["drv_path"],
        "/nix/store/h.drv"
    );
    assert_eq!(find(&r, "overlays.default")["system"], Value::Null);
}

#[test]
fn the_report_names_the_locked_flake_facets_and_systems() {
    let r = run_json(rig().tools);
    assert_eq!(r["flake"], "fake:locked");
    let facets: Vec<&str> = r["facets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f.as_str().unwrap())
        .collect();
    assert_eq!(
        facets,
        [
            "dev-flake",
            "package-set",
            "nixos-module",
            "flake-lib",
            "nixos-host",
            "home-config"
        ]
    );
    assert_eq!(
        r["systems"],
        serde_json::json!(["aarch64-darwin", "x86_64-linux"])
    );
}

#[test]
fn outputs_come_out_sorted_by_attribute() {
    let r = run_json(rig().tools);
    let attrs: Vec<&str> = r["outputs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["attr"].as_str().unwrap())
        .collect();
    let mut sorted = attrs.clone();
    sorted.sort();
    assert_eq!(attrs, sorted);
}

#[test]
fn the_text_form_has_a_header_a_row_per_output_and_a_count() {
    let rig = rig();
    let out = outputs::run_with(
        rig.tools,
        &Args {
            flake: ".".to_string(),
            json: false,
        },
    );
    assert_eq!(out.exit, Exit::Pass);
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(lines[0], "flake fake:locked");
    assert_eq!(
        lines[1],
        "facets dev-flake, package-set, nixos-module, flake-lib, nixos-host, home-config"
    );
    assert_eq!(lines[2], "systems aarch64-darwin, x86_64-linux");
    assert!(
        out.stdout
            .contains("\nevaluated     package         packages.x86_64-linux.a\n"),
        "{}",
        out.stdout
    );
    assert!(
        out.stdout
            .contains("\nerror         overlay         overlays.bad error: overlay bad\n"),
        "{}",
        out.stdout
    );
    assert!(lines.last().unwrap().ends_with(" outputs, 6 errors"));
}

#[test]
fn an_unreadable_flake_is_an_environment_error() {
    let dir = tempfile::tempdir().unwrap();
    let judge = script(
        dir.path(),
        "judge",
        &[("flake metadata", "", "error: no such flake", 1)],
    );
    let tools = Tools {
        judge,
        eval_jobs: dir.path().join("nej"),
        nixpkgs: "n".into(),
    };
    let out = outputs::run_with(
        tools,
        &Args {
            flake: "nope".into(),
            json: false,
        },
    );
    assert_eq!(out.exit, Exit::Environment);
    assert_eq!(
        out.stderr,
        "nixtops outputs: cannot read the flake: error: no such flake\n"
    );
}

#[test]
fn a_flake_whose_output_names_cannot_be_read_is_an_environment_error() {
    let dir = tempfile::tempdir().unwrap();
    let judge = script(
        dir.path(),
        "judge",
        &[
            ("--no-update-lock-file", r#"{"url":"u"}"#, "", 0),
            ("removeAttrs", "", "error: boom", 1),
        ],
    );
    let tools = Tools {
        judge,
        eval_jobs: dir.path().join("nej"),
        nixpkgs: "n".into(),
    };
    let out = outputs::run_with(
        tools,
        &Args {
            flake: ".".into(),
            json: false,
        },
    );
    assert_eq!(out.exit, Exit::Environment);
    assert!(out.stderr.contains("error: boom"));
}

#[test]
fn metadata_without_a_url_or_with_bad_json_is_an_error() {
    for (stdout, expect) in [("{}", "no url"), ("not json", "not JSON")] {
        let dir = tempfile::tempdir().unwrap();
        let judge = script(
            dir.path(),
            "judge",
            &[("--no-update-lock-file", stdout, "", 0)],
        );
        let tools = Tools {
            judge,
            eval_jobs: dir.path().join("nej"),
            nixpkgs: "n".into(),
        };
        let out = outputs::run_with(
            tools,
            &Args {
                flake: ".".into(),
                json: false,
            },
        );
        assert_eq!(out.exit, Exit::Environment);
        assert!(out.stderr.contains(expect), "{}", out.stderr);
    }
}

#[test]
fn a_member_list_that_is_not_json_becomes_that_outputs_error() {
    let dir = tempfile::tempdir().unwrap();
    let arms: Vec<Arm> = vec![
        ("--no-update-lock-file", r#"{"url":"u"}"#, "", 0),
        ("removeAttrs", r#"["packages"]"#, "", 0),
        (r#"[ "packages" ]"#, "garbage", "", 0),
    ];
    let judge = script(dir.path(), "judge", &arms);
    let tools = Tools {
        judge,
        eval_jobs: dir.path().join("nej"),
        nixpkgs: "n".into(),
    };
    let r = run_json(tools);
    assert_eq!(r["summary"], serde_json::json!({"outputs": 1, "errors": 1}));
    assert!(
        find(&r, "packages")["error"]
            .as_str()
            .unwrap()
            .starts_with("not JSON")
    );
    assert_eq!(find(&r, "packages")["kind"], "package");
}

#[test]
fn an_empty_flake_has_no_outputs_and_no_facets() {
    let dir = tempfile::tempdir().unwrap();
    let arms: Vec<Arm> = vec![
        ("--no-update-lock-file", r#"{"url":"u"}"#, "", 0),
        ("removeAttrs", "[]", "", 0),
    ];
    let tools = Tools {
        judge: script(dir.path(), "judge", &arms),
        eval_jobs: dir.path().join("nej"),
        nixpkgs: "n".into(),
    };
    let r = run_json(tools);
    assert_eq!(r["outputs"], serde_json::json!([]));
    assert_eq!(r["facets"], serde_json::json!([]));
    assert_eq!(r["systems"], serde_json::json!([]));
}

#[test]
fn structure_and_the_derived_sets_are_public_for_the_config_check() {
    let rig = rig();
    let session = Session::new(rig.tools.clone()).unwrap();
    let structure = inventory::structure_of(&session, ".").unwrap();
    let facets = inventory::derived_facets(&structure);
    assert!(facets.contains(&Facet::DevFlake) && facets.contains(&Facet::NixosHost));
    let systems: Vec<String> = inventory::exported_systems(&structure)
        .into_iter()
        .collect();
    assert_eq!(systems, ["aarch64-darwin", "x86_64-linux"]);
    assert_eq!(structure.members["overlays"], ["bad", "default"]);
    assert!(structure.members["lib"].is_empty());
}

#[test]
fn the_judge_gets_gate_three_options_and_an_empty_cache() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("log");
    let body = format!(
        "#!/bin/sh\necho \"$XDG_CACHE_HOME|$*\" >> {}\necho '{{\"url\":\"u\"}}'\n",
        log.display()
    );
    let judge = dir.path().join("judge");
    fs::write(&judge, body).unwrap();
    fs::set_permissions(&judge, fs::Permissions::from_mode(0o755)).unwrap();
    let tools = Tools {
        judge,
        eval_jobs: dir.path().join("nej"),
        nixpkgs: "n".into(),
    };
    let session = Session::new(tools).unwrap();
    let _ = inventory::locked_url(&session, "ref");
    let line = fs::read_to_string(&log).unwrap();
    assert!(line.contains("--extra-experimental-features nix-command flakes flake metadata --json --no-update-lock-file ref --no-accept-flake-config --option abort-on-warn true --option allow-import-from-derivation false"), "{line}");
    let cache = line.split('|').next().unwrap();
    assert!(cache.ends_with("/cache") && !Path::new(cache).join("eval-cache-v6").exists());
}

#[test]
fn eval_jobs_gets_flakes_enabled_by_option_and_gate_three_options() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("log");
    let nej = dir.path().join("nej");
    fs::write(
        &nej,
        format!("#!/bin/sh\necho \"$*\" >> {}\n", log.display()),
    )
    .unwrap();
    fs::set_permissions(&nej, fs::Permissions::from_mode(0o755)).unwrap();
    let tools = Tools {
        judge: dir.path().join("judge"),
        eval_jobs: nej,
        nixpkgs: "n".into(),
    };
    let session = Session::new(tools).unwrap();
    assert!(session.eval_jobs("ref#packages.x").ok);
    let line = fs::read_to_string(&log).unwrap();
    assert_eq!(
        line.trim(),
        "--flake ref#packages.x --meta --workers 1 --option extra-experimental-features nix-command flakes --option abort-on-warn true --option allow-import-from-derivation false"
    );
}
