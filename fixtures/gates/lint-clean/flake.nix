{
  description = "gate 2 fixture: a clean flake that passes lint";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/04906337b6d53e8bb0eaf0470ae9f070a4d4eb55";
  outputs =
    _:
    let
      drv =
        system:
        builtins.derivation {
          name = "fixture";
          inherit system;
          builder = "/bin/sh";
        };
    in
    {
      packages = {
        x86_64-linux.default = drv "x86_64-linux";
        aarch64-linux.default = drv "aarch64-linux";
      };
    };
}
