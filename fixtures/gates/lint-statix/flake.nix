{
  description = "gate 2 fixture: a statix finding fails under hard";
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
      pkgs_set = {
        x = 1;
      };
    in
    {
      packages = {
        x86_64-linux.default =
          let
            x = pkgs_set.x;
          in
          if x == 1 then drv "x86_64-linux" else drv "x86_64-linux";
        aarch64-linux.default = drv "aarch64-linux";
      };
    };
}
