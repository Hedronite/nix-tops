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

fn installable(url: &str, path: &[&str]) -> String {
    let quoted: Vec<String> = path
        .iter()
        .map(|name| format!("\"{}\"", name.replace('\\', "\\\\").replace('"', "\\\"")))
        .collect();
    format!("{url}#.{}", quoted.join("."))
}

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

const SANDBOX_VALUES: [&str; 3] = ["true", "false", "relaxed"];

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

pub fn read_sandbox(session: &Session, nix_bin: &str) -> Result<String, String> {
    if let Some(json) = probe::config_show(nix_bin)
        && let Some(val) = probe::setting(&json, "sandbox")
    {
        return normalize_sandbox(&val);
    }
    let run = session.judge(&["config", "show", "sandbox"], &[]);
    if run.ok && !run.stdout.trim().is_empty() {
        return normalize_sandbox(&run.stdout);
    }
    Err("cannot read nix sandbox setting".to_string())
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
    stderr
        .lines()
        .rev()
        .find(|line| line.trim_start().starts_with("error:"))
        .or_else(|| stderr.lines().find(|line| !line.trim().is_empty()))
        .unwrap_or("nix build failed")
        .trim()
        .to_string()
}

pub fn config_build_root(attr: &str) -> Option<(&str, &str, &'static [&'static str])> {
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

/// Runs gate 4 (build-test) and updates `outputs` with `built` / `deferred` status.
pub fn run(session: &Session, input: &Input<'_>, outputs: &mut Vec<OutputEntry>) -> Outcome {
    let sandbox = match read_sandbox(session, input.nix_bin) {
        Ok(s) => s,
        Err(reason) => return refuse(reason, None),
    };
    if let Err(reason) = check_linux_sandbox(input.host_system, &sandbox) {
        return refuse(reason, Some(sandbox));
    }

    if outputs.is_empty() {
        let collected = match inventory::collect(session, input.flake_ref) {
            Ok(inv) => only_system(inv, input.selected_system),
            Err(reason) => return refuse(reason, Some(sandbox)),
        };
        if let Some(broken) = collected.records.iter().find(|r| r.is_error()) {
            return refuse(
                format!(
                    "{} failed: {}",
                    broken.attr,
                    broken.error.as_deref().unwrap_or("evaluation error")
                ),
                Some(sandbox),
            );
        }
        populate_outputs_from_inventory(&collected, outputs);
    }

    let has_configs = outputs.iter().any(|o| config_build_root(&o.attr).is_some());
    let has_host_packages_or_shells = outputs.iter().any(|o| {
        matches!(o.attr.split('.').next(), Some("packages" | "devShells"))
            && attr_system(&o.attr) == Some(input.host_system)
    });
    let locked_url = if has_configs || has_host_packages_or_shells {
        match inventory::locked_url(session, input.flake_ref) {
            Ok(url) => Some(url),
            Err(reason) => return refuse(reason, Some(sandbox)),
        }
    } else {
        None
    };

    let mut check_indices = Vec::new();
    let mut build_indices = Vec::new();
    let mut build_installables = Vec::new();

    for (idx, entry) in outputs.iter_mut().enumerate() {
        if let Some(sys) = is_deferrable_per_system(&entry.attr) {
            if sys != input.host_system {
                entry.deferred = Some(true);
                continue;
            }
            if entry.attr.starts_with("checks.") {
                check_indices.push(idx);
            } else if is_buildable_per_system(&entry.attr) == Some(input.host_system)
                && let Some(ref url) = locked_url
            {
                let parts: Vec<&str> = entry.attr.split('.').collect();
                build_installables.push(installable(url, &parts));
                build_indices.push(idx);
            }
            continue;
        }
        if let Some((top, name, leaf)) = config_build_root(&entry.attr)
            && let Some(ref url) = locked_url
        {
            match eval_config_system(session, url, top, name, leaf) {
                Ok(sys) if sys == input.host_system => {
                    let mut path = vec![top, name];
                    path.extend_from_slice(leaf);
                    build_installables.push(installable(url, &path));
                    build_indices.push(idx);
                }
                Ok(_) => {
                    entry.deferred = Some(true);
                }
                Err(reason) => return refuse(reason, Some(sandbox)),
            }
        }
    }

    let run_host_builds = input
        .selected_system
        .is_none_or(|sel| sel == input.host_system);

    if run_host_builds {
        let check_run = session.judge(
            &["flake", "check"],
            &["--no-update-lock-file", input.flake_ref],
        );
        if !check_run.ok {
            return refuse(
                format!(
                    "flake check build failed: {}",
                    first_error(&check_run.stderr)
                ),
                Some(sandbox),
            );
        }
        for idx in check_indices {
            outputs[idx].level = "built";
        }
        if !build_installables.is_empty() {
            let mut args = vec!["--no-link", "--no-update-lock-file"];
            let refs: Vec<&str> = build_installables.iter().map(String::as_str).collect();
            args.extend(refs);
            let build_run = session.judge(&["build"], &args);
            if !build_run.ok {
                return refuse(
                    format!("build failed: {}", first_error(&build_run.stderr)),
                    Some(sandbox),
                );
            }
            for idx in build_indices {
                outputs[idx].level = "built";
            }
        }
    }

    let deferred_attrs: Vec<&str> = outputs
        .iter()
        .filter(|o| o.deferred == Some(true))
        .map(|o| o.attr.as_str())
        .collect();
    let built_count = outputs.iter().filter(|o| o.level == "built").count();

    let layer = judge_build_layer(&deferred_attrs, built_count, input.ci, input.enforce);
    Outcome {
        layer,
        sandbox: Some(sandbox),
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

    #[test]
    fn sandbox_normalization_and_linux_requirement_behave_as_specified() {
        assert_eq!(normalize_sandbox("true").unwrap(), "true");
        assert_eq!(
            normalize_sandbox(
                " false
"
            )
            .unwrap(),
            "false"
        );
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
        assert_eq!(
            first_error(
                "line
error: boom
"
            ),
            "error: boom"
        );
        assert_eq!(first_error(""), "nix build failed");
    }
}
