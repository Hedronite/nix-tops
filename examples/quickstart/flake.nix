{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    nix-tops.url = "github:Hedronite/nix-tops";
    # Do not set nix-tops.inputs.nixpkgs.follows: the kit's tools stay on the kit's pin (N-43).
  };

  outputs =
    { self, nixpkgs, nix-tops }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "aarch64-darwin" ];
      perSystem = f: nixpkgs.lib.genAttrs systems f;
      forAllSystems = f: perSystem (system: f nixpkgs.legacyPackages.${system});
    in
    {
      packages = forAllSystems (pkgs: { default = pkgs.callPackage ./nix/package.nix { }; });

      # mkChecks returns checks.<system>.nixtops-* from ./nix-tops.toml (no IFD: builtins.fromTOML).
      # Merge it with your own checks per system.
      checks = nixpkgs.lib.recursiveUpdate (nix-tops.lib.mkChecks { inherit self systems; }) (
        perSystem (system: { package = self.packages.${system}.default; })
      );

      formatter = perSystem (system: nix-tops.formatter.${system});

      # Re-export the runner so one locked command runs the gate: nix run .#nixtops -- gate
      apps = perSystem (system: { nixtops = nix-tops.apps.${system}.nixtops; });
    };
}
# planted drift
