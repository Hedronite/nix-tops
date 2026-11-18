//! Facet profiles, embedded from `profiles/` at build time. One `include_str!` per file; the
//! test below lists the directory and fails on a file that is not embedded, so there is no
//! second copy and no fallback facet: a facet without a profile has none.

use serde_json::Value;

pub const ALL: [(&str, &str); 6] = [
    (
        "dev-flake",
        include_str!("../../../profiles/dev-flake.toml"),
    ),
    (
        "flake-lib",
        include_str!("../../../profiles/flake-lib.toml"),
    ),
    (
        "home-config",
        include_str!("../../../profiles/home-config.toml"),
    ),
    (
        "nixos-host",
        include_str!("../../../profiles/nixos-host.toml"),
    ),
    (
        "nixos-module",
        include_str!("../../../profiles/nixos-module.toml"),
    ),
    (
        "package-set",
        include_str!("../../../profiles/package-set.toml"),
    ),
];

/// Where a facet stands (section 5.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// A launch facet: floors and laws written, an adopter runs them.
    Launch,
    /// Floors and laws are written and no adopter has run them.
    Unproven,
    /// The facet is in the vocabulary and has no profile.
    NoProfile,
}

pub fn text(facet: &str) -> Option<&'static str> {
    ALL.iter().find(|(name, _)| *name == facet).map(|(_, t)| *t)
}

pub fn status(facet: &str) -> Status {
    match (text(facet), facet) {
        (None, _) => Status::NoProfile,
        (Some(_), "nixos-host" | "home-config") => Status::Unproven,
        (Some(_), _) => Status::Launch,
    }
}

/// A profile as JSON, or `None` when the facet has no profile.
pub fn value(facet: &str) -> Option<Value> {
    text(facet).and_then(|t| crate::config::parse(t).ok())
}

/// The strictest enforcement of the two (`hard` over `ratchet` over `advisory`).
fn stricter(a: &'static str, b: &str) -> &'static str {
    let rank = |m: &str| ["advisory", "ratchet", "hard"].iter().position(|x| *x == m);
    if rank(b) > rank(a) {
        ["advisory", "ratchet", "hard"][rank(b).unwrap_or(0)]
    } else {
        a
    }
}

/// The floors of a set of facets: the highest floor, the strictest closure mode.
#[derive(Debug, PartialEq)]
pub struct Floors {
    pub built: f64,
    pub tested: f64,
    pub closure_mode: &'static str,
}

pub fn floors(facets: &[String]) -> Floors {
    let mut out = Floors {
        built: 0.0,
        tested: 0.0,
        closure_mode: "advisory",
    };
    for profile in facets.iter().filter_map(|f| value(f)) {
        let number = |k: &str| {
            profile
                .pointer(&format!("/coverage/{k}"))
                .and_then(Value::as_f64)
        };
        out.built = out.built.max(number("built_floor").unwrap_or(0.0));
        out.tested = out.tested.max(number("tested_floor").unwrap_or(0.0));
        if let Some(mode) = profile
            .pointer("/budgets/closure/mode")
            .and_then(Value::as_str)
        {
            out.closure_mode = stricter(out.closure_mode, mode);
        }
    }
    out
}

/// Whether some profile in `facets` maps the layer (it has the key and does not disable it).
pub fn maps_layer(facets: &[String], layer: &str) -> bool {
    facets.iter().filter_map(|f| value(f)).any(|p| {
        p.pointer(&format!("/layers/{layer}"))
            .is_some_and(|l| l.get("enabled").and_then(Value::as_bool) != Some(false))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema;
    use std::{fs, path::Path};

    #[test]
    fn every_profile_file_is_embedded() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../profiles");
        let mut on_disk: Vec<String> = fs::read_dir(dir)
            .expect("profiles/ is readable")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        on_disk.sort();
        let embedded: Vec<String> = ALL.iter().map(|(n, _)| format!("{n}.toml")).collect();
        assert_eq!(on_disk, embedded);
    }

    #[test]
    fn each_profile_validates_against_the_schema_and_matches_the_protocol() {
        let v = schema::validator(schema::CONFIG_SCHEMA);
        for (name, text) in ALL {
            let doc = crate::config::parse(text).expect("profile is TOML");
            assert_eq!(schema::problems(&v, &doc), vec![], "{name}");
            assert_eq!(doc["facet"], name);
            assert_eq!(doc["protocol"], crate::build_info::PROTOCOL);
        }
    }

    #[test]
    fn status_follows_section_5_1() {
        assert_eq!(status("dev-flake"), Status::Launch);
        assert_eq!(status("nixos-host"), Status::Unproven);
        assert_eq!(status("home-config"), Status::Unproven);
        for f in ["darwin-host", "fleet-root", "image"] {
            assert_eq!(status(f), Status::NoProfile);
        }
    }

    #[test]
    fn floors_take_the_highest_and_the_strictest() {
        let f = floors(&["dev-flake".into(), "package-set".into()]);
        assert_eq!(f.built, 100.0);
        assert_eq!(f.tested, 80.0);
        assert_eq!(f.closure_mode, "ratchet");
    }

    #[test]
    fn layers_are_mapped_by_the_profiles() {
        assert!(maps_layer(&["package-set".into()], "rebuild"));
        assert!(!maps_layer(&["dev-flake".into()], "rebuild"));
        assert!(maps_layer(&["nixos-module".into()], "vm_tests"));
        assert!(!maps_layer(&["package-set".into()], "vm_tests"));
    }
}
