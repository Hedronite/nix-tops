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

      devShells = forAllSystems (pkgs: {
        default = pkgs.callPackage ./nix/devshell.nix { };
      });

      formatter = forAllSystems (pkgs: pkgs.nixfmt-tree);
    };
}
