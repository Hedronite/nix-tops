#!/usr/bin/env bash
# The pinned CppNix client (the judge) evaluating the kit against the host's daemon.
#
# usage: run.sh <label> <clone> <rev>
#   label  the daemon under test, used in the log names (for example determinate-daemon)
#   clone  a git clone that contains <rev>
#   rev    a 40-hex commit of the kit
#
# The judge is taken from the nixpkgs revision locked in the kit's flake.lock, so it is the one
# `nixtops version` prints. Eval only: --no-build, no --impure, no unlocked reference. Every run
# gets an empty XDG_CACHE_HOME so the eval cache cannot replay an earlier result.
set -u
label=${1:?usage: run.sh <label> <clone> <rev>}
clone=${2:?}
rev=${3:?}
[[ $rev =~ ^[0-9a-f]{40}$ ]] || { echo "rev must be 40 hex" >&2; exit 2; }
here=$(cd "$(dirname "$0")" && pwd)
out=$here/logs/$label
mkdir -p "$out"
export PATH=/nix/var/nix/profiles/default/bin:$PATH
X=(--extra-experimental-features nix-command --extra-experimental-features flakes)

nixpkgs_rev=$(git -C "$clone" show "$rev:flake.lock" | python3 -c 'import json,sys; print(json.load(sys.stdin)["nodes"]["nixpkgs"]["locked"]["rev"])')
judge=
for p in $(nix "${X[@]}" build --no-link --print-out-paths --no-accept-flake-config \
  "github:NixOS/nixpkgs/$nixpkgs_rev#nixVersions.nix_2_35"); do
  [ -x "$p/bin/nix" ] && judge=$p/bin/nix
done
[ -n "$judge" ] || { echo "no bin/nix in the judge outputs" >&2; exit 2; }
flake="git+file://$clone?rev=$rev"
cache=$(mktemp -d)
run() { XDG_CACHE_HOME=$cache "$judge" "${X[@]}" "$@" --no-accept-flake-config; }

{
  echo "judge: $judge"
  "$judge" --version
  echo "host nix: $(nix --version)"
  echo "daemon: $(run store info --json)"
} > "$out/judge.txt" 2>&1

run flake check --no-build --all-systems --no-update-lock-file \
  --option abort-on-warn true --option allow-import-from-derivation false "$flake" \
  > "$out/flake-check.log" 2>&1
echo "flake-check exit=$?" > "$out/exit-codes.txt"

run flake show --json --all-systems --no-update-lock-file "$flake" > "$out/flake-show.json" 2> "$out/flake-show.err"
echo "flake-show exit=$?" >> "$out/exit-codes.txt"

for s in aarch64-darwin x86_64-linux aarch64-linux; do
  echo "$s $(run eval --raw --no-update-lock-file "$flake#packages.$s.nixtops.drvPath" 2>/dev/null)"
done > "$out/drv-paths.txt"
rm -rf "$cache"
# Logs name the clone and the home directory; neither belongs in the record.
for f in "$out"/*; do
  sed -e "s#$clone#<clone>#g" -e "s#$HOME#<home>#g" "$f" > "$f.tmp" && mv "$f.tmp" "$f"
done
cat "$out/exit-codes.txt"
