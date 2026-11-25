{ nixpkgs }:
{
  mkChecks = import ./mkChecks.nix { inherit nixpkgs; };
}
