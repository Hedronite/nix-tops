#!/usr/bin/env bash
# Evaluate the quickstart adopter against the stub kit. Eval only; never --impure.
# The adopter's flake.lock pins nixpkgs; the nix-tops input is overridden to the stub at run time.
set -eu
export PATH=/nix/var/nix/profiles/default/bin:$PATH
here=$(cd "$(dirname "$0")" && pwd)
nix=(nix --extra-experimental-features 'nix-command flakes')
ov=(--override-input nix-tops "path:$here/stub-kit")
"${nix[@]}" --version
"${nix[@]}" flake check --no-build --all-systems --no-update-lock-file \
  --option abort-on-warn true --option allow-import-from-derivation false "${ov[@]}" "path:$here"
# the adopter's own check survives the merge with mkChecks, on every system; lint exists on one
for s in x86_64-linux aarch64-linux aarch64-darwin; do
  printf '%s: ' "$s"
  "${nix[@]}" eval --json --no-update-lock-file "${ov[@]}" "path:$here#checks.$s" --apply builtins.attrNames 2>/dev/null
done

# the stub hides nothing the interface would raise: rename the adopter's own check in a copy and
# confirm the rename is what the merged checks show (the stub must not swallow or keep the old name)
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
cp -R "$here/." "$tmp/q"
perl -pi -e 's/\{ package = self\.packages/{ renamed = self.packages/' "$tmp/q/flake.nix"
printf 'renamed copy: '
"${nix[@]}" eval --json --no-write-lock-file --override-input nix-tops "path:$tmp/q/stub-kit" \
  "path:$tmp/q#checks.x86_64-linux" --apply builtins.attrNames 2>/dev/null
