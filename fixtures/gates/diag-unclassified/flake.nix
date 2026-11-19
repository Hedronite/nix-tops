{
  description = "gate 3 fixture: nix prints a diagnostic that laws/diagnostics.toml does not list";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/04906337b6d53e8bb0eaf0470ae9f070a4d4eb55";
  nixConfig.extra-substituters = [ "https://cache.example.invalid" ];
  outputs = _: {
    packages.x86_64-linux.default = builtins.derivation { name = "fixture"; system = "x86_64-linux"; builder = "/bin/sh"; };
  };
}
