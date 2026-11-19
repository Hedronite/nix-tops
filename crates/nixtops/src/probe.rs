//! Asking the host's `nix` about itself. The binary is `NIXTOPS_NIX` or `nix` on PATH; it is the
//! host's, not the pinned judge.

use serde_json::Value;
use std::process::Command;

/// Flags that make `nix config show` and `nix store info` work on a CppNix with no config.
const COMMANDS: [&str; 2] = ["--extra-experimental-features", "nix-command"];

pub fn nix_bin() -> String {
    std::env::var("NIXTOPS_NIX").unwrap_or_else(|_| "nix".to_string())
}

/// Runs `nix <args>` and returns trimmed stdout, or `None` when it cannot run or exits non-zero.
pub fn run(nix: &str, args: &[&str]) -> Option<String> {
    let out = crate::exec::output(Command::new(nix).args(args)).ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn version_line(nix: &str) -> Option<String> {
    run(nix, &["--version"]).and_then(|text| text.lines().next().map(str::to_string))
}

pub fn store_info(nix: &str) -> Option<String> {
    run(nix, &[COMMANDS[0], COMMANDS[1], "store", "info", "--json"])
}

pub fn config_show(nix: &str) -> Option<String> {
    run(nix, &[COMMANDS[0], COMMANDS[1], "config", "show", "--json"])
}

/// `url version` of the store `nix store info --json` reports (the daemon).
pub fn daemon(info_json: &str) -> Option<String> {
    let info: Value = serde_json::from_str(info_json).ok()?;
    let url = info.get("url")?.as_str()?;
    let version = info
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    Some(format!("{url} {version}"))
}

fn render(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(items) => items.iter().map(render).collect::<Vec<_>>().join(" "),
        other => other.to_string(),
    }
}

/// One setting of `nix config show --json`, or `None` when this Nix has no such setting.
pub fn setting(config_json: &str, name: &str) -> Option<String> {
    let config: Value = serde_json::from_str(config_json).ok()?;
    config.get(name)?.get("value").map(render)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_reads_url_and_version() {
        assert_eq!(
            daemon(r#"{"url":"daemon","version":"2.35.2"}"#).as_deref(),
            Some("daemon 2.35.2")
        );
        assert_eq!(
            daemon(r#"{"url":"daemon"}"#).as_deref(),
            Some("daemon unknown")
        );
        assert_eq!(daemon("{}"), None);
        assert_eq!(daemon("not json"), None);
    }

    #[test]
    fn settings_render_by_kind() {
        let json =
            r#"{"a":{"value":["x","y"]},"b":{"value":true},"c":{"value":"s"},"d":{"value":0}}"#;
        assert_eq!(setting(json, "a").as_deref(), Some("x y"));
        assert_eq!(setting(json, "b").as_deref(), Some("true"));
        assert_eq!(setting(json, "c").as_deref(), Some("s"));
        assert_eq!(setting(json, "d").as_deref(), Some("0"));
        assert_eq!(setting(json, "e"), None);
        assert_eq!(setting("nope", "a"), None);
    }

    #[test]
    fn run_returns_none_for_a_missing_binary_and_trims_output() {
        assert_eq!(run("/nonexistent/nix", &["--version"]), None);
        assert_eq!(run("echo", &["hi"]).as_deref(), Some("hi"));
        assert_eq!(run("false", &[]), None);
    }

    #[test]
    fn nix_bin_defaults_to_nix() {
        if std::env::var_os("NIXTOPS_NIX").is_none() {
            assert_eq!(nix_bin(), "nix");
        }
    }
}
