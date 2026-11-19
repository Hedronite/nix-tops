{
  description = "gate 3 fixture: a builtins.warn in an output, listed in the ledger";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/04906337b6d53e8bb0eaf0470ae9f070a4d4eb55";
  outputs = _: {
    packages.x86_64-linux.default = builtins.warn "the fixture package has been renamed" (builtins.derivation { name = "fixture"; system = "x86_64-linux"; builder = "/bin/sh"; });
  };
}
