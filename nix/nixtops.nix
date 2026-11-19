# The nixtops runner, built from Cargo.lock with no network. The tool table and the judge path
# are passed to build.rs; nothing about a version is typed in the Rust source.
{
  lib,
  pkgs,
  rustPlatform,
  nixpkgsInput,
}:
let
  kit = import ./tools.nix { inherit pkgs; };
  table = { nix = kit.judge; } // kit.tools;
  toolTable = lib.concatStringsSep ";" (lib.mapAttrsToList (name: pkg: "${name}=${pkg.version}") table);
  root = ../.;
in
rustPlatform.buildRustPackage {
  pname = "nixtops";
  version = "0.0.0";

  # The crate embeds schema/, profiles/ and config/ with include_str! and include_dir! from
  # ../../ paths, so the source must carry them, plus the example the tests read.
  src = lib.fileset.toSource {
    inherit root;
    fileset = lib.fileset.unions [
      (root + "/Cargo.toml")
      (root + "/Cargo.lock")
      (root + "/crates")
      (root + "/schema")
      (root + "/profiles")
      (root + "/config")
      (root + "/laws/diagnostics.toml")
      (root + "/examples/quickstart/nix-tops.toml")
    ];
  };

  cargoLock.lockFile = root + "/Cargo.lock";

  # The gate-one tests run real git repositories in a temporary directory.
  nativeCheckInputs = [ pkgs.git ];

  NIXTOPS_TOOLS = toolTable;
  NIXTOPS_JUDGE = lib.getExe kit.judge;
  NIXTOPS_EVAL_JOBS = lib.getExe kit.tools.nix-eval-jobs;
  # The kit's own lock, as a locked reference: overlays and modules are checked against it.
  NIXTOPS_NIXPKGS = "github:NixOS/nixpkgs/${nixpkgsInput.rev}?narHash=${nixpkgsInput.narHash}";

  meta = {
    description = "The Nix-TOPS runner";
    license = with lib.licenses; [
      mit
      asl20
    ];
    mainProgram = "nixtops";
  };
}
