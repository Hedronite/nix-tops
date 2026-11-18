//! The text `init` writes: `nix-tops.toml`, the workflow and the `AGENTS.md` section. Everything
//! is a pure function of its inputs (no dates, no hashes), so a second run is byte-identical.

use crate::profiles;

pub const AGENTS_TEMPLATE: &str = include_str!("../../../../config/AGENTS.template.md");
pub const WORKFLOW_TEMPLATE: &str = include_str!("../../../../config/workflow.template.yml");
pub const BEGIN: &str = "<!-- nix-tops:begin -->";
pub const END: &str = "<!-- nix-tops:end -->";

/// The systems the kit supports (section 2); `init` writes them until the inventory lands.
pub const SYSTEMS: [&str; 3] = ["x86_64-linux", "aarch64-linux", "aarch64-darwin"];

fn list(items: &[String]) -> String {
    let quoted: Vec<String> = items.iter().map(|i| format!("\"{i}\"")).collect();
    format!("[{}]", quoted.join(", "))
}

/// `nix-tops.toml` for `facets`, section 9 shape. Layers no selected profile maps are disabled.
pub fn config_toml(facets: &[String], systems: &[String]) -> String {
    let mut out = format!(
        "protocol = \"{}\"\nfacets = {}\nenforce = \"ratchet\"\nbaseline = \".nix-tops/baseline.json\"\nnixpkgs_input = \"nixpkgs\"\n\n[toolchain]\nsystems = {}\n\n[eval]\nall_systems = true\nabort_on_warn = true\nallow_ifd = false\n",
        crate::build_info::PROTOCOL,
        list(facets),
        list(systems)
    );
    let unmapped: Vec<&str> = ["vm_tests", "rebuild", "vulnix"]
        .into_iter()
        .filter(|l| !profiles::maps_layer(facets, l))
        .collect();
    if !unmapped.is_empty() {
        out.push_str("\n[layers]\n");
        for layer in unmapped {
            out.push_str(&format!("{layer} = {{ enabled = false }}\n"));
        }
    }
    out
}

fn number(n: f64) -> String {
    if n.fract() == 0.0 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

/// The `AGENTS.md` section, markers included, with the template's fields filled.
pub fn agents_section(facets: &[String], systems: &[String], enforce: &str) -> String {
    let floors = profiles::floors(facets);
    [
        ("{FACETS}", facets.join(", ")),
        ("{FLAKE}", ".".to_string()),
        ("{SYSTEMS}", systems.join(", ")),
        ("{ENFORCE}", enforce.to_string()),
        ("{BUILT_FLOOR}", number(floors.built)),
        ("{TESTED_FLOOR}", number(floors.tested)),
        ("{CLOSURE_MODE}", floors.closure_mode.to_string()),
        ("{CLOSURE_OUTPUTS}", "every output".to_string()),
    ]
    .iter()
    .fold(AGENTS_TEMPLATE.to_string(), |text, (k, v)| {
        text.replace(k, v)
    })
}

/// Put `section` into `existing`: replace the text between the markers, append when they are
/// absent, or make the file when there is none.
pub fn merge_agents(existing: Option<&str>, section: &str) -> String {
    let section = section.trim_end_matches('\n');
    let Some(text) = existing else {
        return format!("{section}\n");
    };
    if let Some(a) = text.find(BEGIN)
        && let Some(rel) = text[a..].find(END)
    {
        return format!("{}{}{}", &text[..a], section, &text[a + rel + END.len()..]);
    }
    let base = text.trim_end_matches('\n');
    if base.is_empty() {
        format!("{section}\n")
    } else {
        format!("{base}\n\n{section}\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }
    fn s() -> Vec<String> {
        f(&SYSTEMS)
    }

    #[test]
    fn the_config_it_writes_passes_the_schema() {
        for facet in [
            "dev-flake",
            "package-set",
            "nixos-module",
            "flake-lib",
            "nixos-host",
            "home-config",
        ] {
            let text = config_toml(&f(&[facet]), &s());
            assert!(crate::config::load(&text).is_ok(), "{facet}: {text}");
        }
    }

    #[test]
    fn unmapped_layers_are_disabled() {
        let t = config_toml(&f(&["dev-flake"]), &s());
        assert!(
            t.contains("vm_tests = { enabled = false }")
                && t.contains("rebuild = { enabled = false }")
        );
        let t = config_toml(&f(&["package-set"]), &s());
        assert!(!t.contains("rebuild") && t.contains("vm_tests = { enabled = false }"));
    }

    #[test]
    fn the_section_has_no_field_left_and_keeps_its_markers() {
        let t = agents_section(&f(&["package-set"]), &s(), "ratchet");
        assert!(t.starts_with(BEGIN) && t.trim_end().ends_with(END));
        assert!(
            !t.contains("{FACETS}") && !t.contains("{CLOSURE_OUTPUTS}"),
            "a field is left"
        );
        assert!(t.contains("`100%` built, `80%` tested"));
    }

    #[test]
    fn merge_replaces_between_markers_and_keeps_the_rest() {
        let old = format!("# mine\n\n{BEGIN}\nold\n{END}\n\ntail\n");
        let out = merge_agents(Some(&old), &format!("{BEGIN}\nnew\n{END}\n"));
        assert_eq!(out, format!("# mine\n\n{BEGIN}\nnew\n{END}\n\ntail\n"));
        assert_eq!(
            merge_agents(Some(&out), &format!("{BEGIN}\nnew\n{END}\n")),
            out
        );
    }

    #[test]
    fn floors_print_without_a_fraction_when_whole() {
        assert_eq!(number(80.0), "80");
        assert_eq!(number(80.5), "80.5");
    }

    #[test]
    fn an_end_marker_before_the_begin_marker_is_not_a_section() {
        let odd = format!("{END}\nx\n");
        let out = merge_agents(Some(&odd), &format!("{BEGIN}\nn\n{END}\n"));
        assert!(
            out.starts_with(&odd) && out.ends_with(&format!("{BEGIN}\nn\n{END}\n")),
            "{out}"
        );
    }

    #[test]
    fn merge_appends_without_markers_and_creates_without_a_file() {
        let sec = format!("{BEGIN}\nx\n{END}\n");
        assert_eq!(
            merge_agents(Some("# mine\n"), &sec),
            format!("# mine\n\n{BEGIN}\nx\n{END}\n")
        );
        assert_eq!(merge_agents(None, &sec), sec);
    }

    #[test]
    fn the_workflow_is_the_section_11_floor() {
        assert!(WORKFLOW_TEMPLATE.contains("--no-accept-flake-config"));
        assert!(WORKFLOW_TEMPLATE.contains("fetch-depth: 0"));
        assert!(!WORKFLOW_TEMPLATE.contains("pull_request_target:"));
        assert!(WORKFLOW_TEMPLATE.contains("permissions:\n  contents: read"));
    }
}
