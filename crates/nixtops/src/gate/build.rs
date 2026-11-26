//! Gate 4, build and test (protocol section 3.1): `nix flake check` (builds every `checks`
//! derivation) and `nix build` of host `packages`, `devShells`, and configuration build roots,
//! requiring `sandbox = true` on Linux, recording `sandbox` in `report.json`, and marking
//! non-host outputs `deferred`.

use super::system::{attr_system, only_system};
use crate::{
    inventory::{self, Inventory, Level, Session},
    probe,
    report::{Layer, LayerId, OutputEntry, Verdict},
};

/// Inputs needed to run gate 4.
pub struct Input<'a> {
    pub flake_ref: &'a str,
    pub nix_bin: &'a str,
    pub host_system: &'a str,
    pub selected_system: Option<&'a str>,
    pub enforce: &'a str,
    pub ci: bool,
}

/// Result of gate 4: the layer verdict and host Nix `sandbox` setting.
pub struct Outcome {
    pub layer: Layer,
    pub sandbox: Option<String>,
}

struct Targets {
    check_indices: Vec<usize>,
    build_indices: Vec<usize>,
    build_installables: Vec<String>,
}

const SANDBOX_VALUES: [&str; 3] = ["true", "false", "relaxed"];

fn installable(url: &str, path: &[&str]) -> String {
    let quoted: Vec<String> = path
        .iter()
        .map(|name| format!("\"{}\"", name.replace('\\', "\\\\").replace('"', "\\\"")))
        .collect();
    format!("{url}#.{}", quoted.join("."))
}

pub fn normalize_sandbox(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if SANDBOX_VALUES.contains(&trimmed) {
        Ok(trimmed.to_string())
    } else {
        Err(format!(
            "unexpected nix sandbox value {trimmed:?} (expected true, false, or relaxed)"
        ))
    }
}

struct SandboxState {
    sandbox: String,
    fallback_is_false: bool,
}

fn read_sandbox_state(session: &Session, nix_bin: &str) -> Result<SandboxState, String> {
    if let Some(json) = probe::config_show(nix_bin)
        && let Some(val) = probe::setting(&json, "sandbox")
    {
        let sandbox = normalize_sandbox(&val)?;
        let fallback_is_false =
            probe::setting(&json, "sandbox-fallback").as_deref() == Some("false");
        return Ok(SandboxState {
            sandbox,
            fallback_is_false,
        });
    }
    let run = session.judge(&["config", "show", "sandbox"], &[]);
    if run.ok && !run.stdout.trim().is_empty() {
        return Ok(SandboxState {
            sandbox: normalize_sandbox(&run.stdout)?,
            fallback_is_false: false,
        });
    }
    Err("cannot read nix sandbox setting".to_string())
}

pub fn read_sandbox(session: &Session, nix_bin: &str) -> Result<String, String> {
    read_sandbox_state(session, nix_bin).map(|state| state.sandbox)
}

fn ignored_sandbox_fallback(stderr: &str) -> bool {
    stderr.lines().any(|line| {
        line.contains("ignoring the client-specified setting") && line.contains("sandbox-fallback")
    })
}

fn check_build_run(
    run: &inventory::Run,
    label: &str,
    is_linux: bool,
    fallback_is_false: bool,
) -> Result<(), String> {
    if is_linux && !fallback_is_false && ignored_sandbox_fallback(&run.stderr) {
        return Err(
            "cannot enforce sandbox-fallback = false: daemon ignored client-specified setting"
                .to_string(),
        );
    }
    if !run.ok {
        return Err(format!("{label}: {}", first_error(&run.stderr)));
    }
    Ok(())
}

pub fn check_linux_sandbox(host_system: &str, sandbox: &str) -> Result<(), String> {
    if host_system.ends_with("-linux") && sandbox != "true" {
        Err(format!(
            "build-test on {host_system} requires sandbox = true (got {sandbox})"
        ))
    } else {
        Ok(())
    }
}

