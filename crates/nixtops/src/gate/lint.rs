//! Gate 2, lint (protocol section 3.1): builds the sandbox findings derivations for nixfmt,
//! statix, deadnix and gitleaks, records per-tool and per-file counts in `report.json`, and
//! judges under the configured enforcement mode (`hard`, `ratchet`, `advisory`).

use crate::{
    inventory::{self, Session},
    report::{Layer, LayerId, LintReport, LintToolReport, Verdict},
};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
};

pub const TOOLS: [&str; 4] = ["nixfmt", "statix", "deadnix", "gitleaks"];

const TOOLS_NIX: &str = include_str!("../../../../nix/tools.nix");
const LIB_DEFAULT_NIX: &str = include_str!("../../../../nix/lib/default.nix");
const MK_CHECKS_NIX: &str = include_str!("../../../../nix/lib/mkChecks.nix");
const PARSE_FINDINGS_PY: &str = include_str!("../../../../nix/lib/parse_findings.py");

/// What gate 2 needs besides the flake.
pub struct Input<'a> {
    pub root: &'a Path,
    pub flake_ref: &'a str,
    pub doc: &'a Value,
    pub enforce: &'a str,
    pub system: &'a str,
    pub writing_baseline: bool,
}

/// Gate 2's result: the layer verdict and the structured tool counts for `report.json`.
pub struct Outcome {
    pub layer: Layer,
    pub lint: Option<LintReport>,
}

fn nix_quote(text: &str) -> String {
    let escaped = text
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('$', "\\$")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t");
    format!("\"{escaped}\"")
}

fn installable(url: &str, path: &[&str]) -> String {
    let quoted: Vec<String> = path
        .iter()
        .map(|name| format!("\"{}\"", name.replace('\\', "\\\\").replace('"', "\\\"")))
        .collect();
    format!("{url}#.{}", quoted.join("."))
}

pub fn findings_attr(tool: &str) -> String {
    format!("nixtops-{tool}-findings")
}

pub fn has_all_findings(attrs: &[String]) -> bool {
    TOOLS
        .iter()
        .all(|tool| attrs.contains(&findings_attr(tool)))
}

fn names_from(value: Result<Value, String>) -> Vec<String> {
    value
        .ok()
        .and_then(|v| v.as_array().cloned())
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect()
}

fn find_lint_system(session: &Session, flake_ref: &str, host_system: &str) -> Option<String> {
    let mut systems = names_from(inventory::eval_json(
        session,
        flake_ref,
        &["checks"],
        "builtins.attrNames",
    ));
    if let Some(pos) = systems.iter().position(|s| s == host_system) {
        systems.swap(0, pos);
    }
    systems.into_iter().find(|sys| {
        let attrs = names_from(inventory::eval_json(
            session,
            flake_ref,
            &["checks", sys],
            "builtins.attrNames",
        ));
        has_all_findings(&attrs)
    })
}

pub fn parse_prefetch(stdout: &str) -> Result<(String, String), String> {
    let value: Value =
        serde_json::from_str(stdout).map_err(|e| format!("nix flake prefetch not JSON: {e}"))?;
    let path = value
        .get("storePath")
        .and_then(Value::as_str)
        .ok_or_else(|| "nix flake prefetch gave no storePath".to_string())?;
    let hash = value
        .get("hash")
        .and_then(Value::as_str)
        .ok_or_else(|| "nix flake prefetch gave no hash".to_string())?;
    Ok((path.to_string(), hash.to_string()))
}

fn prefetch_ref(session: &Session, target: &str) -> Result<(String, String), String> {
    let run = session.judge(&["flake", "prefetch", "--json"], &[target]);
    if !run.ok {
        return Err(first_error(&run.stderr));
    }
    parse_prefetch(&run.stdout)
}

