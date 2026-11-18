//! Reads and checks `nix-tops.toml`: the TOML as JSON, the schema failures with their keys, the
//! semantic checks that need no outputs, and the text `init` writes.

pub mod render;
pub mod schema;
pub mod semantic;

use serde_json::Value;

/// One failure, with the key it names (dotted path, `[n]` for an array index).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub key: String,
    pub message: String,
}

impl Problem {
    pub fn new(key: String, message: String) -> Self {
        Self { key, message }
    }
}

fn json_of(value: toml::Value) -> Value {
    match value {
        toml::Value::String(s) => Value::String(s),
        toml::Value::Integer(i) => Value::from(i),
        toml::Value::Float(f) => serde_json::Number::from_f64(f).map_or(Value::Null, Value::Number),
        toml::Value::Boolean(b) => Value::Bool(b),
        // An unquoted TOML date is a datetime, not a string; the schema wants the string.
        toml::Value::Datetime(d) => Value::String(d.to_string()),
        toml::Value::Array(items) => Value::Array(items.into_iter().map(json_of).collect()),
        toml::Value::Table(table) => {
            Value::Object(table.into_iter().map(|(k, v)| (k, json_of(v))).collect())
        }
    }
}

/// The TOML text as a JSON value, or the parse error as a problem on the file.
pub fn parse(text: &str) -> Result<Value, Problem> {
    text.parse::<toml::Table>()
        .map(|table| json_of(toml::Value::Table(table)))
        .map_err(|e| Problem::new("(file)".into(), format!("not valid TOML: {}", e.message())))
}

/// Parse and validate against the embedded schema. Every failure names its key.
pub fn load(text: &str) -> Result<Value, Vec<Problem>> {
    let doc = parse(text).map_err(|p| vec![p])?;
    if doc.get("facet").is_some() {
        return Err(vec![Problem::new(
            "facet".into(),
            "a nix-tops.toml lists facets; facet is the key of a profile file (section 5.3)".into(),
        )]);
    }
    let found = schema::problems(&schema::validator(schema::CONFIG_SCHEMA), &doc);
    if found.is_empty() {
        Ok(doc)
    } else {
        Err(found)
    }
}
