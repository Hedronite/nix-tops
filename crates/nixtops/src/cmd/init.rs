//! `nixtops init`: writes `nix-tops.toml`, the CI workflow and the `AGENTS.md` section, prints the
//! `flake.nix` lines, and never edits `flake.nix`. Refuses to overwrite without `--force`.
//! The facets and systems come from the flake's outputs (narrowed by `--facet`); a build with no
//! pinned judge falls back to `--facet` and the kit's three systems.

use crate::{
    build_info,
    config::{
        outputs::{self, Read},
        render,
    },
    exit::{Exit, Output},
    inventory::{self, Structure, Tools},
    profiles,
};
use clap::Args as ClapArgs;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Facet to write (repeatable); narrows the facets the outputs show
    #[arg(long = "facet")]
    pub facets: Vec<String>,
    /// Overwrite existing files
    #[arg(long)]
    pub force: bool,
}

const FLAKE_LINES: &str = "Add to your flake.nix (init never edits it):\n  inputs.nix-tops.url = \"github:Hedronite/nix-tops\";   # no follows: the kit's tools stay on its pin\n  apps = perSystem (system: { nixtops = nix-tops.apps.${system}.nixtops; });\n  checks = nixpkgs.lib.recursiveUpdate (nix-tops.lib.mkChecks { inherit self systems; }) { /* your checks */ };\n";

const KNOWN: [&str; 9] = [
    "dev-flake",
    "package-set",
    "nixos-module",
    "flake-lib",
    "nixos-host",
    "home-config",
    "darwin-host",
    "fleet-root",
    "image",
];

fn bad(message: String) -> Output {
    Output::fail(Exit::ConfigInvalid, format!("nixtops init: {message}\n"))
}

/// The names given with `--facet`, checked against the vocabulary before anything is read.
fn check_names(args: &Args) -> Option<Output> {
    args.facets.iter().find_map(|f| {
        if outputs::NOT_DERIVED.contains(&f.as_str()) {
            return Some(bad(format!(
                "--facet {f}: M1 does not derive this facet from the outputs"
            )));
        }
        (!KNOWN.contains(&f.as_str()))
            .then(|| bad(format!("--facet {f}: not a facet (section 5.1)")))
    })
}

/// The facets and systems to write when the build has no pinned judge: `--facet` only, and the
/// kit's three systems.
fn without_inventory(args: &Args) -> Result<(Vec<String>, Vec<String>), Output> {
    if args.facets.is_empty() {
        return Err(bad(
            "--facet is required when the outputs cannot be read (no pinned judge in this build)"
                .into(),
        ));
    }
    let mut facets: Vec<String> = Vec::new();
    for f in &args.facets {
        if profiles::text(f).is_none() {
            return Err(bad(format!("--facet {f}: no profile for this facet yet")));
        }
        if !facets.contains(f) {
            facets.push(f.clone());
        }
    }
    let systems = render::SYSTEMS.iter().map(|s| s.to_string()).collect();
    Ok((facets, systems))
}

fn unsupported(systems: &[String]) -> Option<Output> {
    let bad_one = systems
        .iter()
        .find(|s| !render::SYSTEMS.contains(&s.as_str()))?;
    Some(bad(format!(
        "the outputs export {bad_one}, which the kit does not support (section 2)"
    )))
}

/// The facets the outputs show, narrowed by `--facet` (which may only name facets they show), and
/// the systems they export.
fn from_inventory(
    args: &Args,
    structure: &Structure,
) -> Result<(Vec<String>, Vec<String>), Output> {
    if let Some((name, error)) = structure.errors.iter().next() {
        return Err(Output::fail(
            Exit::Environment,
            format!("nixtops init: cannot list the members of {name}: {error}\n"),
        ));
    }
    let derived: Vec<String> = inventory::derived_facets(structure)
        .iter()
        .map(|f| f.as_str().to_string())
        .collect();
    if let Some(missing) = args.facets.iter().find(|f| !derived.contains(f)) {
        return Err(bad(format!(
            "--facet {missing}: the outputs do not show it (they show: {})",
            derived.join(", ")
        )));
    }
    let facets = if args.facets.is_empty() {
        derived
    } else {
        args.facets.clone()
    };
    let systems: Vec<String> = inventory::exported_systems(structure).into_iter().collect();
    if facets.is_empty() || systems.is_empty() {
        return Err(bad(
            "the outputs show no facet or no system; there is nothing to write".into(),
        ));
    }
    match unsupported(&systems) {
        Some(refusal) => Err(refusal),
        None => Ok((dedup(facets), systems)),
    }
}