pub fn parse_build_outputs(stdout: &str) -> Result<Vec<PathBuf>, String> {
    let value: Value =
        serde_json::from_str(stdout).map_err(|e| format!("nix build output not JSON: {e}"))?;
    let items = value
        .as_array()
        .ok_or_else(|| "nix build output is not an array".to_string())?;
    let paths: Vec<PathBuf> = items
        .iter()
        .filter_map(|item| item.pointer("/outputs/out").and_then(Value::as_str))
        .map(PathBuf::from)
        .collect();
    if paths.len() != TOOLS.len() {
        return Err(format!(
            "expected {} findings outputs, got {}",
            TOOLS.len(),
            paths.len()
        ));
    }
    Ok(paths)
}

fn run_build(session: &Session, args: &[&str]) -> Result<Vec<PathBuf>, String> {
    let run = session.judge(
        &["build", "--no-link", "--json", "--no-update-lock-file"],
        args,
    );
    if !run.ok {
        return Err(first_error(&run.stderr));
    }
    parse_build_outputs(&run.stdout)
}

fn build_direct(session: &Session, flake_ref: &str, system: &str) -> Result<Vec<PathBuf>, String> {
    let targets: Vec<String> = TOOLS
        .iter()
        .map(|tool| installable(flake_ref, &["checks", system, &findings_attr(tool)]))
        .collect();
    let refs: Vec<&str> = targets.iter().map(String::as_str).collect();
    run_build(session, &refs)
}

pub fn passthru_expr(
    store_path: &str,
    nar_hash: &str,
    other_system: &str,
    host_system: &str,
) -> String {
    let url = nix_quote(&format!("path:{store_path}?narHash={nar_hash}"));
    let other = nix_quote(other_system);
    let host = nix_quote(host_system);
    let list: Vec<String> = TOOLS
        .iter()
        .map(|tool| format!("(chk.{}.passthru.forSystem {host})", findings_attr(tool)))
        .collect();
    format!(
        "let f = builtins.getFlake {url}; chk = f.checks.{other}; in [ {} ]",
        list.join(" ")
    )
}

pub fn kit_lib_expr(
    nixpkgs_ref: &str,
    kit_path: &str,
    kit_hash: &str,
    target_path: &str,
    target_hash: &str,
    host_system: &str,
) -> String {
    let np = nix_quote(nixpkgs_ref);
    let kp = nix_quote(kit_path);
    let kh = nix_quote(kit_hash);
    let tp = nix_quote(target_path);
    let th = nix_quote(target_hash);
    let host = nix_quote(host_system);
    let list: Vec<String> = TOOLS
        .iter()
        .map(|tool| format!("chk.{}", findings_attr(tool)))
        .collect();
    format!(
        "let nixpkgs = builtins.getFlake {np}; \
         kitLib = builtins.fetchTree {{ type = \"path\"; path = {kp}; narHash = {kh}; }}; \
         target = builtins.fetchTree {{ type = \"path\"; path = {tp}; narHash = {th}; }}; \
         mkChecks = import (kitLib + \"/nix/lib/mkChecks.nix\") {{ inherit nixpkgs; }}; \
         chk = (mkChecks {{ self = target; systems = [ {host} ]; exclude = [ ]; }}).{host}; \
         in [ {} ]",
        list.join(" ")
    )
}

fn write_kit_lib(dir: &Path) -> io::Result<()> {
    let lib_dir = dir.join("nix/lib");
    fs::create_dir_all(&lib_dir)?;
    fs::write(dir.join("flake.nix"), "{ outputs = _: {}; }\n")?;
    fs::write(dir.join("nix/tools.nix"), TOOLS_NIX)?;
    fs::write(lib_dir.join("default.nix"), LIB_DEFAULT_NIX)?;
    fs::write(lib_dir.join("mkChecks.nix"), MK_CHECKS_NIX)?;
    fs::write(lib_dir.join("parse_findings.py"), PARSE_FINDINGS_PY)?;
    Ok(())
}

fn build_passthru(
    session: &Session,
    flake_ref: &str,
    other_system: &str,
    host_system: &str,
) -> Result<Vec<PathBuf>, String> {
    let (path, hash) = prefetch_ref(session, flake_ref)?;
    let nix_expr = passthru_expr(&path, &hash, other_system, host_system);
    run_build(session, &["--expr", &nix_expr])
}

