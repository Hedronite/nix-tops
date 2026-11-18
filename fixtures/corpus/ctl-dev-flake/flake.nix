{
  description = "Corpus control: a dev-flake that passes";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/04906337b6d53e8bb0eaf0470ae9f070a4d4eb55";
  outputs = { self, nixpkgs }:
    let
      systems = [ "aarch64-darwin" "x86_64-linux" "aarch64-linux" ];
      forAll = f: nixpkgs.lib.genAttrs systems (s: f nixpkgs.legacyPackages.${s});
    in
    {
      devShells = forAll (pkgs: { default = pkgs.mkShell { packages = [ pkgs.hello ]; }; });
      checks = forAll (pkgs: { shell = self.devShells.${pkgs.stdenv.hostPlatform.system}.default; });
    };
}
