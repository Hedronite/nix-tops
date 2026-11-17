{
  description = "source 2: the same renamed option through Home Manager as a NixOS module";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/1c872b8c508bbbfebc1b0b74777ff391ce0a574c";
  inputs.home-manager.url = "github:nix-community/home-manager/9d7ba6bef3904a018d38b92f83132eaf01eb8ef3";
  inputs.home-manager.inputs.nixpkgs.follows = "nixpkgs";
  outputs = { self, nixpkgs, home-manager }: {
    nixosConfigurations.fixture = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        home-manager.nixosModules.home-manager
        {
          boot.loader.grub.devices = [ "nodev" ];
          fileSystems."/" = { device = "/dev/null"; fsType = "ext4"; };
          system.stateVersion = "26.05";
          users.users.fixture.isNormalUser = true;
          home-manager.users.fixture = {
            home.stateVersion = "26.05";
            programs.carapace.ignoreCase = true;
          };
        }
      ];
    };
  };
}
