//! The shape of a flake's outputs: which top-level outputs it exports and for which systems.
//! From it the runner derives the facets (protocol section 5.1) and the exported systems.

use super::record::Kind;
use std::collections::{BTreeMap, BTreeSet};

/// How an output name is laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// `<name>.<system>.<attr>` (or `<name>.<system>` for a formatter).
    PerSystem(Kind),
    /// `<name>.<attr>`, with no system level.
    Flat(Kind),
    /// A name this runner does not know.
    Unknown,
}

const PER_SYSTEM: [(&str, Kind); 6] = [
    ("packages", Kind::Package),
    ("checks", Kind::Check),
    ("devShells", Kind::DevShell),
    ("apps", Kind::App),
    ("formatter", Kind::Formatter),
    ("legacyPackages", Kind::LegacyPackages),
];

const FLAT: [(&str, Kind); 9] = [
    ("overlays", Kind::Overlay),
    ("templates", Kind::Template),
    ("nixosModules", Kind::Module),
    ("homeManagerModules", Kind::Module),
    ("darwinModules", Kind::Module),
    ("lib", Kind::Lib),
    ("nixosConfigurations", Kind::Configuration),
    ("homeConfigurations", Kind::Configuration),
    ("darwinConfigurations", Kind::Configuration),
];

pub fn shape_of(name: &str) -> Shape {
    let per_system = PER_SYSTEM
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, kind)| Shape::PerSystem(*kind));
    let flat = FLAT
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, kind)| Shape::Flat(*kind));
    per_system.or(flat).unwrap_or(Shape::Unknown)
}

/// A facet of section 5.1 that outputs can show. `darwin-host` has no profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Facet {
    DevFlake,
    PackageSet,
    NixosModule,
    FlakeLib,
    NixosHost,
    HomeConfig,
    DarwinHost,
}

impl Facet {
    pub fn as_str(self) -> &'static str {
        match self {
            Facet::DevFlake => "dev-flake",
            Facet::PackageSet => "package-set",
            Facet::NixosModule => "nixos-module",
            Facet::FlakeLib => "flake-lib",
            Facet::NixosHost => "nixos-host",
            Facet::HomeConfig => "home-config",
            Facet::DarwinHost => "darwin-host",
        }
    }
}

/// The top-level output that makes each facet (protocol section 5.1).
const FACET_OF: [(&str, Facet); 10] = [
    ("devShells", Facet::DevFlake),
    ("packages", Facet::PackageSet),
    ("overlays", Facet::PackageSet),
    ("nixosModules", Facet::NixosModule),
    ("homeManagerModules", Facet::NixosModule),
    ("darwinModules", Facet::NixosModule),
    ("lib", Facet::FlakeLib),
    ("nixosConfigurations", Facet::NixosHost),
    ("homeConfigurations", Facet::HomeConfig),
    ("darwinConfigurations", Facet::DarwinHost),
];

/// Top-level output name to its members: the systems for a per-system output, the attribute
/// names for a flat one, and nothing for `lib` and for names the runner does not know.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Structure {
    pub members: BTreeMap<String, Vec<String>>,
}

impl Structure {
    pub fn insert(&mut self, name: &str, members: Vec<String>) {
        self.members.insert(name.to_string(), members);
    }

    /// The facets the outputs show.
    pub fn facets(&self) -> BTreeSet<Facet> {
        let shown = FACET_OF
            .iter()
            .filter(|(name, _)| self.members.contains_key(*name));
        shown.map(|(_, facet)| *facet).collect()
    }

    /// The systems the flake exports: every system key of every per-system output.
    pub fn systems(&self) -> BTreeSet<String> {
        let per_system = |name: &&String| matches!(shape_of(name), Shape::PerSystem(_));
        let keys = self.members.keys().filter(per_system);
        keys.flat_map(|name| self.members[name].iter().cloned())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn structure(entries: &[(&str, &[&str])]) -> Structure {
        let mut s = Structure::default();
        for (name, members) in entries {
            s.insert(name, members.iter().map(|m| m.to_string()).collect());
        }
        s
    }

    #[test]
    fn every_known_name_has_a_shape() {
        assert_eq!(shape_of("packages"), Shape::PerSystem(Kind::Package));
        assert_eq!(shape_of("formatter"), Shape::PerSystem(Kind::Formatter));
        assert_eq!(shape_of("overlays"), Shape::Flat(Kind::Overlay));
        assert_eq!(shape_of("homeManagerModules"), Shape::Flat(Kind::Module));
        assert_eq!(shape_of("lib"), Shape::Flat(Kind::Lib));
        assert_eq!(shape_of("fooBar"), Shape::Unknown);
    }

    #[test]
    fn facets_follow_the_catalog() {
        let s = structure(&[
            ("devShells", &["x"]),
            ("packages", &["x"]),
            ("nixosModules", &["m"]),
            ("lib", &[]),
        ]);
        let names: Vec<&str> = s.facets().into_iter().map(Facet::as_str).collect();
        assert_eq!(
            names,
            ["dev-flake", "package-set", "nixos-module", "flake-lib"]
        );
    }

    #[test]
    fn an_overlay_alone_makes_a_package_set() {
        let s = structure(&[("overlays", &["default"])]);
        assert_eq!(
            s.facets().into_iter().collect::<Vec<_>>(),
            [Facet::PackageSet]
        );
    }

    #[test]
    fn configurations_make_hosts() {
        let s = structure(&[
            ("nixosConfigurations", &["a"]),
            ("homeConfigurations", &["b"]),
            ("darwinConfigurations", &["c"]),
        ]);
        let names: Vec<&str> = s.facets().into_iter().map(Facet::as_str).collect();
        assert_eq!(names, ["nixos-host", "home-config", "darwin-host"]);
    }

    #[test]
    fn a_flake_with_unknown_outputs_only_has_no_facet() {
        assert!(structure(&[("fooBar", &[])]).facets().is_empty());
        assert!(Structure::default().facets().is_empty());
    }

    #[test]
    fn systems_are_the_union_of_per_system_outputs_only() {
        let s = structure(&[
            ("packages", &["x86_64-linux", "aarch64-darwin"]),
            ("devShells", &["x86_64-linux"]),
            ("overlays", &["default"]),
            ("nixosModules", &["m"]),
        ]);
        assert_eq!(
            s.systems().into_iter().collect::<Vec<_>>(),
            ["aarch64-darwin", "x86_64-linux"]
        );
    }

    #[test]
    fn facet_names_are_the_catalog_names() {
        let all = [
            Facet::DevFlake,
            Facet::PackageSet,
            Facet::NixosModule,
            Facet::FlakeLib,
            Facet::NixosHost,
            Facet::HomeConfig,
            Facet::DarwinHost,
        ];
        let names: Vec<&str> = all.iter().map(|f| f.as_str()).collect();
        assert_eq!(
            names,
            [
                "dev-flake",
                "package-set",
                "nixos-module",
                "flake-lib",
                "nixos-host",
                "home-config",
                "darwin-host"
            ]
        );
    }
}
