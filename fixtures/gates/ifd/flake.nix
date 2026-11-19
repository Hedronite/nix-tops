{
  description = "gate 3 fixture: an output needs import from derivation";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/04906337b6d53e8bb0eaf0470ae9f070a4d4eb55";
  outputs = { nixpkgs, ... }: let
    pkgs = nixpkgs.legacyPackages.x86_64-linux;
    value = import (pkgs.runCommand "value" { } "echo 1 > $out");
  in {
    packages.x86_64-linux.default = pkgs.runCommand "ifd-${toString value}" { } "touch $out";
  };
}
