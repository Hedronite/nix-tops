//! The Nix expressions the inventory evaluates. Strings are escaped as Nix string literals, so a
//! flake reference or an attribute name can never end the literal.

/// Top-level names of a flake value that are metadata, not outputs.
const METADATA: [&str; 12] = [
    "_type",
    "inputs",
    "outputs",
    "lastModified",
    "lastModifiedDate",
    "narHash",
    "outPath",
    "rev",
    "shortRev",
    "revCount",
    "submodules",
    "sourceInfo",
];

/// A Nix string literal for `text`. The backslash goes first so the others are not escaped twice.
pub fn quote(text: &str) -> String {
    let escaped = text
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('$', "\\$")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t");
    format!("\"{escaped}\"")
}

fn list(items: &[&str]) -> String {
    let quoted: Vec<String> = items.iter().map(|item| quote(item)).collect();
    format!("[ {} ]", quoted.join(" "))
}

fn flake(url: &str) -> String {
    format!("(builtins.getFlake {})", quote(url))
}

/// The value at `path` below the flake.
pub fn at(url: &str, path: &[&str]) -> String {
    format!(
        "(builtins.foldl' (a: n: a.${{n}}) {} {})",
        flake(url),
        list(path)
    )
}

/// The names of the flake's outputs.
pub fn output_names(url: &str) -> String {
    format!(
        "builtins.attrNames (removeAttrs {} {})",
        flake(url),
        list(&METADATA)
    )
}

/// The attribute names at `path`.
pub fn attr_names(url: &str, path: &[&str]) -> String {
    format!("builtins.attrNames {}", at(url, path))
}

/// The derivation at `path`: its `.drv` path and name.
pub fn derivation(url: &str, path: &[&str]) -> String {
    let value = at(url, path);
    format!("let d = {value}; in {{ drvPath = d.drvPath; name = d.name or null; }}")
}

/// An app: its type and the program it runs.
pub fn app(url: &str, path: &[&str]) -> String {
    let value = at(url, path);
    format!("let a = {value}; in {{ type = a.type; program = a.program; }}")
}

/// A flake input as a function argument: the locked nixpkgs flake.
fn nixpkgs(reference: &str) -> String {
    flake(reference)
}

/// An overlay applied to the pinned nixpkgs at `system`; the result must evaluate.
pub fn overlay(url: &str, pins: &str, system: &str, path: &[&str]) -> String {
    let (value, np, sys) = (at(url, path), nixpkgs(pins), quote(system));
    let pkgs = format!("import np.outPath {{ system = {sys}; overlays = [ o ]; }}");
    format!(
        "let np = {np}; o = {value}; pkgs = {pkgs}; in {{ attrs = builtins.length (builtins.attrNames (o pkgs pkgs)); }}"
    )
}

/// A NixOS module evaluated in a minimal NixOS configuration of the pinned nixpkgs, with the
/// module system's option check on: a module that sets an option nobody declares fails here. The
/// configuration is `x86_64-linux` (NixOS is Linux; nothing is built), and its build root is forced.
pub fn nixos_module(url: &str, pins: &str, path: &[&str]) -> String {
    let (value, np) = (at(url, path), nixpkgs(pins));
    let base = "{ nixpkgs.hostPlatform = \"x86_64-linux\"; system.stateVersion = \"25.11\"; \
        boot.loader.grub.enable = false; \
        fileSystems.\"/\" = { device = \"none\"; fsType = \"tmpfs\"; }; }";
    let eval = format!("np.lib.nixosSystem {{ modules = [ m {base} ]; }}");
    format!(
        "let np = {np}; m = {value}; e = {eval}; in {{ drvPath = e.config.system.build.toplevel.drvPath; }}"
    )
}

/// What a module export is, forced to a value; used where the kit's lock has no module system for
/// it (Home Manager and nix-darwin modules).
pub fn module_value(url: &str, path: &[&str]) -> String {
    let value = at(url, path);
    format!("let m = {value}; in {{ type = builtins.typeOf m; }}")
}

/// A configuration's build root: the derivation of `leaf` (for example `config.system.build.toplevel`).
pub fn configuration(url: &str, path: &[&str], leaf: &[&str]) -> String {
    let (value, leaf) = (at(url, path), leaf.join("."));
    format!("let c = {value}; in {{ drvPath = c.{leaf}.drvPath; name = c.{leaf}.name or null; }}")
}

/// A template: the path it is copied from and its description.
pub fn template(url: &str, path: &[&str]) -> String {
    let value = at(url, path);
    format!(
        "let t = {value}; in {{ path = toString t.path; description = t.description or null; }}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_escapes_what_could_end_or_open_a_literal() {
        assert_eq!(quote("a\"b"), "\"a\\\"b\"");
        assert_eq!(quote("a\\b"), "\"a\\\\b\"");
        assert_eq!(quote("${x}"), "\"\\${x}\"");
        assert_eq!(quote("a\nb\r\tc"), "\"a\\nb\\r\\tc\"");
        assert_eq!(quote("plain"), "\"plain\"");
    }

    #[test]
    fn a_path_is_a_fold_over_quoted_names() {
        let e = at("github:o/r/abc", &["packages", "x86_64-linux"]);
        assert_eq!(
            e,
            "(builtins.foldl' (a: n: a.${n}) (builtins.getFlake \"github:o/r/abc\") [ \"packages\" \"x86_64-linux\" ])"
        );
        assert!(at("u", &[]).ends_with("[  ])"));
    }

    #[test]
    fn output_names_drop_the_metadata() {
        let e = output_names("u");
        assert!(e.starts_with("builtins.attrNames (removeAttrs (builtins.getFlake \"u\") ["));
        for key in [
            "\"_type\"",
            "\"inputs\"",
            "\"outputs\"",
            "\"sourceInfo\"",
            "\"narHash\"",
        ] {
            assert!(e.contains(key), "{key}");
        }
        assert!(!e.contains("\"packages\""));
    }

    #[test]
    fn each_builder_names_the_thing_it_forces() {
        assert!(attr_names("u", &["a"]).starts_with("builtins.attrNames (builtins.foldl'"));
        assert!(derivation("u", &["a"]).contains("d.drvPath"));
        assert!(app("u", &["a"]).contains("a.program") && app("u", &["a"]).contains("a.type"));
        let o = overlay("u", "np:ref", "x86_64-linux", &["overlays", "default"]);
        assert!(
            o.contains("import np.outPath")
                && o.contains("\"x86_64-linux\"")
                && o.contains("np:ref")
        );
        let m = nixos_module("u", "np:ref", &["nixosModules", "m"]);
        assert!(m.contains("np.lib.nixosSystem") && m.contains("np:ref"));
        assert!(m.contains("hostPlatform = \"x86_64-linux\""));
        assert!(!m.contains("_module.check = false"));
        assert!(m.contains("config.system.build.toplevel.drvPath"));
        assert!(module_value("u", &["homeManagerModules", "m"]).contains("builtins.typeOf m"));
        let c = configuration(
            "u",
            &["nixosConfigurations", "h"],
            &["config", "system", "build", "toplevel"],
        );
        assert!(c.contains("c.config.system.build.toplevel.drvPath"));
        assert!(template("u", &["templates", "t"]).contains("toString t.path"));
    }

    #[test]
    fn a_hostile_name_stays_inside_its_literal() {
        let e = at("u", &["a\" ; builtins.abort \"x"]);
        assert!(e.contains("\\\" ; builtins.abort \\\"x"));
    }
}
