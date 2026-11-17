{
  description = "Stub of the Nix-TOPS kit interface: the shape an adopter consumes, with no behaviour";
  outputs =
    { self }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "aarch64-darwin" ];
      forSystems = f: builtins.listToAttrs (map (s: { name = s; value = f s; }) systems);
      stubDrv = system: name: builtins.derivation { inherit name system; builder = "/bin/sh"; };
    in
    {
      lib = {
        # Returns checks.<system>.<name>, per system, for the adopter to merge with its own.
        # Lint checks exist for one system only (the first declared).
        mkChecks =
          { self, systems }:
          let
            # The documented read (spec section 4): the config is TOML, read without IFD. A missing
            # or invalid file fails evaluation, as it would for the real kit; the class names the check.
            cfg = builtins.fromTOML (builtins.readFile (self + "/nix-tops.toml"));
          in
          builtins.listToAttrs (
            map (system: {
              name = system;
              value =
                { nixtops-config = stubDrv system "nixtops-config-${cfg.class}"; }
                // (if system == builtins.head systems then { nixtops-lint = stubDrv system "nixtops-lint"; } else { });
            }) systems
          );
      };
      apps = forSystems (system: {
        nixtops = {
          type = "app";
          program = "/bin/true";
          meta.description = "Stub nixtops runner";
        };
      });
      formatter = forSystems (system: stubDrv system "nixtops-formatter");
    };
}