fn first_error(stderr: &str) -> String {
    let errors: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("error:"))
        .collect();
    if let Some(&last) = errors.last() {
        if last.contains("Invalid BuildResult status from remote")
            && let Some(&prev) = errors[..errors.len() - 1].last()
        {
            return format!("{prev} | {last}");
        }
        return last.to_string();
    }
    stderr
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("nix build failed")
        .trim()
        .to_string()
}

pub fn config_build_root(attr: &str) -> Option<(&str, &str, &[&'static str])> {
    let (top, name) = attr.split_once('.')?;
    match top {
        "homeConfigurations" => Some((top, name, &["activationPackage"])),
        "nixosConfigurations" | "darwinConfigurations" => {
            Some((top, name, &["config", "system", "build", "toplevel"]))
        }
        _ => None,
    }
}

fn eval_config_system(
    session: &Session,
    locked_url: &str,
    top: &str,
    name: &str,
    leaf: &[&str],
) -> Result<String, String> {
    let expr = format!("c: c.{}.system or null", leaf.join("."));
    let val = inventory::eval_json(session, locked_url, &[top, name], &expr)?;
    val.as_str()
        .map(str::to_string)
        .ok_or_else(|| format!("{top}.{name} build root has no string system attribute"))
}

fn populate_outputs_from_inventory(inv: &Inventory, outputs: &mut Vec<OutputEntry>) {
    for r in &inv.records {
        if !r.is_error() && r.level != Level::Listed {
            outputs.push(OutputEntry {
                attr: r.attr.clone(),
                level: r.level.name(),
                deferred: None,
            });
        }
    }
}

fn is_buildable_per_system(attr: &str) -> Option<&str> {
    let top = attr.split('.').next()?;
    match top {
        "checks" | "packages" | "devShells" => attr_system(attr),
        _ => None,
    }
}

fn is_deferrable_per_system(attr: &str) -> Option<&str> {
    let top = attr.split('.').next()?;
    match top {
        "checks" | "packages" | "devShells" | "apps" | "formatter" => attr_system(attr),
        _ => None,
    }
}

fn refuse(reason: String, sandbox: Option<String>) -> Outcome {
    Outcome {
        layer: Layer::new(LayerId::BuildTest, Verdict::Refuse, Some(reason)),
        sandbox,
    }
}

fn ensure_outputs(
    session: &Session,
    input: &Input<'_>,
    outputs: &mut Vec<OutputEntry>,
) -> Result<(), String> {
    if !outputs.is_empty() {
        return Ok(());
    }
    let inv = inventory::collect(session, input.flake_ref)?;
    let collected = only_system(inv, input.selected_system);
    if let Some(broken) = collected.records.iter().find(|r| r.is_error()) {
        return Err(format!(
            "{} failed: {}",
            broken.attr,
            broken.error.as_deref().unwrap_or("evaluation error")
        ));
    }
    populate_outputs_from_inventory(&collected, outputs);
    Ok(())
}

fn resolve_locked_url(
    session: &Session,
    flake_ref: &str,
    host_system: &str,
    outputs: &[OutputEntry],
) -> Result<Option<String>, String> {
    let needed = outputs.iter().any(|o| {
        config_build_root(&o.attr).is_some()
            || (matches!(o.attr.split('.').next(), Some("packages" | "devShells"))
                && attr_system(&o.attr) == Some(host_system))
    });
    if needed {
        inventory::locked_url(session, flake_ref).map(Some)
    } else {
        Ok(None)
    }
}

fn classify_per_system(
    host_system: &str,
    locked_url: Option<&str>,
    idx: usize,
    entry: &mut OutputEntry,
    targets: &mut Targets,
) {
    if let Some(sys) = is_deferrable_per_system(&entry.attr)
        && sys != host_system
    {
        entry.deferred = Some(true);
        return;
    }
    if entry.attr.starts_with("checks.") {
        targets.check_indices.push(idx);
    } else if is_buildable_per_system(&entry.attr) == Some(host_system)
        && let Some(url) = locked_url
    {
        let parts: Vec<&str> = entry.attr.split('.').collect();
        targets.build_installables.push(installable(url, &parts));
        targets.build_indices.push(idx);
    }
}

fn classify_config(
    session: &Session,
    host_system: &str,
    locked_url: Option<&str>,
    idx: usize,
    entry: &mut OutputEntry,
    targets: &mut Targets,
) -> Result<(), String> {
    if let Some((top, name, leaf)) = config_build_root(&entry.attr)
        && let Some(url) = locked_url
    {
        let sys = eval_config_system(session, url, top, name, leaf)?;
        if sys == host_system {
            let mut path = vec![top, name];
            path.extend_from_slice(leaf);
            targets.build_installables.push(installable(url, &path));
            targets.build_indices.push(idx);
        } else {
            entry.deferred = Some(true);
        }
    }
    Ok(())
}

fn partition_targets(
    session: &Session,
    host_system: &str,
    locked_url: Option<&str>,
    outputs: &mut [OutputEntry],
) -> Result<Targets, String> {
    let mut targets = Targets {
        check_indices: Vec::new(),
        build_indices: Vec::new(),
        build_installables: Vec::new(),
    };
    for (idx, entry) in outputs.iter_mut().enumerate() {
        if is_deferrable_per_system(&entry.attr).is_some() {
            classify_per_system(host_system, locked_url, idx, entry, &mut targets);
        } else {
            classify_config(session, host_system, locked_url, idx, entry, &mut targets)?;
        }
    }
    Ok(targets)
}

fn execute_host_builds(
    session: &Session,
    input: &Input<'_>,
    fallback_is_false: bool,
    targets: &Targets,
    outputs: &mut [OutputEntry],
) -> Result<(), String> {
    let is_linux = input.host_system.ends_with("-linux");
    let mut check_args = vec!["--no-update-lock-file"];
    if is_linux {
        check_args.extend(["--option", "sandbox-fallback", "false"]);
    }
    check_args.push(input.flake_ref);
    let check_run = session.judge(&["flake", "check"], &check_args);
    check_build_run(
        &check_run,
        "flake check build failed",
        is_linux,
        fallback_is_false,
    )?;
    for &idx in &targets.check_indices {
        outputs[idx].level = "built";
    }
    if !targets.build_installables.is_empty() {
        let mut args = vec!["--no-link", "--no-update-lock-file"];
        if is_linux {
            args.extend(["--option", "sandbox-fallback", "false"]);
        }
        args.extend(targets.build_installables.iter().map(String::as_str));
        let build_run = session.judge(&["build"], &args);
        check_build_run(&build_run, "build failed", is_linux, fallback_is_false)?;
        for &idx in &targets.build_indices {
            outputs[idx].level = "built";
        }
    }
    Ok(())
}

fn run_build_phase(
    session: &Session,
    input: &Input<'_>,
    fallback_is_false: bool,
    outputs: &mut Vec<OutputEntry>,
) -> Result<Layer, String> {
    ensure_outputs(session, input, outputs)?;
    let locked_url = resolve_locked_url(session, input.flake_ref, input.host_system, outputs)?;
    let targets = partition_targets(session, input.host_system, locked_url.as_deref(), outputs)?;
    if input
        .selected_system
        .is_none_or(|sel| sel == input.host_system)
    {
        execute_host_builds(session, input, fallback_is_false, &targets, outputs)?;
    }
    let deferred_attrs: Vec<&str> = outputs
        .iter()
        .filter(|o| o.deferred == Some(true))
        .map(|o| o.attr.as_str())
        .collect();
    let built_count = outputs.iter().filter(|o| o.level == "built").count();
    Ok(judge_build_layer(
        &deferred_attrs,
        built_count,
        input.ci,
        input.enforce,
    ))
}

/// Runs gate 4 (build-test) and updates `outputs` with `built` / `deferred` status.
pub fn run(session: &Session, input: &Input<'_>, outputs: &mut Vec<OutputEntry>) -> Outcome {
    let state = match read_sandbox_state(session, input.nix_bin) {
        Ok(s) => s,
        Err(reason) => return refuse(reason, None),
    };
    if let Err(reason) = check_linux_sandbox(input.host_system, &state.sandbox) {
        return refuse(reason, Some(state.sandbox));
    }
    match run_build_phase(session, input, state.fallback_is_false, outputs) {
        Ok(layer) => Outcome {
            layer,
            sandbox: Some(state.sandbox),
        },
        Err(reason) => refuse(reason, Some(state.sandbox)),
    }
}

pub fn judge_build_layer(
    deferred_attrs: &[&str],
    built_count: usize,
    ci: bool,
    enforce: &str,
) -> Layer {
    if deferred_attrs.is_empty() {
        return Layer::new(LayerId::BuildTest, Verdict::Pass, None);
    }
    if ci && enforce == "hard" {
        return Layer::new(
            LayerId::BuildTest,
            Verdict::Refuse,
            Some(format!(
                "deferred output(s) in CI under hard: {}",
                deferred_attrs.join(", ")
            )),
        );
    }
    if built_count == 0 {
        return Layer::new(
            LayerId::BuildTest,
            Verdict::Deferred,
            Some(format!(
                "{} output(s) deferred to other systems: {}",
                deferred_attrs.len(),
                deferred_attrs.join(", ")
            )),
        );
    }
    Layer::new(
        LayerId::BuildTest,
        Verdict::Pass,
        Some(format!(
            "{built_count} host output(s) built; {} deferred",
            deferred_attrs.len()
        )),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::{Kind, Record, Structure, Tools};
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    fn script(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        fs::write(&path, body).unwrap();
        let mut perms = fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&path, perms).unwrap();
        path
    }

    #[test]
    fn installable_escapes_segments_cleanly() {
        assert_eq!(
            installable("path:/r", &["packages", "x86_64-linux", "a\"b"]),
            r#"path:/r#."packages"."x86_64-linux"."a\"b""#
        );
    }

    #[test]
    fn resolve_locked_url_queries_only_when_needed() {
        let dir = tempfile::tempdir().unwrap();
        let nix = script(dir.path(), "nix", "#!/bin/sh\nexit 1\n");
        let judge = script(
            dir.path(),
            "judge",
            "#!/bin/sh\necho '{\"url\":\"fake:locked\"}'\n",
        );
        let session = Session::new(Tools {
            judge,
            eval_jobs: nix,
            nixpkgs: "np".into(),
        })
        .unwrap();
        let only_checks = [OutputEntry {
            attr: "checks.aarch64-darwin.unit".into(),
            level: "evaluated",
            deferred: None,
        }];
        assert_eq!(
            resolve_locked_url(&session, ".", "aarch64-darwin", &only_checks).unwrap(),
            None
        );
        let with_pkg = [OutputEntry {
            attr: "packages.aarch64-darwin.default".into(),
            level: "evaluated",
            deferred: None,
        }];
        assert_eq!(
            resolve_locked_url(&session, ".", "aarch64-darwin", &with_pkg).unwrap(),
            Some("fake:locked".into())
        );
    }

    #[test]
    fn read_sandbox_fallback_requires_ok_exit_and_non_empty_stdout() {
        let dir = tempfile::tempdir().unwrap();
        let fail_nix = script(dir.path(), "nix", "#!/bin/sh\nexit 1\n");

        let bad_judge = script(dir.path(), "bad_judge", "#!/bin/sh\necho true\nexit 1\n");
        let empty_ok_judge = script(
            dir.path(),
            "empty_judge",
            "#!/bin/sh\necho \"  \"\nexit 0\n",
        );
        let ok_judge = script(dir.path(), "ok_judge", "#!/bin/sh\necho true\nexit 0\n");

        let mk = |judge| {
            Session::new(Tools {
                judge,
                eval_jobs: fail_nix.clone(),
                nixpkgs: "np".into(),
            })
            .unwrap()
        };
        assert!(read_sandbox(&mk(bad_judge), &fail_nix.display().to_string()).is_err());
        assert!(read_sandbox(&mk(empty_ok_judge), &fail_nix.display().to_string()).is_err());
        assert_eq!(
            read_sandbox(&mk(ok_judge), &fail_nix.display().to_string()).unwrap(),
            "true"
        );
    }

    #[test]
    fn populate_outputs_skips_errors_and_listed_records() {
        let inv = Inventory {
            flake: "f".into(),
            structure: Structure::default(),
            records: vec![
                Record::new(
                    "packages.x86_64-linux.a",
                    Kind::Package,
                    Some("x86_64-linux"),
                    Level::Evaluated,
                ),
                Record::new(
                    "legacyPackages.x86_64-linux",
                    Kind::LegacyPackages,
                    Some("x86_64-linux"),
                    Level::Listed,
                ),
                Record::failed(
                    "packages.x86_64-linux.bad",
                    Kind::Package,
                    Some("x86_64-linux"),
                    "boom".into(),
                ),
            ],
        };
        let mut out = Vec::new();
        populate_outputs_from_inventory(&inv, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].attr, "packages.x86_64-linux.a");
    }

    #[test]
    fn selected_non_host_system_skips_host_builds() {
        let dir = tempfile::tempdir().unwrap();
        let nix = script(dir.path(), "nix", "#!/bin/sh\necho false\n");
        let judge = script(
            dir.path(),
            "judge",
            "#!/bin/sh\ncase \"$*\" in *config*show*sandbox*) echo false; exit 0;; *) echo boom >&2; exit 99;; esac\n",
        );
        let session = Session::new(Tools {
            judge,
            eval_jobs: nix.clone(),
            nixpkgs: "np".into(),
        })
        .unwrap();
        let mut outputs = vec![OutputEntry {
            attr: "packages.x86_64-linux.default".into(),
            level: "evaluated",
            deferred: None,
        }];
        let out = run(
            &session,
            &Input {
                flake_ref: ".",
                nix_bin: &nix.display().to_string(),
                host_system: "aarch64-darwin",
                selected_system: Some("x86_64-linux"),
                enforce: "hard",
                ci: false,
            },
            &mut outputs,
        );
        assert_eq!(out.layer.verdict, Verdict::Deferred);
        assert_eq!(outputs[0].deferred, Some(true));
    }

    #[test]
    fn sandbox_normalization_and_linux_requirement_behave_as_specified() {
        assert_eq!(normalize_sandbox("true").unwrap(), "true");
        assert_eq!(normalize_sandbox(" false\n").unwrap(), "false");
        assert_eq!(normalize_sandbox("relaxed").unwrap(), "relaxed");
        assert!(normalize_sandbox("bogus").is_err());

        assert!(check_linux_sandbox("x86_64-linux", "true").is_ok());
        assert!(check_linux_sandbox("aarch64-linux", "true").is_ok());
        assert!(check_linux_sandbox("x86_64-linux", "false").is_err());
        assert!(check_linux_sandbox("x86_64-linux", "relaxed").is_err());
        assert!(check_linux_sandbox("aarch64-darwin", "false").is_ok());
        assert!(check_linux_sandbox("aarch64-darwin", "relaxed").is_ok());
    }

    #[test]
    fn config_build_root_maps_nixos_darwin_and_home_configurations() {
        assert_eq!(
            config_build_root("nixosConfigurations.host"),
            Some((
                "nixosConfigurations",
                "host",
                &["config", "system", "build", "toplevel"][..]
            ))
        );
        assert_eq!(
            config_build_root("darwinConfigurations.mac"),
            Some((
                "darwinConfigurations",
                "mac",
                &["config", "system", "build", "toplevel"][..]
            ))
        );
        assert_eq!(
            config_build_root("homeConfigurations.user"),
            Some(("homeConfigurations", "user", &["activationPackage"][..]))
        );
        assert_eq!(config_build_root("packages.x86_64-linux.default"), None);
    }

    #[test]
    fn judge_build_layer_handles_local_deferred_mixed_and_ci_hard() {
        let pass = judge_build_layer(&[], 1, true, "hard");
        assert_eq!(pass.verdict, Verdict::Pass);
        assert_eq!(pass.reason, None);

        let local_def = judge_build_layer(&["packages.x86_64-linux.default"], 0, false, "hard");
        assert_eq!(local_def.verdict, Verdict::Deferred);
        assert!(local_def.reason.unwrap().contains("deferred"));

        let local_mixed = judge_build_layer(&["packages.x86_64-linux.default"], 2, false, "hard");
        assert_eq!(local_mixed.verdict, Verdict::Pass);
        assert!(
            local_mixed
                .reason
                .unwrap()
                .contains("2 host output(s) built; 1 deferred")
        );

        let ci_hard = judge_build_layer(&["packages.x86_64-linux.default"], 0, true, "hard");
        assert_eq!(ci_hard.verdict, Verdict::Refuse);
        assert!(ci_hard.reason.unwrap().contains("CI under hard"));

        let ci_ratchet = judge_build_layer(&["packages.x86_64-linux.default"], 0, true, "ratchet");
        assert_eq!(ci_ratchet.verdict, Verdict::Deferred);
    }

    #[test]
    fn per_system_helpers_and_first_error_work() {
        assert_eq!(
            is_buildable_per_system("packages.x86_64-linux.default"),
            Some("x86_64-linux")
        );
        assert_eq!(
            is_buildable_per_system("devShells.aarch64-darwin.default"),
            Some("aarch64-darwin")
        );
        assert_eq!(is_buildable_per_system("apps.x86_64-linux.a"), None);
        assert_eq!(
            is_deferrable_per_system("apps.x86_64-linux.a"),
            Some("x86_64-linux")
        );
        assert_eq!(
            is_deferrable_per_system("formatter.aarch64-darwin"),
            Some("aarch64-darwin")
        );
        assert_eq!(first_error("\n   \nplain error\nline2"), "plain error");
        assert_eq!(first_error("line\nerror: boom\n"), "error: boom");
        assert_eq!(
            first_error(
                "error: builder for '/nix/store/x.drv' failed\nerror: Invalid BuildResult status from remote\n"
            ),
            "error: builder for '/nix/store/x.drv' failed | error: Invalid BuildResult status from remote"
        );
        assert_eq!(
            first_error("noise\nerror: Invalid BuildResult status from remote\n"),
            "error: Invalid BuildResult status from remote"
        );
        assert_eq!(first_error("\n  \n"), "nix build failed");

        assert!(ignored_sandbox_fallback(
            "warning: ignoring the client-specified setting 'sandbox-fallback', because you are not a trusted user"
        ));
        assert!(!ignored_sandbox_fallback(
            "warning: ignoring the client-specified setting 'max-jobs'"
        ));
        assert!(!ignored_sandbox_fallback("sandbox-fallback = false"));

        let ok_run = inventory::Run {
            ok: true,
            stdout: String::new(),
            stderr: String::new(),
        };
        assert!(check_build_run(&ok_run, "build failed", true, false).is_ok());

        let ignored_run = inventory::Run {
            ok: true,
            stdout: String::new(),
            stderr: "warning: ignoring the client-specified setting 'sandbox-fallback'".into(),
        };
        assert!(check_build_run(&ignored_run, "build failed", true, false).is_err());
        assert!(check_build_run(&ignored_run, "build failed", true, true).is_ok());
        assert!(check_build_run(&ignored_run, "build failed", false, false).is_ok());
    }
}