fn build_with_kit(
    session: &Session,
    flake_ref: &str,
    host_system: &str,
) -> Result<Vec<PathBuf>, String> {
    let kit_dir = session.scratch("kit-lib");
    write_kit_lib(&kit_dir).map_err(|e| format!("cannot stage kit lib: {e}"))?;
    let (kit_path, kit_hash) = prefetch_ref(session, &kit_dir.display().to_string())?;
    let (target_path, target_hash) = prefetch_ref(session, flake_ref)?;
    let nix_expr = kit_lib_expr(
        session.nixpkgs(),
        &kit_path,
        &kit_hash,
        &target_path,
        &target_hash,
        host_system,
    );
    run_build(session, &["--expr", &nix_expr])
}

fn build_findings(session: &Session, input: &Input<'_>) -> Result<Vec<PathBuf>, String> {
    match find_lint_system(session, input.flake_ref, input.system) {
        Some(sys) if sys == input.system => build_direct(session, input.flake_ref, &sys),
        Some(other) => build_passthru(session, input.flake_ref, &other, input.system),
        None => build_with_kit(session, input.flake_ref, input.system),
    }
}

pub fn parse_findings_json(text: &str) -> Result<LintToolReport, String> {
    let value: Value =
        serde_json::from_str(text).map_err(|e| format!("findings output is not JSON: {e}"))?;
    let tool = value
        .get("tool")
        .and_then(Value::as_str)
        .ok_or_else(|| "findings JSON missing tool".to_string())?;
    let version = value
        .get("version")
        .and_then(Value::as_str)
        .ok_or_else(|| "findings JSON missing version".to_string())?;
    let config_hash = value
        .get("config_hash")
        .and_then(Value::as_str)
        .ok_or_else(|| "findings JSON missing config_hash".to_string())?;
    let raw_files = value
        .get("files")
        .and_then(Value::as_object)
        .ok_or_else(|| "findings JSON missing files".to_string())?;
    let files: BTreeMap<String, usize> = raw_files
        .iter()
        .map(|(path, items)| {
            let count = items.as_array().map_or(0, Vec::len);
            (path.clone(), count)
        })
        .collect();
    Ok(LintToolReport {
        tool: tool.to_string(),
        version: version.to_string(),
        config_hash: config_hash.to_string(),
        files,
    })
}

fn read_reports(paths: &[PathBuf]) -> Result<Vec<LintToolReport>, String> {
    if paths.len() != TOOLS.len() {
        return Err(format!(
            "expected {} findings outputs, got {}",
            TOOLS.len(),
            paths.len()
        ));
    }
    paths
        .iter()
        .map(|path| {
            let text = fs::read_to_string(path)
                .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
            parse_findings_json(&text)
        })
        .collect()
}

pub fn total_findings(tools: &[LintToolReport]) -> usize {
    tools.iter().flat_map(|t| t.files.values()).copied().sum()
}

pub fn findings_summary(tools: &[LintToolReport]) -> String {
    let mut parts = Vec::new();
    for tool in tools {
        for (file, count) in &tool.files {
            if *count > 0 {
                parts.push(format!("{}: {file} ({count})", tool.tool));
            }
        }
    }
    if parts.is_empty() {
        "lint findings detected".to_string()
    } else {
        parts.join("; ")
    }
}

pub fn judge_verdict(input: &Input<'_>, lint: &LintReport) -> Layer {
    let total = total_findings(&lint.tools);
    match input.enforce {
        "ratchet" => {
            match crate::baseline::judge_lint(input.root, input.doc, lint, input.writing_baseline) {
                Ok(()) => Layer::new(LayerId::Lint, Verdict::Pass, None),
                Err(reason) => Layer::new(LayerId::Lint, Verdict::Refuse, Some(reason)),
            }
        }
        "advisory" => {
            let reason = (total > 0).then(|| format!("advisory: {total} finding(s)"));
            Layer::new(LayerId::Lint, Verdict::Pass, reason)
        }
        _ => {
            if total > 0 {
                Layer::new(
                    LayerId::Lint,
                    Verdict::Refuse,
                    Some(findings_summary(&lint.tools)),
                )
            } else {
                Layer::new(LayerId::Lint, Verdict::Pass, None)
            }
        }
    }
}

