//! The output inventory: every output of a flake at every system, evaluated with the kit's pinned
//! judge and `nix-eval-jobs` under gate 3's options.
//!
//! [`Structure`] is the cheap part (which outputs and systems exist); the configuration check
//! calls [`derived_facets`] and [`exported_systems`] on it. [`collect`] is the full walk.

mod collect;
mod jobs;
mod nix_expr;
mod record;
mod structure;
mod tools;

pub use collect::{Inventory, Listed, collect, locked_url, read_structure};
pub use record::{COVERAGE, Kind, Level, Record};
pub use structure::{Facet, Shape, Structure, shape_of};
pub use tools::{Run, Session, Tools};

use std::collections::BTreeSet;

/// The facets the flake's outputs show (protocol section 5.1).
pub fn derived_facets(structure: &Structure) -> BTreeSet<Facet> {
    structure.facets()
}

/// The systems the flake's per-system outputs export.
pub fn exported_systems(structure: &Structure) -> BTreeSet<String> {
    structure.systems()
}

/// The structure of `flake_ref` without evaluating any derivation: which outputs exist and which
/// systems they export. An output whose members cannot be listed is kept with no members and its
/// error in `Structure::errors`; the caller decides what that means (the systems of that output
/// are missing from [`exported_systems`]).
pub fn structure_of(session: &Session, flake_ref: &str) -> Result<Structure, String> {
    let url = locked_url(session, flake_ref)?;
    let mut structure = Structure::default();
    for (name, members) in read_structure(session, &url)? {
        match members {
            Ok(found) => structure.insert(&name, found),
            Err(error) => structure.insert_error(&name, error),
        }
    }
    Ok(structure)
}
