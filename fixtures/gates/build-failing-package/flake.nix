{
  description = "gate 4 fixture: a failing package refuses build-test";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/04906337b6d53e8bb0eaf0470ae9f070a4d4eb55";
  outputs =
    { nixpkgs, ... }:
    let
      mk =
        system:
        nixpkgs.legacyPackages.${system}.runCommand "build-failing-package" { } "exit 1";
    in
    {
      packages = {
        x86_64-linux.default = mk "x86_64-linux";
        aarch64-linux.default = mk "aarch64-linux";
        aarch64-darwin.default = mk "aarch64-darwin";
      };
    };
}
