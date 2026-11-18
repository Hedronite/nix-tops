//! Build-time values for `nixtops version` and `doctor`: the protocol id from the schema
//! (one source), and the tool versions and judge path the kit's flake passes in.

use std::{env, fs, path::Path};

fn protocol(schema: &Path) -> String {
    let text = fs::read_to_string(schema).expect("schema/nix-tops.schema.json is readable");
    let json: serde_json::Value = serde_json::from_str(&text).expect("the schema is JSON");
    let id = json
        .pointer("/properties/protocol/const")
        .and_then(|v| v.as_str());
    id.expect("the schema pins the protocol id").to_string()
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let schema = root.join("schema/nix-tops.schema.json");
    println!("cargo:rerun-if-changed={}", schema.display());
    println!("cargo:rustc-env=NIXTOPS_PROTOCOL={}", protocol(&schema));
    for name in [
        "NIXTOPS_TOOLS",
        "NIXTOPS_JUDGE",
        "NIXTOPS_EVAL_JOBS",
        "NIXTOPS_NIXPKGS",
    ] {
        println!("cargo:rerun-if-env-changed={name}");
        println!(
            "cargo:rustc-env={name}={}",
            env::var(name).unwrap_or_default()
        );
    }
}
