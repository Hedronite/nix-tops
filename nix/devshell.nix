# Contributor shell: the Rust toolchain of the kit's pin plus every pinned tool.
{
  pkgs,
  mkShell,
}:
let
  kit = import ./tools.nix { inherit pkgs; };
in
mkShell {
  packages = [
    kit.judge
    pkgs.cargo
    pkgs.rustc
    pkgs.clippy
    pkgs.rustfmt
    pkgs.nixfmt-tree
  ]
  ++ builtins.attrValues kit.tools;
}
