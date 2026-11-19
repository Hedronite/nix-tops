{
  description = "gate 3 fixture, warning source 2: a renamed Home Manager option in a standalone configuration";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/1c872b8c508bbbfebc1b0b74777ff391ce0a574c";
  inputs.home-manager.url = "github:nix-community/home-manager/9d7ba6bef3904a018d38b92f83132eaf01eb8ef3";
  inputs.home-manager.inputs.nixpkgs.follows = "nixpkgs";
  outputs = { nixpkgs, home-manager, ... }: {
    packages.x86_64-linux.default = builtins.derivation { name = "fixture"; system = "x86_64-linux"; builder = "/bin/sh"; };
    homeConfigurations.fixture = home-manager.lib.homeManagerConfiguration {
      pkgs = nixpkgs.legacyPackages.x86_64-linux;
      modules = [{
        home.username = "fixture";
        home.homeDirectory = "/home/fixture";
        home.stateVersion = "26.05";
        # renamed to programs.carapace.environment.CARAPACE_MATCH
        programs.carapace.ignoreCase = true;
      }];
    };
  };
}
