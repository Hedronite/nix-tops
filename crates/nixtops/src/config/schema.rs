//! JSON Schema validation from files embedded in the binary. The schemas are read from
//! `schema/` at build time (one source, no copy); `$ref` into `schema/core` resolves from the
//! embedded core file, so nothing is fetched. Public so the report check validates through the
//! same function instead of a second copy.

use super::Problem;
use jsonschema::{
    Draft, Registry, Resource, ValidationError, Validator, error::ValidationErrorKind,
};
use serde_json::Value;

const BASE: &str = "https://nixtops.invalid/schema/";
pub const CONFIG_SCHEMA: &str = include_str!("../../../../schema/nix-tops.schema.json");
pub const REPORT_SCHEMA: &str = include_str!("../../../../schema/gate-report.schema.json");
pub const CORE_SCHEMA: &str = include_str!("../../../../schema/core/core.schema.json");

/// Compile `schema` with `$ref` into `core/core.schema.json` resolved from the embedded file.
pub fn validator(schema: &str) -> Validator {
    let core: Value = serde_json::from_str(CORE_SCHEMA).expect("the core schema is JSON");
    let schema: Value = serde_json::from_str(schema).expect("an embedded schema is JSON");
    let registry = Registry::new()
        .add(
            format!("{BASE}core/core.schema.json"),
            Resource::from_contents(core),
        )
        .expect("the core schema registers")
        .prepare()
        .expect("the registry prepares");
    jsonschema::options()
        .with_draft(Draft::Draft202012)
        .with_base_uri(format!("{BASE}x.json"))
        .with_registry(&registry)
        .build(&schema)
        .expect("an embedded schema compiles with its refs resolved from the embedded files")
}

/// `/toolchain/systems/0` as `toolchain.systems[0]`; the empty path as `(top level)`.
fn dotted(pointer: &str) -> String {
    let mut out = String::new();
    for part in pointer.split('/').filter(|p| !p.is_empty()) {
        if part.chars().all(|c| c.is_ascii_digit()) {
            out.push_str(&format!("[{part}]"));
        } else {
            if !out.is_empty() {
                out.push('.');
            }
            out.push_str(part);
        }
    }
    if out.is_empty() {
        "(top level)".into()
    } else {
        out
    }
}

fn join_key(path: &str, name: &str) -> String {
    if path == "(top level)" {
        name.to_string()
    } else {
        format!("{path}.{name}")
    }
}

fn problems_of(error: &ValidationError<'_>, into: &mut Vec<Problem>) {
    let path = dotted(&error.instance_path().to_string());
    match error.kind() {
        ValidationErrorKind::AdditionalProperties { unexpected } => into.extend(
            unexpected
                .iter()
                .map(|n| Problem::new(join_key(&path, n), "is not a key of the schema".into())),
        ),
        // A oneOf over the two shapes (a configuration, a profile). A file that is a
        // configuration is judged by its own branch, the first.
        ValidationErrorKind::OneOfNotValid { context } => match context.first() {
            Some(branch) => branch.iter().for_each(|e| problems_of(e, into)),
            None => into.push(Problem::new(path, error.to_string())),
        },
        ValidationErrorKind::Required { property } => into.push(Problem::new(
            join_key(&path, property.as_str().unwrap_or("?")),
            "is required".into(),
        )),
        _ => into.push(Problem::new(path, error.to_string())),
    }
}

/// Every schema failure of `doc`, each with the key it names.
pub fn problems(validator: &Validator, doc: &Value) -> Vec<Problem> {
    let mut out = Vec::new();
    for error in validator.iter_errors(doc) {
        problems_of(&error, &mut out);
    }
    out.sort_by(|a, b| (&a.key, &a.message).cmp(&(&b.key, &b.message)));
    out.dedup();
    out
}
