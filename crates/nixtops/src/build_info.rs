//! Values fixed at build time. Nothing here is typed by hand: the protocol id is read from the
//! schema by `build.rs`, the package version is Cargo's, and the tool table and the judge path
//! come from the kit's flake (`NIXTOPS_TOOLS`, `NIXTOPS_JUDGE`). A build outside the flake has
//! empty tool values and says so.

pub const PROTOCOL: &str = env!("NIXTOPS_PROTOCOL");
pub const PACKAGE: &str = env!("CARGO_PKG_VERSION");
pub const TOOLS: &str = env!("NIXTOPS_TOOLS");
pub const JUDGE_PATH: &str = env!("NIXTOPS_JUDGE");

/// Tool name for the pinned CppNix judge in the tool table.
pub const JUDGE_TOOL: &str = "nix";

/// `name=version;name=version` into pairs. Entries without `=` are dropped.
pub fn parse_tools(table: &str) -> Vec<(&str, &str)> {
    table
        .split(';')
        .filter_map(|entry| entry.split_once('='))
        .collect()
}

/// The version of the pinned CppNix judge in a tool table, if it lists one.
pub fn judge_version(table: &str) -> Option<&str> {
    parse_tools(table)
        .into_iter()
        .find(|(name, _)| *name == JUDGE_TOOL)
        .map(|(_, version)| version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_table_parses_in_order_and_drops_malformed_entries() {
        assert_eq!(
            parse_tools("a=1;b=2;;junk;c=3.4"),
            [("a", "1"), ("b", "2"), ("c", "3.4")]
        );
        assert!(parse_tools("").is_empty());
    }

    #[test]
    fn the_judge_is_the_entry_named_nix() {
        assert_eq!(judge_version("a=1;nix=2.9.9;b=3"), Some("2.9.9"));
        assert_eq!(judge_version("a=1;b=3"), None);
        assert_eq!(judge_version(""), None);
    }

    #[test]
    fn the_protocol_and_package_are_set() {
        assert!(!PROTOCOL.is_empty());
        assert_eq!(PACKAGE, env!("CARGO_PKG_VERSION"));
    }
}