fn dedup(items: Vec<String>) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for item in items {
        if !seen.contains(&item) {
            seen.push(item);
        }
    }
    seen
}

/// The refusal when a file `init` owns exists and `--force` is absent.
fn refuse_overwrite(dir: &Path, paths: &[&PathBuf]) -> Option<Output> {
    let exists: Vec<String> = paths
        .iter()
        .filter(|p| p.exists())
        .map(|p| p.strip_prefix(dir).unwrap_or(p).display().to_string())
        .collect();
    (!exists.is_empty()).then(|| {
        Output::fail(
            Exit::Refused,
            format!(
                "nixtops init: {} exists; pass --force to overwrite\n",
                exists.join(", ")
            ),
        )
    })
}

fn write_all(writes: &[(PathBuf, String)]) -> Option<Output> {
    let env = |what: &str, p: &Path| {
        Some(Output::fail(
            Exit::Environment,
            format!("nixtops init: cannot {what} {}\n", p.display()),
        ))
    };
    for (path, text) in writes {
        if let Some(parent) = path.parent()
            && fs::create_dir_all(parent).is_err()
        {
            return env("create", parent);
        }
        if fs::write(path, text).is_err() {
            return env("write", path);
        }
    }
    None
}

/// The facets and systems to write, from whatever reading the outputs gave.
fn choose(args: &Args, read: Read) -> Result<(Vec<String>, Vec<String>), Output> {
    match read {
        Read::Found(structure) => from_inventory(args, &structure),
        Read::NoJudge => without_inventory(args),
        Read::Failed(error) => Err(Output::fail(
            Exit::Environment,
            format!("nixtops init: cannot read the flake's outputs: {error}\n"),
        )),
    }
}

/// Write into `dir`. `inventory` reads the flake's outputs; it is called only after the cheap
/// checks pass. Pure of the process's working directory so tests can use a temp dir.
pub fn init_with(dir: &Path, args: &Args, inventory: &dyn Fn() -> Read) -> Output {
    if let Some(refusal) = check_names(args) {
        return refusal;
    }
    let toml = dir.join("nix-tops.toml");
    let workflow = dir.join(".github/workflows/nix-tops.yml");
    let agents = dir.join("AGENTS.md");
    if !args.force
        && let Some(refusal) = refuse_overwrite(dir, &[&toml, &workflow])
    {
        return refusal;
    }
    let (facets, systems) = match choose(args, inventory()) {
        Ok(chosen) => chosen,
        Err(output) => return output,
    };
    let merged = render::merge_agents(
        fs::read_to_string(&agents).ok().as_deref(),
        &render::agents_section(&facets, &systems, "ratchet"),
    );
    let writes = [
        (toml, render::config_toml(&facets, &systems)),
        (workflow, render::WORKFLOW_TEMPLATE.to_string()),
        (agents, merged),
    ];
    if let Some(failure) = write_all(&writes) {
        return failure;
    }
    Output::ok(format!(
        "wrote nix-tops.toml, .github/workflows/nix-tops.yml and the AGENTS.md section\nfacets {}; systems {}\n{FLAKE_LINES}",
        facets.join(", "),
        systems.join(", ")
    ))
}

/// Without reading the outputs (the build has no pinned judge): `--facet` and the kit's systems.
pub fn init_in(dir: &Path, args: &Args) -> Output {
    init_with(dir, args, &|| Read::NoJudge)
}

/// `nixtops init` in `dir` with the tools chosen by the caller.
pub fn run_in(dir: &Path, args: &Args, tools: Option<Tools>) -> Output {
    init_with(dir, args, &|| outputs::read_with(tools.clone(), dir))
}

pub fn run(args: &Args) -> Output {
    let tools = Tools::from_parts(
        build_info::JUDGE_PATH,
        build_info::EVAL_JOBS_PATH,
        build_info::NIXPKGS_REF,
    );
    run_in(Path::new("."), args, tools)
}
