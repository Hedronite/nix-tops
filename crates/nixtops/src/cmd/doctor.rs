//! `nixtops doctor`: what the host's Nix is, and the one gate it applies, the daemon floor.

use crate::{
    build_info,
    exit::{Exit, Output},
    host::{self, Floor, HostNix},
    probe,
};
use clap::Args as ClapArgs;
use std::path::Path;

#[derive(Debug, ClapArgs)]
pub struct Args {}

const KVM_DEVICE: &str = "/dev/kvm";
/// Settings only Determinate Nix has; a CppNix host does not list them.
const DETERMINATE_SETTINGS: [&str; 2] = ["lazy-trees", "eval-cores"];

fn judge_line(table: &str, path: &str) -> String {
    match build_info::judge_version(table) {
        Some(version) => format!("judge: CppNix {version} (pinned in the kit's lock) {path}"),
        None => "judge: not pinned (built outside the kit's flake)".to_string(),
    }
}

fn or_unknown(value: Option<String>) -> String {
    value.unwrap_or_else(|| "unknown".to_string())
}

fn settings_lines(config: Option<&str>, kvm: bool) -> Vec<String> {
    let get = |name: &str| config.and_then(|json| probe::setting(json, name));
    let mut lines = vec![
        format!("sandbox: {}", or_unknown(get("sandbox"))),
        format!(
            "experimental-features: {}",
            or_unknown(get("experimental-features"))
        ),
        format!("kvm: {}", if kvm { "yes" } else { "no" }),
        format!("remote builders: {}", or_unknown(get("builders"))),
        format!("substituters: {}", or_unknown(get("substituters"))),
    ];
    for name in DETERMINATE_SETTINGS {
        lines.push(format!(
            "{name}: {}",
            get(name).unwrap_or_else(|| "absent".to_string())
        ));
    }
    lines
}

fn host_line(host: &HostNix) -> String {
    format!(
        "nix: {} {} (reports Nix {})",
        host.implementation.name(),
        host.version,
        host.nix_version
    )
}

fn floor_line(floor: &Floor) -> String {
    match floor {
        Floor::Met => "floor: met (CppNix 2.24 or later, or Determinate Nix 3.x)".to_string(),
        Floor::Below(why) => format!("floor: not met: {why}"),
    }
}

/// Runs the probes against `nix` and renders the report. Exit 3 only below the floor.
pub fn report(nix: &str, kvm: bool) -> Output {
    let Some(host) = probe::version_line(nix)
        .as_deref()
        .and_then(host::parse_version_line)
    else {
        return Output::fail(
            Exit::Environment,
            format!("nixtops doctor: cannot read the version of `{nix}`\n"),
        );
    };
    let floor = host::check_floor(&host);
    let daemon = probe::store_info(nix).as_deref().and_then(probe::daemon);
    let judge = judge_line(build_info::TOOLS, build_info::JUDGE_PATH);
    let mut lines = vec![
        judge,
        host_line(&host),
        format!("daemon: {}", or_unknown(daemon)),
    ];
    lines.extend(settings_lines(probe::config_show(nix).as_deref(), kvm));
    lines.push(floor_line(&floor));
    let text = lines.join("\n") + "\n";
    match floor {
        Floor::Met => Output::ok(text),
        Floor::Below(_) => Output {
            exit: Exit::Environment,
            stdout: text,
            stderr: String::new(),
        },
    }
}

pub fn run(_args: &Args) -> Output {
    report(&probe::nix_bin(), Path::new(KVM_DEVICE).exists())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_judge_line_names_the_pin_or_says_there_is_none() {
        assert_eq!(
            judge_line("nix=2.9.9", "/p/nix"),
            "judge: CppNix 2.9.9 (pinned in the kit's lock) /p/nix"
        );
        assert_eq!(
            judge_line("", "/p/nix"),
            "judge: not pinned (built outside the kit's flake)"
        );
    }

    #[test]
    fn settings_say_unknown_when_the_config_is_unreadable() {
        let lines = settings_lines(None, false);
        assert_eq!(lines[0], "sandbox: unknown");
        assert_eq!(lines[2], "kvm: no");
        assert_eq!(lines[5], "lazy-trees: absent");
        assert_eq!(settings_lines(None, true)[2], "kvm: yes");
    }
}
