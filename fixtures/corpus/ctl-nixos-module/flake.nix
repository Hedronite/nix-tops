{
  description = "Corpus control: a nixos-module that passes";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/04906337b6d53e8bb0eaf0470ae9f070a4d4eb55";
  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
    in
    {
      nixosModules.default = { config, lib, ... }: {
        options.corpus.enable = lib.mkEnableOption "the corpus service";
        config = lib.mkIf config.corpus.enable { environment.etc."corpus".text = "on"; };
      };
      # Exercises the module by evaluating a configuration's drvPath, eval only. It is a checks
      # entry, not a nixosConfigurations output, so the flake stays a nixos-module and not a nixos-host.
      checks = nixpkgs.lib.genAttrs systems (system: {
        module-eval =
          let
            toplevel = (nixpkgs.lib.nixosSystem {
              inherit system;
              modules = [
                self.nixosModules.default
                {
                  corpus.enable = true;
                  boot.loader.grub.devices = [ "nodev" ];
                  fileSystems."/" = { device = "/dev/null"; fsType = "ext4"; };
                  system.stateVersion = "26.11";
                }
              ];
            }).config.system.build.toplevel;
          in
          builtins.derivation {
            name = "module-eval";
            inherit system;
            builder = "/bin/sh";
            args = [ "-c" "echo ${builtins.unsafeDiscardStringContext toplevel.drvPath} > $out" ];
          };
      });
    };
}
