//! The text form of a report: one line per layer, the lint tools, the warnings, the verdict of
//! the run. It reads the report as JSON so `nixtops report` can render a file written by an
//! earlier run.

use serde_json::Value;

fn str_of<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

fn layer_line(layer: &Value) -> String {
    let (id, verdict) = (str_of(layer, "id"), str_of(layer, "verdict"));
    match layer.get("reason").and_then(Value::as_str) {
        Some(reason) => format!("{id:<13} {verdict:<9} {}", first_line(reason)),
        None => format!("{id:<13} {verdict}"),
    }
}

fn first_line(text: &str) -> &str {
    text.lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim()
}

fn warning_line(warning: &Value) -> String {
    let message = first_line(str_of(warning, "message"));
    format!(
        "  {} [{}] {message}",
        str_of(warning, "verdict"),
        str_of(warning, "source")
    )
}

fn items<'a>(report: &'a Value, key: &str) -> impl Iterator<Item = &'a Value> {
    report
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

fn lint_lines(report: &Value) -> Vec<String> {
    let Some(lint) = report.get("lint").and_then(Value::as_object) else {
        return Vec::new();
    };
    let system = lint.get("system").and_then(Value::as_str).unwrap_or("");
    let mut out = vec![format!("lint ({system})")];
    for tool in lint
        .get("tools")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let name = str_of(tool, "tool");
        let version = str_of(tool, "version");
        let count: usize = tool
            .get("files")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
            .filter_map(|(_, v)| v.as_u64())
            .sum::<u64>() as usize;
        out.push(format!("  {name} {version}: {count} finding(s)"));
    }
    out
}

/// The laws layer is `skip`: no law was judged, and the text says so.
fn laws_switched_off(report: &Value) -> bool {
    items(report, "layers").any(|l| str_of(l, "id") == "laws" && str_of(l, "verdict") == "skip")
}

/// The report as text.
pub fn render_text(report: &Value) -> String {
    let mut lines: Vec<String> = items(report, "layers").map(layer_line).collect();
    if str_of(report, "enforce") == "advisory" {
        lines.push("run is not enforced: enforce = advisory".to_string());
    }
    if laws_switched_off(report) {
        lines.push("laws are not enforced in this run: the laws layer is switched off".to_string());
    }
    lines.extend(lint_lines(report));
    let warnings: Vec<String> = items(report, "warnings").map(warning_line).collect();
    if !warnings.is_empty() {
        lines.push("warnings".to_string());
        lines.extend(warnings);
    }
    let outputs = items(report, "outputs").count();
    lines.push(format!("{outputs} outputs evaluated"));
    let passed = report
        .get("passed")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    lines.push(format!(
        "gate: {}",
        if passed { "passed" } else { "failed" }
    ));
    lines.join("\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_layer_line_pads_the_id_and_shows_the_first_line_of_the_reason() {
        let report = json!({"layers": [
            {"id": "source", "verdict": "pass"},
            {"id": "eval", "verdict": "Refuse", "reason": "\nfirst\nsecond"}
        ], "passed": false});
        let text = render_text(&report);
        assert!(text.starts_with("source        pass\n"), "{text}");
        assert!(text.contains("eval          Refuse    first\n"), "{text}");
        assert!(text.ends_with("0 outputs evaluated\ngate: failed\n"));
    }

    #[test]
    fn warnings_are_listed_under_a_heading() {
        let report = json!({"layers": [], "passed": true, "outputs": [{"attr": "a"}],
            "warnings": [{"source": "builtins-warn", "message": "x\ny", "verdict": "Ack"}]});
        assert_eq!(
            render_text(&report),
            "warnings\n  Ack [builtins-warn] x\n1 outputs evaluated\ngate: passed\n"
        );
    }

    #[test]
    fn lint_summary_lists_each_tool_and_count() {
        let report = json!({
            "layers": [],
            "passed": true,
            "lint": {
                "system": "x86_64-linux",
                "tools": [
                    {"tool": "deadnix", "version": "9.9.9", "config_hash": "h", "files": {"flake.nix": 2}}
                ]
            }
        });
        let text = render_text(&report);
        assert!(
            text.contains("lint (x86_64-linux)\n  deadnix 9.9.9: 2 finding(s)\n"),
            "{text}"
        );
    }

    #[test]
    fn a_report_with_the_laws_layer_switched_off_says_laws_are_not_enforced() {
        let off =
            json!({"layers": [{"id": "laws", "verdict": "skip", "reason": "r"}], "passed": true});
        assert!(
            render_text(&off)
                .contains("\nlaws are not enforced in this run: the laws layer is switched off\n")
        );
        let skipped_other =
            json!({"layers": [{"id": "lint", "verdict": "skip", "reason": "r"}], "passed": true});
        assert!(!render_text(&skipped_other).contains("not enforced"));
        let laws_on = json!({"layers": [{"id": "laws", "verdict": "not-run", "reason": "r"}], "passed": false});
        assert!(!render_text(&laws_on).contains("not enforced"));
    }

    #[test]
    fn advisory_report_says_run_is_not_enforced() {
        let adv = json!({"enforce": "advisory", "layers": [], "passed": false});
        assert!(render_text(&adv).contains(
            "run is not enforced: enforce = advisory
"
        ));
        let hard = json!({"enforce": "hard", "layers": [], "passed": false});
        assert!(!render_text(&hard).contains("not enforced"));
    }

    #[test]
    fn a_missing_field_does_not_panic() {
        assert_eq!(
            render_text(&json!({})),
            "0 outputs evaluated\ngate: failed\n"
        );
    }
}
