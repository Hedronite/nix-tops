{
  description = "ratchet fixture: one recorded deadnix finding";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/04906337b6d53e8bb0eaf0470ae9f070a4d4eb55";
  outputs =
    _:
    let
      unused = 1;
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
