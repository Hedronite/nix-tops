//! The declared JSON Schema crate does draft 2020-12 and resolves `$ref` into `schema/core` from
//! files embedded in the binary, with no network feature on.

use jsonschema::{Draft, Registry, Resource};
use serde_json::{Value, json};

const BASE: &str = "https://nixtops.invalid/schema/";
const CONFIG_SCHEMA: &str = include_str!("../../../schema/nix-tops.schema.json");
const REPORT_SCHEMA: &str = include_str!("../../../schema/gate-report.schema.json");
const CORE_SCHEMA: &str = include_str!("../../../schema/core/core.schema.json");

fn validator(schema: &str) -> jsonschema::Validator {
    let core: Value = serde_json::from_str(CORE_SCHEMA).unwrap();
    let schema: Value = serde_json::from_str(schema).unwrap();
    let registry = Registry::new()
        .add(
            format!("{BASE}core/core.schema.json"),
            Resource::from_contents(core),
        )
        .unwrap()
        .prepare()
        .unwrap();
    jsonschema::options()
        .with_draft(Draft::Draft202012)
        .with_base_uri(format!("{BASE}x.json"))
        .with_registry(&registry)
        .build(&schema)
        .expect("the schema compiles with its refs resolved from the embedded files")
}

#[test]
fn the_config_schema_accepts_the_quickstart_config() {
    let text = include_str!("../../../examples/quickstart/nix-tops.toml");
    let doc: Value = serde_json::to_value(text.parse::<toml::Table>().unwrap()).unwrap();
    assert!(validator(CONFIG_SCHEMA).is_valid(&doc));
}

#[test]
fn a_value_that_a_core_ref_forbids_is_rejected() {
    let bad = json!({ "protocol": "0.2.0", "facets": ["dev-flake"], "toolchain": { "systems": ["x86_64-linux"] }, "enforce": "sometimes" });
    assert!(!validator(CONFIG_SCHEMA).is_valid(&bad));
    let good = json!({ "protocol": "0.2.0", "facets": ["dev-flake"], "toolchain": { "systems": ["x86_64-linux"] }, "enforce": "hard" });
    assert!(validator(CONFIG_SCHEMA).is_valid(&good));
}

#[test]
fn the_report_schema_compiles_the_same_way() {
    let _ = validator(REPORT_SCHEMA);
}
