//! The Nix expressions the inventory evaluates. Strings are escaped as Nix string literals, so a
//! flake reference or an attribute name can never end the literal.

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

fn flake(reference: &str) -> String {
    format!("(builtins.getFlake {})", quote(reference))
}

/// An attribute path for `nix eval <url>#<path>`: every name quoted, `.` for the flake itself.
pub fn attr_path(path: &[&str]) -> String {
    if path.is_empty() {
        return ".".to_string();
    }
    let quoted: Vec<String> = path
        .iter()
        .map(|name| format!("\"{}\"", name.replace('\\', "\\\\").replace('"', "\\\"")))
        .collect();
    quoted.join(".")
}

/// The installable for the value at `path` below the flake at `url`.
pub fn installable(url: &str, path: &[&str]) -> String {
    format!("{url}#{}", attr_path(path))
}

/// The names of an attribute set (the outputs of the flake at the root, or any set below it).
pub fn names() -> String {
    "builtins.attrNames".to_string()
}

/// A derivation: its `.drv` path, name and a stable subset of its meta.
pub fn derivation() -> String {
    let keep = "{ description = null; homepage = null; license = null; mainProgram = null; broken = null; available = null; }";
    format!(
        "d: {{ drvPath = d.drvPath; name = d.name or null; meta = builtins.intersectAttrs {keep} (d.meta or {{ }}); }}"
    )
}

/// The `warnings` list of a configuration (NixOS, Home Manager, nix-darwin), as data.
pub fn config_warnings() -> String {
    "c: c.config.warnings".to_string()
}

/// An app: its type and the program it runs.
pub fn app() -> String {
    "a: { type = a.type; program = a.program; }".to_string()
}

/// An overlay applied to the pinned nixpkgs at `system`; the result must evaluate.
pub fn overlay(pins: &str, system: &str) -> String {
    let (np, sys) = (flake(pins), quote(system));
    let pkgs = format!("import np.outPath {{ system = {sys}; overlays = [ o ]; }}");
    format!(
        "o: let np = {np}; pkgs = {pkgs}; in {{ attrs = builtins.length (builtins.attrNames (o pkgs pkgs)); }}"
    )
}

/// A NixOS module evaluated in a minimal NixOS configuration of the pinned nixpkgs, with the
/// module system's option check on: a module that sets an option nobody declares fails here. The
/// configuration is `x86_64-linux` (NixOS is Linux; nothing is built), and its build root is forced.
pub fn nixos_module(pins: &str) -> String {
    let np = flake(pins);
    let base = "{ nixpkgs.hostPlatform = \"x86_64-linux\"; system.stateVersion = \"25.11\"; \
        boot.loader.grub.enable = false; \
        fileSystems.\"/\" = { device = \"none\"; fsType = \"tmpfs\"; }; }";
    let eval = format!("np.lib.nixosSystem {{ modules = [ m {base} ]; }}");
    format!(
        "m: let np = {np}; e = {eval}; in {{ drvPath = e.config.system.build.toplevel.drvPath; }}"
    )
}

/// What a module export is, forced to a value; used where the kit's lock has no module system for
/// it (Home Manager and nix-darwin modules).
pub fn module_value() -> String {
    "m: { type = builtins.typeOf m; }".to_string()
}

/// A configuration's build root: the derivation of `leaf` (for example `config.system.build.toplevel`).
pub fn configuration(leaf: &[&str]) -> String {
    let leaf = leaf.join(".");
    format!("c: {{ drvPath = c.{leaf}.drvPath; name = c.{leaf}.name or null; }}")
}

/// A template: the path it is copied from and its description.
pub fn template() -> String {
    "t: { path = \"${t.path}\"; description = t.description or null; }".to_string()
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
    fn an_attribute_path_quotes_every_name_and_the_root_is_a_dot() {
        assert_eq!(attr_path(&[]), ".");
        assert_eq!(
            attr_path(&["packages", "x86_64-linux"]),
            "\"packages\".\"x86_64-linux\""
        );
        assert_eq!(
            attr_path(&["a.b", "c\"d", "e\\f"]),
            "\"a.b\".\"c\\\"d\".\"e\\\\f\""
        );
    }

    #[test]
    fn an_installable_is_the_reference_a_hash_and_the_path() {
        assert_eq!(
            installable("git+file:///r?dir=x", &["apps", "s"]),
            "git+file:///r?dir=x#\"apps\".\"s\""
        );
        assert_eq!(installable("u", &[]), "u#.");
    }

    #[test]
    fn each_builder_names_the_thing_it_forces() {
        assert_eq!(names(), "builtins.attrNames");
        assert!(derivation().starts_with("d: ") && derivation().contains("d.drvPath"));
        assert!(derivation().contains("intersectAttrs"));
        assert!(app().contains("a.program") && app().contains("a.type"));
        let o = overlay("np:ref", "x86_64-linux");
        assert!(
            o.starts_with("o: ")
                && o.contains("import np.outPath")
                && o.contains("\"x86_64-linux\"")
                && o.contains("np:ref")
        );
        let m = nixos_module("np:ref");
        assert!(m.starts_with("m: ") && m.contains("np.lib.nixosSystem") && m.contains("np:ref"));
        assert!(m.contains("hostPlatform = \"x86_64-linux\""));
        assert!(!m.contains("_module.check = false"));
        assert!(m.contains("config.system.build.toplevel.drvPath"));
        assert_eq!(module_value(), "m: { type = builtins.typeOf m; }");
        let c = configuration(&["config", "system", "build", "toplevel"]);
        assert!(c.contains("c.config.system.build.toplevel.drvPath"));
        assert_eq!(config_warnings(), "c: c.config.warnings");
        // The path is interpolated, which puts the template's source in the store (a fresh source
        // is not there yet, and `toString` would name a path that does not exist).
        assert!(template().contains("\"${t.path}\""));
        assert!(!template().contains("toString"));
    }

    #[test]
    fn a_hostile_pin_stays_inside_its_literal() {
        let o = overlay("x\" ; builtins.abort \"y", "s");
        assert!(o.contains("x\\\" ; builtins.abort \\\"y"));
    }
}
