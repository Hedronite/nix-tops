{
  description = "gate fixture: a clean flake";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/04906337b6d53e8bb0eaf0470ae9f070a4d4eb55";
  outputs = _: {
    packages.x86_64-linux.default = builtins.derivation { name = "fixture"; system = "x86_64-linux"; builder = "/bin/sh"; };
    packages.aarch64-linux.default = builtins.derivation { name = "fixture"; system = "aarch64-linux"; builder = "/bin/sh"; };
    devShells.x86_64-linux.default = builtins.derivation { name = "fixture"; system = "x86_64-linux"; builder = "/bin/sh"; };
    devShells.aarch64-linux.default = builtins.derivation { name = "fixture"; system = "aarch64-linux"; builder = "/bin/sh"; };
  };
}
