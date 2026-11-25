{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    # The kit under test is the tree this fixture sits in, so a pull request measures its own head.
    # An adopter writes github:Hedronite/nix-tops here and locks it.
    nix-tops.url = "path:../..";
    # Do not set nix-tops.inputs.nixpkgs.follows: the kit's tools stay on the kit's pin (N-43).
  };

  outputs =
    {
      self,
      nixpkgs,
      nix-tops,
    }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];
      perSystem = f: nixpkgs.lib.genAttrs systems f;
    in
    {
      formatter = perSystem (system: nix-tops.formatter.${system});

      # Re-export the runner so one locked command runs the gate: nix run .#nixtops -- gate
      apps = perSystem (system: {
        nixtops = nix-tops.apps.${system}.nixtops;
      });
    };
}
