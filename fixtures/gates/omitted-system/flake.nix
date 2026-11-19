{
  description = "gate fixture: the config declares two systems and the flake exports one";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/04906337b6d53e8bb0eaf0470ae9f070a4d4eb55";
  outputs = _: {
    packages.x86_64-linux.default = builtins.derivation { name = "fixture"; system = "x86_64-linux"; builder = "/bin/sh"; };
  };
}