fn first_error(stderr: &str) -> String {
    stderr
        .lines()
        .rev()
        .find(|line| line.trim_start().starts_with("error:"))
        .or_else(|| stderr.lines().find(|line| !line.trim().is_empty()))
        .unwrap_or("nix lint build failed")
        .trim()
        .to_string()
}

/// Runs gate 2 (lint) over `input.flake_ref` on `input.system`.
pub fn run(session: &Session, input: &Input<'_>) -> Outcome {
    let paths = match build_findings(session, input) {
        Ok(paths) => paths,
        Err(reason) => {
            return Outcome {
                layer: Layer::new(LayerId::Lint, Verdict::Refuse, Some(reason)),
                lint: None,
            };
        }
    };
    let tools = match read_reports(&paths) {
        Ok(tools) => tools,
        Err(reason) => {
            return Outcome {
                layer: Layer::new(LayerId::Lint, Verdict::Refuse, Some(reason)),
                lint: None,
            };
        }
    };
    let report = LintReport {
        system: input.system.to_string(),
        tools,
    };
    let layer = judge_verdict(input, &report);
    Outcome {
        layer,
        lint: Some(report),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_tool(tool: &str, files: &[(&str, usize)]) -> LintToolReport {
        LintToolReport {
            tool: tool.into(),
            version: "1.0".into(),
            config_hash: "h".into(),
            files: files.iter().map(|(k, v)| ((*k).to_string(), *v)).collect(),
        }
    }

    #[test]
    fn nix_quote_and_installable_escape_special_characters() {
        assert_eq!(nix_quote("a\\b\"c$d\ne\rf\tg"), r#""a\\b\"c\$d\ne\rf\tg""#);
        assert_eq!(
            installable("path:/r", &["checks", "s", "a\"b"]),
            r#"path:/r#."checks"."s"."a\"b""#
        );
    }

    #[test]
    fn attr_helpers_cover_all_four_tools() {
        assert_eq!(findings_attr("deadnix"), "nixtops-deadnix-findings");
        let full: Vec<String> = TOOLS.iter().map(|t| findings_attr(t)).collect();
        assert!(has_all_findings(&full));
        assert!(!has_all_findings(&full[..3]));
    }

    #[test]
    fn parse_prefetch_and_build_outputs_extract_expected_fields() {
        let (p, h) = parse_prefetch(r#"{"storePath":"/nix/store/x","hash":"sha256-y"}"#).unwrap();
        assert_eq!((p.as_str(), h.as_str()), ("/nix/store/x", "sha256-y"));
        assert!(parse_prefetch("bad").is_err());
        assert!(parse_prefetch("{}").is_err());
        assert!(parse_prefetch(r#"{"storePath":"/s"}"#).is_err());

        let ok = r#"[{"outputs":{"out":"/a"}},{"outputs":{"out":"/b"}},{"outputs":{"out":"/c"}},{"outputs":{"out":"/d"}}]"#;
        assert_eq!(
            parse_build_outputs(ok).unwrap(),
            vec![
                PathBuf::from("/a"),
                PathBuf::from("/b"),
                PathBuf::from("/c"),
                PathBuf::from("/d"),
            ]
        );
        assert!(parse_build_outputs("[]").is_err());
        assert!(parse_build_outputs("{}").is_err());
        assert!(parse_build_outputs("not json").is_err());
    }

    #[test]
    fn expr_builders_quote_arguments_safely() {
        let p = passthru_expr("/nix/store/a", "sha256-b", "x86_64-linux", "aarch64-darwin");
        assert!(p.contains("path:/nix/store/a?narHash=sha256-b"));
        assert!(p.contains("passthru.forSystem \"aarch64-darwin\""));
        assert!(p.contains("f.checks.\"x86_64-linux\""));

        let k = kit_lib_expr("np", "/k", "kh", "/t", "th", "aarch64-darwin");
        assert!(k.contains("builtins.getFlake \"np\""));
        assert!(k.contains("systems = [ \"aarch64-darwin\" ]"));
        assert!(k.contains("chk.nixtops-gitleaks-findings"));
    }

    #[test]
    fn parse_findings_json_counts_per_file_entries() {
        let text = r#"{"tool":"deadnix","version":"9.9.9","config_hash":"abc","files":{"a.nix":[{"line":1},{"line":2}],"b.nix":[]}}"#;
        let parsed = parse_findings_json(text).unwrap();
        assert_eq!(parsed.tool, "deadnix");
        assert_eq!(parsed.version, "9.9.9");
        assert_eq!(parsed.config_hash, "abc");
        assert_eq!(parsed.files.get("a.nix"), Some(&2));
        assert_eq!(parsed.files.get("b.nix"), Some(&0));

        assert!(parse_findings_json("bad").is_err());
        assert!(parse_findings_json(r#"{"version":"1"}"#).is_err());
    }

    #[test]
    fn judge_verdict_enforces_hard_ratchet_and_advisory() {
        let doc = json!({});
        let clean = LintReport {
            system: "x86_64-linux".into(),
            tools: vec![sample_tool("deadnix", &[])],
        };
        let dirty = LintReport {
            system: "x86_64-linux".into(),
            tools: vec![sample_tool("deadnix", &[("flake.nix", 1)])],
        };
        let input_hard = Input {
            root: Path::new("."),
            flake_ref: ".",
            doc: &doc,
            enforce: "hard",
            system: "x86_64-linux",
            writing_baseline: false,
        };
        assert_eq!(judge_verdict(&input_hard, &clean).verdict, Verdict::Pass);
        let refused = judge_verdict(&input_hard, &dirty);
        assert_eq!(refused.verdict, Verdict::Refuse);
        assert_eq!(refused.reason.as_deref(), Some("deadnix: flake.nix (1)"));

        let input_adv = Input {
            enforce: "advisory",
            ..input_hard
        };
        let adv = judge_verdict(&input_adv, &dirty);
        assert_eq!(adv.verdict, Verdict::Pass);
        assert_eq!(adv.reason.as_deref(), Some("advisory: 1 finding(s)"));
        assert_eq!(judge_verdict(&input_adv, &clean).reason, None);

        let input_ratchet = Input {
            enforce: "ratchet",
            ..input_hard
        };
        assert_eq!(judge_verdict(&input_ratchet, &dirty).verdict, Verdict::Pass);
    }

    #[test]
    fn write_kit_lib_populates_all_required_nix_files() {
        let dir = tempfile::tempdir().unwrap();
        write_kit_lib(dir.path()).unwrap();
        for rel in [
            "flake.nix",
            "nix/tools.nix",
            "nix/lib/default.nix",
            "nix/lib/mkChecks.nix",
            "nix/lib/parse_findings.py",
        ] {
            assert!(dir.path().join(rel).is_file(), "missing {rel}");
        }
    }

    #[test]
    fn summary_and_error_helpers_handle_empty_and_non_empty() {
        let mixed = [
            sample_tool("statix", &[("a.nix", 0), ("b.nix", 2)]),
            sample_tool("deadnix", &[("c.nix", 1)]),
        ];
        assert_eq!(
            findings_summary(&mixed),
            "statix: b.nix (2); deadnix: c.nix (1)"
        );
        assert_eq!(
            findings_summary(&[sample_tool("statix", &[("a.nix", 0)])]),
            "lint findings detected"
        );
        assert_eq!(first_error("line1\nerror: boom\n"), "error: boom");
        assert_eq!(first_error("plain\n"), "plain");
        assert_eq!(first_error("\n"), "nix lint build failed");
    }
}
