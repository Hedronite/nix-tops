//! `nixtops outputs`: every output of a flake at every system, with its derivation, meta and the
//! error evaluating it gave. The walk does not stop at the first error; errors are data and the
//! command exits 0 when the inventory is complete.

use crate::{
    build_info,
    exit::{Exit, Output},
    inventory::{self, Inventory, Record, Session, Tools},
};
use clap::Args as ClapArgs;
use serde::Serialize;

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// The flake to inventory
    #[arg(default_value = ".")]
    pub flake: String,
    /// Print JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Serialize)]
struct Summary {
    outputs: usize,
    errors: usize,
}

#[derive(Serialize)]
struct Report<'a> {
    flake: &'a str,
    facets: Vec<&'static str>,
    systems: Vec<String>,
    summary: Summary,
    outputs: &'a [Record],
}

fn report(inventory: &Inventory) -> Report<'_> {
    let facets = inventory::derived_facets(&inventory.structure)
        .into_iter()
        .map(|f| f.as_str())
        .collect();
    let errors = inventory.records.iter().filter(|r| r.is_error()).count();
    Report {
        flake: &inventory.flake,
        facets,
        systems: inventory::exported_systems(&inventory.structure)
            .into_iter()
            .collect(),
        summary: Summary {
            outputs: inventory.records.len(),
            errors,
        },
        outputs: &inventory.records,
    }
}

fn first_line(error: &str) -> &str {
    error
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim()
}

fn row(record: &Record) -> String {
    let detail = record.error.as_deref().map(first_line).unwrap_or("");
    format!(
        "{:<13} {:<15} {} {}",
        record.level.name(),
        record.kind.name(),
        record.attr,
        detail
    )
    .trim_end()
    .to_string()
}

/// The text form: a header, one row per output, a count.
pub fn render_text(inventory: &Inventory) -> String {
    let r = report(inventory);
    let mut text = format!(
        "flake {}\nfacets {}\nsystems {}\n",
        r.flake,
        r.facets.join(", "),
        r.systems.join(", ")
    );
    for record in r.outputs {
        text.push_str(&row(record));
        text.push('\n');
    }
    text.push_str(&format!(
        "{} outputs, {} errors\n",
        r.summary.outputs, r.summary.errors
    ));
    text
}

pub fn render_json(inventory: &Inventory) -> String {
    let mut text = serde_json::to_string_pretty(&report(inventory)).unwrap_or_default();
    text.push('\n');
    text
}

/// Runs the inventory with `tools`; the tool choice is the caller's so tests can pass stand-ins.
pub fn run_with(tools: Tools, args: &Args) -> Output {
    let session = match Session::new(tools) {
        Ok(session) => session,
        Err(err) => {
            return Output::fail(
                Exit::Environment,
                format!("nixtops outputs: cannot create a scratch directory: {err}\n"),
            );
        }
    };
    match inventory::collect(&session, &args.flake) {
        Ok(found) if args.json => Output::ok(render_json(&found)),
        Ok(found) => Output::ok(render_text(&found)),
        Err(error) => Output::fail(
            Exit::Environment,
            format!("nixtops outputs: cannot read the flake: {error}\n"),
        ),
    }
}

pub fn run(args: &Args) -> Output {
    match Tools::from_parts(
        build_info::JUDGE_PATH,
        build_info::EVAL_JOBS_PATH,
        build_info::NIXPKGS_REF,
    ) {
        Some(tools) => run_with(tools, args),
        None => Output::fail(
            Exit::Environment,
            "nixtops outputs: this build has no pinned judge (built outside the kit's flake)\n"
                .to_string(),
        ),
    }
}
