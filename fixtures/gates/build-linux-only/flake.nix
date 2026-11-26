{
  description = "gate 4 fixture: Linux-only package deferred on darwin";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/04906337b6d53e8bb0eaf0470ae9f070a4d4eb55";
  outputs =
    { nixpkgs, ... }:
    {
      packages.x86_64-linux.default =
        nixpkgs.legacyPackages.x86_64-linux.runCommand "linux-only-pkg" { } "mkdir -p $out";
    };
}
