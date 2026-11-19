//! Walks a flake's outputs with the pinned tools and returns one record per output and system.
//! An output that fails to evaluate is a record with its error; the walk goes on.

use super::{
    jobs::{self, Target, clean},
    nix_expr as expr,
    record::{Kind, Level, Record, prune_meta},
    structure::{Shape, Structure, shape_of},
    tools::{Backend, Session},
};
use serde_json::Value;
use std::{fs, io, path::Path};

/// The inventory of one flake.
#[derive(Debug)]
pub struct Inventory {
    /// The locked reference the walk used.
    pub flake: String,
    pub structure: Structure,
    pub records: Vec<Record>,
}

/// `nix eval --json <url>#<path> --apply <apply>` with the judge. The flake is read through its
/// own reference, not `builtins.getFlake`, so a dirty git tree evaluates the way `nix flake
/// check` reads it: tracked files only (an untracked file is invisible, as N-04 says).
pub fn eval_json(
    session: &Session,
    url: &str,
    path: &[&str],
    apply: &str,
) -> Result<Value, String> {
    let target = expr::installable(url, path);
    let run = session.judge(
        &["eval", "--json", &target, "--apply", apply],
        &["--no-update-lock-file"],
    );
    if !run.ok {
        return Err(clean(&run.stderr));
    }
    serde_json::from_str(&run.stdout).map_err(|err| format!("not JSON: {err}"))
}

