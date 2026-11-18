//! One line of the inventory: an output of the flake at one system, with what evaluating it gave.

use serde::Serialize;
use serde_json::Value;

/// The coverage of every output in this milestone: nothing maps outputs to checks yet.
pub const COVERAGE: &str = "not-evaluated";

/// What kind of output this is (protocol section 6.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Package,
    Check,
    DevShell,
    App,
    Formatter,
    LegacyPackages,
    Overlay,
    Template,
    Module,
    Lib,
    Configuration,
    Unknown,
}

/// Every kind with the name used in JSON and in the text table.
const KIND_NAMES: [(Kind, &str); 12] = [
    (Kind::Package, "package"),
    (Kind::Check, "check"),
    (Kind::DevShell, "dev-shell"),
    (Kind::App, "app"),
    (Kind::Formatter, "formatter"),
    (Kind::LegacyPackages, "legacy-packages"),
    (Kind::Overlay, "overlay"),
    (Kind::Template, "template"),
    (Kind::Module, "module"),
    (Kind::Lib, "lib"),
    (Kind::Configuration, "configuration"),
    (Kind::Unknown, "unknown"),
];

impl Kind {
    /// The kebab-case name used in JSON and in the text table.
    pub fn name(self) -> &'static str {
        KIND_NAMES
            .iter()
            .find(|(kind, _)| *kind == self)
            .map_or("unknown", |(_, name)| name)
    }
}

/// How far evaluation got. `Instantiated` is for outputs that are not derivations (section 6.1).
/// `Listed` means the output exists and was not evaluated.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Level {
    Evaluated,
    Instantiated,
    Listed,
    Error,
}

impl Level {
    pub fn name(self) -> &'static str {
        match self {
            Level::Evaluated => "evaluated",
            Level::Instantiated => "instantiated",
            Level::Listed => "listed",
            Level::Error => "error",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Record {
    /// Flake attribute path, for example `packages.x86_64-linux.default`.
    pub attr: String,
    pub kind: Kind,
    pub system: Option<String>,
    pub level: Level,
    pub coverage: &'static str,
    pub drv_path: Option<String>,
    pub name: Option<String>,
    pub meta: Option<Value>,
    pub error: Option<String>,
}

impl Record {
    pub fn new(attr: &str, kind: Kind, system: Option<&str>, level: Level) -> Self {
        Self {
            attr: attr.to_string(),
            kind,
            system: system.map(str::to_string),
            level,
            coverage: COVERAGE,
            drv_path: None,
            name: None,
            meta: None,
            error: None,
        }
    }

    pub fn failed(attr: &str, kind: Kind, system: Option<&str>, error: String) -> Self {
        Self {
            error: Some(error),
            ..Self::new(attr, kind, system, Level::Error)
        }
    }

    pub fn with_drv(self, drv_path: Option<String>, name: Option<String>) -> Self {
        Self {
            drv_path,
            name,
            ..self
        }
    }

    pub fn with_meta(self, meta: Option<Value>) -> Self {
        Self { meta, ..self }
    }

    pub fn is_error(&self) -> bool {
        self.level == Level::Error
    }
}

/// The meta keys the inventory keeps.
const META_KEYS: [&str; 6] = [
    "description",
    "homepage",
    "license",
    "mainProgram",
    "broken",
    "available",
];

/// A stable subset of a derivation's `meta`; `None` when there is nothing to keep.
pub fn prune_meta(meta: &Value) -> Option<Value> {
    let object = meta.as_object()?;
    let kept: serde_json::Map<String, Value> = META_KEYS
        .iter()
        .filter_map(|key| {
            object
                .get(*key)
                .map(|value| ((*key).to_string(), value.clone()))
        })
        .collect();
    (!kept.is_empty()).then_some(Value::Object(kept))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_new_record_is_not_evaluated_for_coverage_and_has_no_error() {
        let r = Record::new("packages.s.p", Kind::Package, Some("s"), Level::Evaluated);
        assert_eq!(r.coverage, "not-evaluated");
        assert!(r.error.is_none() && r.drv_path.is_none() && !r.is_error());
    }

    #[test]
    fn a_failed_record_carries_its_error() {
        let r = Record::failed("a", Kind::Check, None, "boom".into());
        assert!(r.is_error());
        assert_eq!(r.error.as_deref(), Some("boom"));
        assert_eq!(r.system, None);
    }

    #[test]
    fn builders_set_their_field_only() {
        let r = Record::new("a", Kind::App, None, Level::Instantiated)
            .with_drv(Some("/d".into()), Some("n".into()))
            .with_meta(Some(json!({"x": 1})));
        assert_eq!(
            (r.drv_path.as_deref(), r.name.as_deref()),
            (Some("/d"), Some("n"))
        );
        assert_eq!(r.meta, Some(json!({"x": 1})));
        assert_eq!(r.level, Level::Instantiated);
    }

    #[test]
    fn meta_is_pruned_to_the_stable_keys() {
        let meta = json!({"description": "d", "platforms": [1], "license": {"spdxId": "MIT"}, "broken": false});
        assert_eq!(
            prune_meta(&meta),
            Some(json!({"description": "d", "license": {"spdxId": "MIT"}, "broken": false}))
        );
        assert_eq!(prune_meta(&json!({"platforms": []})), None);
        assert_eq!(prune_meta(&json!("not an object")), None);
    }

    #[test]
    fn names_match_the_serialized_forms() {
        let kinds = [
            Kind::Package,
            Kind::Check,
            Kind::DevShell,
            Kind::App,
            Kind::Formatter,
            Kind::LegacyPackages,
            Kind::Overlay,
            Kind::Template,
            Kind::Module,
            Kind::Lib,
            Kind::Configuration,
            Kind::Unknown,
        ];
        for kind in kinds {
            assert_eq!(
                serde_json::to_string(&kind).unwrap(),
                format!("\"{}\"", kind.name())
            );
        }
        for level in [
            Level::Evaluated,
            Level::Instantiated,
            Level::Listed,
            Level::Error,
        ] {
            assert_eq!(
                serde_json::to_string(&level).unwrap(),
                format!("\"{}\"", level.name())
            );
        }
    }

    #[test]
    fn kinds_and_levels_serialize_in_kebab_case() {
        assert_eq!(
            serde_json::to_string(&Kind::DevShell).unwrap(),
            "\"dev-shell\""
        );
        assert_eq!(
            serde_json::to_string(&Kind::LegacyPackages).unwrap(),
            "\"legacy-packages\""
        );
        assert_eq!(
            serde_json::to_string(&Level::Instantiated).unwrap(),
            "\"instantiated\""
        );
    }
}
