{
  description = "Nix-TOPS kit: the nixtops runner, gated by its own protocol";

  # One input, locked. The kit's tools and its judge (the CppNix of this pin) come from it.
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "aarch64-darwin"
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      lib = import ./nix/lib { inherit nixpkgs; };

      packages = forAllSystems (pkgs: rec {
        nixtops = pkgs.callPackage ./nix/nixtops.nix { nixpkgsInput = nixpkgs; };
        default = nixtops;
      });

      apps = forAllSystems (pkgs: {
        nixtops = {
          type = "app";
          program = nixpkgs.lib.getExe self.packages.${pkgs.stdenv.hostPlatform.system}.nixtops;
          meta.description = "Gate a Nix flake against the Nix-TOPS protocol";
        };
      });

      # Frozen evidence, the verbatim quickstart example, and deliberate lint fixtures stay untouched.
      # The lint checks live on one system; the runner is a check on every system, so gate 4 has
      # something to build wherever it runs.
      checks =
        nixpkgs.lib.recursiveUpdate
          (self.lib.mkChecks {
            inherit self systems;
            exclude = [
              "docs/evidence"
              "examples/quickstart"
              "fixtures"
            ];
          })
          (
            nixpkgs.lib.genAttrs systems (system: {
              inherit (self.packages.${system}) nixtops;
            })
          );

      devShells = forAllSystems (pkgs: {
        default = pkgs.callPackage ./nix/devshell.nix { };
      });

      formatter = forAllSystems (pkgs: pkgs.nixfmt-tree);
    };
}
