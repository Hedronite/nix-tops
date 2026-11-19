{
  description = "gate 3 fixture: a NixOS configuration sets an option that does not exist";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/04906337b6d53e8bb0eaf0470ae9f070a4d4eb55";
  outputs = { nixpkgs, ... }: {
    packages.x86_64-linux.default = builtins.derivation { name = "fixture"; system = "x86_64-linux"; builder = "/bin/sh"; };
    nixosConfigurations.host = nixpkgs.lib.nixosSystem {
      modules = [
        { nixpkgs.hostPlatform = "x86_64-linux"; system.stateVersion = "25.11"; boot.loader.grub.enable = false; fileSystems."/" = { device = "none"; fsType = "tmpfs"; }; }
        { nonexistent.option = 1; }
      ];
    };
  };
}
