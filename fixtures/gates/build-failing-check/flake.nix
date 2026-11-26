{
  description = "gate 4 fixture: a failing check refuses build-test";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/04906337b6d53e8bb0eaf0470ae9f070a4d4eb55";
  outputs =
    { nixpkgs, ... }:
    let
      mk =
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          pkg = pkgs.runCommand "build-ok-pkg" { } "mkdir -p $out";
          chk = pkgs.runCommand "build-failing-check" { } "exit 1";
        };
    in
    {
      packages = {
        x86_64-linux.default = (mk "x86_64-linux").pkg;
        aarch64-linux.default = (mk "aarch64-linux").pkg;
        aarch64-darwin.default = (mk "aarch64-darwin").pkg;
      };
      checks = {
        x86_64-linux.fail = (mk "x86_64-linux").chk;
        aarch64-linux.fail = (mk "aarch64-linux").chk;
        aarch64-darwin.fail = (mk "aarch64-darwin").chk;
      };
    };
}