fn names(value: Value) -> Vec<String> {
    let items = value.as_array().into_iter().flatten();
    items
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

fn text(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

/// The locked reference of `flake_ref`: what `nix flake metadata` says its `url` is.
pub fn locked_url(session: &Session, flake_ref: &str) -> Result<String, String> {
    let run = session.judge(
        &[
            "flake",
            "metadata",
            "--json",
            "--no-update-lock-file",
            flake_ref,
        ],
        &[],
    );
    if !run.ok {
        return Err(clean(&run.stderr));
    }
    let json: Value =
        serde_json::from_str(&run.stdout).map_err(|err| format!("not JSON: {err}"))?;
    text(&json, "url").ok_or_else(|| "nix flake metadata gave no url".to_string())
}

/// An output name with its members, or the error listing them gave.
pub type Listed = Vec<(String, Result<Vec<String>, String>)>;

/// The top-level output names and, for each, its members.
pub fn read_structure(session: &Session, url: &str) -> Result<Listed, String> {
    let top = eval_json(session, url, &[], &expr::names())?;
    let listed = names(top).into_iter().map(|name| {
        let members = member_names(session, url, &name);
        (name, members)
    });
    Ok(listed.collect())
}

fn member_names(session: &Session, url: &str, name: &str) -> Result<Vec<String>, String> {
    if name == "lib" || shape_of(name) == Shape::Unknown {
        return Ok(Vec::new());
    }
    let listed = eval_json(session, url, &[name], &expr::names()).map(names)?;
    match shape_of(name) {
        Shape::PerSystem(kind) if lists_members(kind) => Ok(listed
            .into_iter()
            .filter(|system| has_members(session, url, name, system))
            .collect()),
        _ => Ok(listed),
    }
}

/// A formatter is a derivation per system and `legacyPackages` is the whole of nixpkgs; neither
/// is listed to see whether a system's set is empty.
fn lists_members(kind: Kind) -> bool {
    !matches!(kind, Kind::Formatter | Kind::LegacyPackages)
}

/// Whether the set of `name` at `system` has anything in it. `devShells.aarch64-linux = { }`
/// exports nothing: the system is not exported, and an output with no non-empty set derives no
/// facet. A set that cannot be listed counts as non-empty (its error shows when it is walked).
fn has_members(session: &Session, url: &str, name: &str, system: &str) -> bool {
    let listed = eval_json(session, url, &[name, system], &expr::names());
    listed.map_or(true, |value| !names(value).is_empty())
}

fn drv_record(attr: &str, kind: Kind, system: Option<&str>, value: &Value) -> Record {
    Record::new(attr, kind, system, Level::Evaluated)
        .with_drv(text(value, "drvPath"), text(value, "name"))
        .with_meta(value.get("meta").and_then(prune_meta))
}

fn single_drv(
    session: &Session,
    url: &str,
    path: &[&str],
    kind: Kind,
    system: Option<&str>,
) -> Record {
    let attr = path.join(".");
    match eval_json(session, url, path, &expr::derivation()) {
        Ok(value) => drv_record(&attr, kind, system, &value),
        Err(error) => Record::failed(&attr, kind, system, error),
    }
}

fn derivation_set(
    session: &Session,
    url: &str,
    name: &str,
    system: &str,
    kind: Kind,
) -> Vec<Record> {
    let base = format!("{name}.{system}");
    if session.backend() == Backend::PerAttribute {
        return per_attribute(session, url, &[name, system], kind, system);
    }
    let run = session.eval_jobs(&format!("{url}#{base}"));
    let mut found = jobs::records(
        &Target {
            base: &base,
            kind,
            system,
        },
        run.ok,
        &run.stdout,
        &run.stderr,
    );
    found.extend(missing_members(session, url, &[name, system], kind, &found));
    found
}

/// A member of a per-system output that `nix-eval-jobs` says nothing about is not a derivation (it
/// lists derivations and the sets it descends into, and passes over the rest): `packages.<s>.bad
/// = 42`. That is an error record, never a silent gap. When the whole set failed there is nothing
/// to add: its own record says why.
fn missing_members(
    session: &Session,
    url: &str,
    base: &[&str],
    kind: Kind,
    found: &[Record],
) -> Vec<Record> {
    let (base_attr, system) = (base.join("."), base[1]);
    if found.iter().any(|r| r.attr == base_attr) {
        return Vec::new();
    }
    let Ok(listed) = eval_json(session, url, base, &expr::names()).map(names) else {
        return Vec::new();
    };
    let said = |member: &str| {
        let full = format!("{base_attr}.{member}");
        let below = format!("{full}.");
        found
            .iter()
            .any(|r| r.attr == full || r.attr.starts_with(&below))
    };
    let silent = listed.into_iter().filter(|member| !said(member));
    silent
        .map(|member| {
            let full = format!("{base_attr}.{member}");
            let error = format!("{full} is not a derivation");
            Record::failed(&full, kind, Some(system), error)
        })
        .collect()
}

/// The derivations of one attribute set, one `nix eval` each: a failing attribute is an error
/// record and the others still evaluate.
fn per_attribute(
    session: &Session,
    url: &str,
    base: &[&str],
    kind: Kind,
    system: &str,
) -> Vec<Record> {
    match eval_json(session, url, base, &expr::names()).map(names) {
        Err(error) => vec![Record::failed(&base.join("."), kind, Some(system), error)],
        Ok(list) => list
            .iter()
            .map(|n| {
                let path: Vec<&str> = base.iter().copied().chain([n.as_str()]).collect();
                single_drv(session, url, &path, kind, Some(system))
            })
            .collect(),
    }
}

fn app(session: &Session, url: &str, path: &[&str], system: &str) -> Record {
    let attr = path.join(".");
    let checked = eval_json(session, url, path, &expr::app()).and_then(|value| {
        match text(&value, "type").as_deref() {
            Some("app") if text(&value, "program").is_some() => Ok(()),
            other => Err(format!(
                "not an app: type is {other:?} and program is {}",
                text(&value, "program").is_some()
            )),
        }
    });
    match checked {
        // Instantiated would mean `program` resolves to a file in the built output, which needs a
        // build; in this version an app that checks out is evaluated.
        Ok(()) => Record::new(&attr, Kind::App, Some(system), Level::Evaluated),
        Err(error) => Record::failed(&attr, Kind::App, Some(system), error),
    }
}

fn apps(session: &Session, url: &str, system: &str) -> Vec<Record> {
    let base = format!("apps.{system}");
    let listed = eval_json(session, url, &["apps", system], &expr::names()).map(names);
    match listed {
        Err(error) => vec![Record::failed(&base, Kind::App, Some(system), error)],
        Ok(list) => list
            .iter()
            .map(|n| app(session, url, &["apps", system, n], system))
            .collect(),
    }
}

fn per_system_leaf(
    session: &Session,
    url: &str,
    name: &str,
    kind: Kind,
    system: &str,
) -> Vec<Record> {
    let attr = format!("{name}.{system}");
    match kind {
        Kind::App => apps(session, url, system),
        Kind::Formatter => vec![single_drv(
            session,
            url,
            &[name, system],
            kind,
            Some(system),
        )],
        Kind::LegacyPackages => vec![Record::new(&attr, kind, Some(system), Level::Listed)],
        _ => derivation_set(session, url, name, system, kind),
    }
}

fn overlay(session: &Session, url: &str, name: &str, system: &str) -> Record {
    let path = ["overlays", name];
    let attr = path.join(".");
    match eval_json(
        session,
        url,
        &path,
        &expr::overlay(session.nixpkgs(), system),
    ) {
        Ok(_) => Record::new(&attr, Kind::Overlay, None, Level::Instantiated),
        Err(error) => Record::failed(&attr, Kind::Overlay, None, error),
    }
}

/// Why a Home Manager or nix-darwin module stays at `evaluated`: the kit's lock has neither.
const NO_MODULE_SYSTEM: &str =
    "the kit's lock has no Home Manager or nix-darwin; the module is evaluated, not instantiated";

/// A NixOS module is instantiated in a minimal NixOS configuration with the option check on.
/// Other module kinds are evaluated as values only.
fn module(session: &Session, url: &str, top: &str, name: &str) -> Record {
    let path = [top, name];
    let attr = path.join(".");
    if top != "nixosModules" {
        return match eval_json(session, url, &path, &expr::module_value()) {
            Ok(_) => {
                Record::new(&attr, Kind::Module, None, Level::Evaluated).with_note(NO_MODULE_SYSTEM)
            }
            Err(error) => Record::failed(&attr, Kind::Module, None, error),
        };
    }
    match eval_json(session, url, &path, &expr::nixos_module(session.nixpkgs())) {
        Ok(_) => Record::new(&attr, Kind::Module, None, Level::Instantiated),
        Err(error) => Record::failed(&attr, Kind::Module, None, error),
    }
}

fn copy_dir(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// A template is copied to a scratch directory and its flake must evaluate there.
fn template(session: &Session, url: &str, name: &str) -> Record {
    let path = ["templates", name];
    let attr = path.join(".");
    let fail = |error: String| Record::failed(&attr, Kind::Template, None, error);
    let value = match eval_json(session, url, &path, &expr::template()) {
        Ok(value) => value,
        Err(error) => return fail(error),
    };
    let Some(source) = text(&value, "path") else {
        return fail("the template has no path".to_string());
    };
    let scratch = session.scratch(&format!("template-{name}"));
    if let Err(err) = copy_dir(Path::new(&source), &scratch) {
        return fail(format!("cannot copy the template: {err}"));
    }
    let target = format!("path:{}", scratch.display());
    let run = session.judge(
        &[
            "flake",
            "metadata",
            "--json",
            "--no-write-lock-file",
            &target,
        ],
        &[],
    );
    if run.ok {
        Record::new(&attr, Kind::Template, None, Level::Instantiated)
    } else {
        fail(clean(&run.stderr))
    }
}

fn configuration(session: &Session, url: &str, top: &str, name: &str) -> Record {
    let leaf: &[&str] = if top == "homeConfigurations" {
        &["activationPackage"]
    } else {
        &["config", "system", "build", "toplevel"]
    };
    let path = [top, name];
    let attr = path.join(".");
    match eval_json(session, url, &path, &expr::configuration(leaf)) {
        Ok(value) => drv_record(&attr, Kind::Configuration, None, &value),
        Err(error) => Record::failed(&attr, Kind::Configuration, None, error),
    }
}

fn flat_member(
    session: &Session,
    url: &str,
    top: &str,
    kind: Kind,
    name: &str,
    system: &str,
) -> Record {
    match kind {
        Kind::Overlay => overlay(session, url, name, system),
        Kind::Module => module(session, url, top, name),
        Kind::Template => template(session, url, name),
        _ => configuration(session, url, top, name),
    }
}

fn flat(
    session: &Session,
    url: &str,
    top: &str,
    kind: Kind,
    members: &[String],
    system: &str,
) -> Vec<Record> {
    if kind == Kind::Lib {
        return vec![Record::new(top, kind, None, Level::Listed)];
    }
    members
        .iter()
        .map(|name| flat_member(session, url, top, kind, name, system))
        .collect()
}

/// The system overlays and modules are checked against: the first the flake exports.
fn check_system(structure: &Structure) -> String {
    structure
        .systems()
        .into_iter()
        .next()
        .unwrap_or_else(|| "x86_64-linux".to_string())
}

fn walk_per_system(
    session: &Session,
    url: &str,
    name: &str,
    kind: Kind,
    systems: &[String],
) -> Vec<Record> {
    systems
        .iter()
        .flat_map(|system| per_system_leaf(session, url, name, kind, system))
        .collect()
}

fn walk_one(
    session: &Session,
    url: &str,
    name: &str,
    members: &[String],
    system: &str,
) -> Vec<Record> {
    match shape_of(name) {
        Shape::PerSystem(kind) => walk_per_system(session, url, name, kind, members),
        Shape::Flat(kind) => flat(session, url, name, kind, members, system),
        Shape::Unknown => vec![Record::new(name, Kind::Unknown, None, Level::Listed)],
    }
}

fn container_error(name: &str, error: String) -> Record {
    let kind = match shape_of(name) {
        Shape::PerSystem(kind) | Shape::Flat(kind) => kind,
        Shape::Unknown => Kind::Unknown,
    };
    Record::failed(name, kind, None, error)
}

/// Walks every output of `flake_ref`.
pub fn collect(session: &Session, flake_ref: &str) -> Result<Inventory, String> {
    let url = locked_url(session, flake_ref)?;
    let listed = read_structure(session, &url)?;
    let mut structure = Structure::default();
    let mut records = Vec::new();
    for (name, members) in &listed {
        match members {
            Ok(found) => structure.insert(name, found.clone()),
            Err(error) => {
                structure.insert_error(name, error.clone());
                records.push(container_error(name, error.clone()));
            }
        }
    }
    let system = check_system(&structure);
    for (name, found) in structure.members.clone() {
        let broken = records.iter().any(|record| record.attr == name);
        if !broken {
            records.extend(walk_one(session, &url, &name, &found, &system));
        }
    }
    records.sort_by(|a, b| a.attr.cmp(&b.attr));
    Ok(Inventory {
        flake: url,
        structure,
        records,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_reads_strings_only() {
        assert_eq!(names(serde_json::json!(["a", 1, "b"])), ["a", "b"]);
        assert!(names(serde_json::json!({"a": 1})).is_empty());
    }

    #[test]
    fn the_check_system_is_the_first_exported_or_a_default() {
        let mut s = Structure::default();
        assert_eq!(check_system(&s), "x86_64-linux");
        s.insert(
            "packages",
            vec!["x86_64-linux".into(), "aarch64-darwin".into()],
        );
        assert_eq!(check_system(&s), "aarch64-darwin");
    }

    #[test]
    fn a_container_error_takes_the_kind_of_its_name() {
        assert_eq!(container_error("packages", "e".into()).kind, Kind::Package);
        assert_eq!(container_error("overlays", "e".into()).kind, Kind::Overlay);
        assert_eq!(container_error("fooBar", "e".into()).kind, Kind::Unknown);
        assert!(container_error("packages", "e".into()).is_error());
    }

    #[test]
    fn copy_dir_copies_nested_files() {
        let from = tempfile::tempdir().unwrap();
        fs::create_dir(from.path().join("sub")).unwrap();
        fs::write(from.path().join("a.nix"), "1").unwrap();
        fs::write(from.path().join("sub/b.nix"), "2").unwrap();
        let to = tempfile::tempdir().unwrap();
        copy_dir(from.path(), &to.path().join("out")).unwrap();
        assert_eq!(
            fs::read_to_string(to.path().join("out/a.nix")).unwrap(),
            "1"
        );
        assert_eq!(
            fs::read_to_string(to.path().join("out/sub/b.nix")).unwrap(),
            "2"
        );
        assert!(copy_dir(Path::new("/nonexistent/x"), &to.path().join("o2")).is_err());
    }
}
