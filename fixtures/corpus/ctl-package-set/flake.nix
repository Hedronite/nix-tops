{
  description = "Corpus control: a package-set that passes";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/04906337b6d53e8bb0eaf0470ae9f070a4d4eb55";
  outputs = { self, nixpkgs }:
    let
      systems = [ "aarch64-darwin" "x86_64-linux" "aarch64-linux" ];
      forAll = f: nixpkgs.lib.genAttrs systems (s: f nixpkgs.legacyPackages.${s});
      meta = {
        description = "A corpus tool";
        license = nixpkgs.lib.licenses.mit;
        mainProgram = "tool";
      };
    in
    {
      packages = forAll (pkgs: rec {
        tool = pkgs.writeShellScriptBin "tool" "echo tool" // { inherit meta; };
        default = tool;
      });
      overlays.default = final: _prev: { corpusTool = self.packages.${final.stdenv.hostPlatform.system}.tool; };
    };
}
