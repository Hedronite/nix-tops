//! System scoping from structured inventory/attribute data (protocol section 3.1, closes M1
//! message-text limits q4 and q9). Never inspects error message text to guess an output system.

use crate::inventory::{Inventory, Kind, Record};

/// Output kinds keyed per system (`<kind>.<system>[.<member>]`).
pub const PER_SYSTEM_KINDS: [&str; 6] = [
    "checks",
    "packages",
    "devShells",
    "apps",
    "formatter",
    "legacyPackages",
];

/// Returns the system level of a per-system flake attribute path, or `None` for flat outputs.
pub fn attr_system(attr: &str) -> Option<&str> {
    let mut parts = attr.split('.');
    let top = parts.next()?;
    let sys = parts.next()?;
    PER_SYSTEM_KINDS.contains(&top).then_some(sys)
}

/// The system a record belongs to: its structured `system` field, or for per-system output kinds,
/// the second component of its attribute path (`packages.<system>.<name>`).
pub fn record_system(record: &Record) -> Option<&str> {
    if record.system.is_some() {
        return record.system.as_deref();
    }
    let per_system = !matches!(
        record.kind,
        Kind::Overlay
            | Kind::Template
            | Kind::Module
            | Kind::Lib
            | Kind::Configuration
            | Kind::Unknown
    );
    per_system.then(|| record.attr.split('.').nth(1)).flatten()
}

/// Whether `record` belongs to `system` (or `system` is `None`). Systemless outputs count for
/// every selection.
pub fn record_concerns_system(record: &Record, system: Option<&str>) -> bool {
    match system {
        None => true,
        Some(wanted) => record_system(record).is_none_or(|s| s == wanted),
    }
}

/// Filters `inv` to keep only records that concern `system`.
pub fn only_system(mut inv: Inventory, system: Option<&str>) -> Inventory {
    if let Some(wanted) = system {
        inv.records
            .retain(|r| record_concerns_system(r, Some(wanted)));
    }
    inv
}

/// Decides from structured `inventory` records whether a `nix flake check` failure concerns
/// `system`.
///
/// - When `run_ok` is true, returns `false`.
/// - When `system` is `None` (`--all-systems`) or `inventory` failed to list structure, returns
///   `true`.
/// - When any error record in `inventory` belongs to `system` (or is systemless), returns `true`.
/// - When `inventory` contains error records exclusively on other systems, returns `false`.
/// - When `inventory` has zero error records despite `flake check` failing, returns `true` (fail
///   closed without guessing from stderr text).
pub fn check_failed_for_system(
    run_ok: bool,
    inventory: &Result<Inventory, String>,
    system: Option<&str>,
) -> bool {
    if run_ok {
        return false;
    }
    let Some(wanted) = system else {
        return true;
    };
    let Ok(inv) = inventory else {
        return true;
    };
    let errors: Vec<&Record> = inv.records.iter().filter(|r| r.is_error()).collect();
    if errors.is_empty() {
        return true;
    }
    errors
        .iter()
        .any(|r| record_concerns_system(r, Some(wanted)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::{Level, Structure};

    fn inv_with(records: Vec<Record>) -> Result<Inventory, String> {
        Ok(Inventory {
            flake: "path:/x".into(),
            structure: Structure::default(),
            records,
        })
    }

    #[test]
    fn attr_and_record_system_classify_per_system_and_flat_outputs() {
        assert_eq!(attr_system("packages.x86_64-linux.a"), Some("x86_64-linux"));
        assert_eq!(
            attr_system("checks.aarch64-darwin.b"),
            Some("aarch64-darwin")
        );
        assert_eq!(
            attr_system("devShells.aarch64-linux.default"),
            Some("aarch64-linux")
        );
        assert_eq!(attr_system("apps.x86_64-linux.cli"), Some("x86_64-linux"));
        assert_eq!(
            attr_system("formatter.aarch64-darwin"),
            Some("aarch64-darwin")
        );
        assert_eq!(
            attr_system("legacyPackages.x86_64-linux"),
            Some("x86_64-linux")
        );
        assert_eq!(attr_system("nixosConfigurations.host"), None);
        assert_eq!(attr_system("overlays.default"), None);
        assert_eq!(attr_system("lib"), None);

        let own = Record::new(
            "packages.aarch64-linux.a",
            Kind::Package,
            Some("x86_64-linux"),
            Level::Evaluated,
        );
        assert_eq!(record_system(&own), Some("x86_64-linux"));
        let derived = Record::new("checks.aarch64-linux.a", Kind::Check, None, Level::Error);
        assert_eq!(record_system(&derived), Some("aarch64-linux"));
        for (attr, kind) in [
            ("overlays.default", Kind::Overlay),
            ("nixosModules.m", Kind::Module),
            ("templates.t", Kind::Template),
            ("lib", Kind::Lib),
            ("nixosConfigurations.h", Kind::Configuration),
            ("fooBar", Kind::Unknown),
        ] {
            assert_eq!(
                record_system(&Record::new(attr, kind, None, Level::Evaluated)),
                None,
                "{attr}"
            );
        }
    }

    #[test]
    fn check_failed_for_system_ignores_errors_confined_to_other_systems_regardless_of_message() {
        let q9_other = Record::failed(
            "packages.aarch64-linux.default",
            Kind::Package,
            Some("aarch64-linux"),
            "error: incompatible 'x86_64-linux' mentioned in message".into(),
        );
        let ok_x86 = Record::new(
            "packages.x86_64-linux.default",
            Kind::Package,
            Some("x86_64-linux"),
            Level::Evaluated,
        );
        let inv = inv_with(vec![q9_other, ok_x86]);
        assert!(!check_failed_for_system(false, &inv, Some("x86_64-linux")));
        assert!(check_failed_for_system(false, &inv, Some("aarch64-linux")));
        assert!(check_failed_for_system(false, &inv, None));
        assert!(!check_failed_for_system(true, &inv, None));

        // Unfiltered inventory error on systemless output or top-level error fails selected system.
        let flat_err = inv_with(vec![Record::failed(
            "overlays.default",
            Kind::Overlay,
            None,
            "broken".into(),
        )]);
        assert!(check_failed_for_system(
            false,
            &flat_err,
            Some("x86_64-linux")
        ));
        assert!(check_failed_for_system(
            false,
            &Err("metadata failed".into()),
            Some("x86_64-linux")
        ));

        // Zero errors in inventory when flake check failed fails closed.
        let clean_inv = inv_with(vec![Record::new(
            "packages.x86_64-linux.default",
            Kind::Package,
            Some("x86_64-linux"),
            Level::Evaluated,
        )]);
        assert!(check_failed_for_system(
            false,
            &clean_inv,
            Some("x86_64-linux")
        ));

        // only_system keeps selected + systemless and drops other systems.
        let filtered = only_system(inv.unwrap(), Some("x86_64-linux"));
        assert_eq!(filtered.records.len(), 1);
        assert_eq!(filtered.records[0].attr, "packages.x86_64-linux.default");
    }
}
