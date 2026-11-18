//! `nixtops init`: writes `nix-tops.toml`, the CI workflow and the `AGENTS.md` section, prints the
//! `flake.nix` lines, and never edits `flake.nix`. Refuses to overwrite without `--force`.
//! Until the output inventory is built the facets come from `--facet` only.

use crate::{
    config::render,
    exit::{Exit, Output},
    profiles,
};
use clap::Args as ClapArgs;
use std::{fs, path::Path};

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

/// Write into `dir`. Pure of the process's working directory so tests can use a temp dir.
pub fn init_in(dir: &Path, args: &Args) -> Output {
    let bad = |m: String| Output::fail(Exit::ConfigInvalid, format!("nixtops init: {m}\n"));
    if args.facets.is_empty() {
        return bad(
            "--facet is required until the output inventory is built (nixtops outputs)".into(),
        );
    }
    let mut facets: Vec<String> = Vec::new();
    for f in &args.facets {
        if !KNOWN.contains(&f.as_str()) {
            return bad(format!("--facet {f}: not a facet (section 5.1)"));
        }
        if profiles::text(f).is_none() {
            return bad(format!("--facet {f}: no profile for this facet yet"));
        }
        if !facets.contains(f) {
            facets.push(f.clone());
        }
    }
    let systems: Vec<String> = render::SYSTEMS.iter().map(|s| s.to_string()).collect();
    let toml = dir.join("nix-tops.toml");
    let workflow = dir.join(".github/workflows/nix-tops.yml");
    if !args.force {
        let exists: Vec<String> = [&toml, &workflow]
            .iter()
            .filter(|p| p.exists())
            .map(|p| p.strip_prefix(dir).unwrap_or(p).display().to_string())
            .collect();
        if !exists.is_empty() {
            return Output::fail(
                Exit::Refused,
                format!(
                    "nixtops init: {} exists; pass --force to overwrite\n",
                    exists.join(", ")
                ),
            );
        }
    }
    let agents = dir.join("AGENTS.md");
    let merged = render::merge_agents(
        fs::read_to_string(&agents).ok().as_deref(),
        &render::agents_section(&facets, &systems, "ratchet"),
    );
    let writes = [
        (toml, render::config_toml(&facets, &systems)),
        (workflow, render::WORKFLOW_TEMPLATE.to_string()),
        (agents, merged),
    ];
    for (path, text) in &writes {
        if let Some(parent) = path.parent()
            && fs::create_dir_all(parent).is_err()
        {
            return Output::fail(
                Exit::Environment,
                format!("nixtops init: cannot create {}\n", parent.display()),
            );
        }
        if fs::write(path, text).is_err() {
            return Output::fail(
                Exit::Environment,
                format!("nixtops init: cannot write {}\n", path.display()),
            );
        }
    }
    Output::ok(format!(
        "wrote nix-tops.toml, .github/workflows/nix-tops.yml and the AGENTS.md section\nsystems are the kit's three; edit [toolchain].systems to the systems your flake exports\n{FLAKE_LINES}"
    ))
}

pub fn run(args: &Args) -> Output {
    init_in(Path::new("."), args)
}
