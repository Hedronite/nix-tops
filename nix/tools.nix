# The tools the kit pins (protocol section 2), taken from the kit's nixpkgs lock. The package
# reads their versions from here at build time, so `nixtops version` follows the lock.
{ pkgs }:
{
  # The judge: CppNix 2.35.x from the lock. The host supplies the daemon and the store only.
  judge = pkgs.nixVersions.nix_2_35;

  tools = {
    nixfmt = pkgs.nixfmt;
    treefmt = pkgs.treefmt;
    statix = pkgs.statix;
    deadnix = pkgs.deadnix;
    nix-unit = pkgs.nix-unit;
    gitleaks = pkgs.gitleaks;
    vulnix = pkgs.vulnix;
    check-jsonschema = pkgs.check-jsonschema;
    nix-eval-jobs = pkgs.nix-eval-jobs;
    nix-fast-build = pkgs.nix-fast-build;
    nix-tree = pkgs.nix-tree;
  };
}
