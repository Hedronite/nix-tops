# The tools the kit pins (protocol section 2), taken from the kit's nixpkgs lock. The package
# reads their versions from here at build time, so `nixtops version` follows the lock.
{ pkgs }:
{
  # The judge: CppNix 2.35.x from the lock. The host supplies the daemon and the store only.
  judge = pkgs.nixVersions.nix_2_35;

  tools = {
    inherit (pkgs)
      nixfmt
      treefmt
      statix
      deadnix
      nix-unit
      gitleaks
      vulnix
      check-jsonschema
      nix-eval-jobs
      nix-fast-build
      nix-tree
      ;
  };
}
