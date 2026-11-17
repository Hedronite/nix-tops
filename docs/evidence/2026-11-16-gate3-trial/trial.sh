#!/usr/bin/env bash
# Gate-3 trial. Runs the NIX_TOPS.md section 3.1 gate 3 command (eval only) against
# every flake in refs.txt, one 40-hex commit each, and writes logs under logs/<impl>/.
#
# usage: trial.sh cppnix|determinate
#   cppnix       CppNix; flakes and nix-command are off by default, so the flags below enable them
#   determinate  Determinate Nix; the same flags are accepted and change nothing
#
# Eval only: --no-build, no --impure, no unlocked reference.
set -u
impl=${1:?usage: trial.sh cppnix|determinate}
here=$(cd "$(dirname "$0")" && pwd)
export PATH=/nix/var/nix/profiles/default/bin:$PATH
out=$here/logs/$impl
mkdir -p "$out"
X=(--extra-experimental-features nix-command --extra-experimental-features flakes)

nix --version > "$out/version.txt"
: > "$out/exit-codes.txt"
: > "$out/summary.txt"

while read -r repo sha; do
  [ -n "$repo" ] || continue
  if [[ ! $sha =~ ^[0-9a-f]{40}$ ]]; then echo "refs.txt: not a 40-hex commit: $repo $sha" >&2; exit 2; fi
  name=${repo##*/}
  log=$out/gate3-$name.log
  nix "${X[@]}" flake check --no-build --all-systems --no-update-lock-file \
    --option abort-on-warn true --option allow-import-from-derivation false \
    "github:$repo/$sha" > "$log" 2>&1
  code=$?
  echo "$repo ${sha:0:10} exit=$code" >> "$out/exit-codes.txt"
  {
    echo "######## $repo@${sha:0:10} exit=$code"
    grep -v -E '^(copying|unpacking|fetching|downloading|\[)' "$log" \
      | grep -i -E 'warning|error|unknown|unchecked|lacks|omitted|missing|does not|throw|not support' \
      | cut -c1-300 | sort | uniq -c | head -14
  } >> "$out/summary.txt"
done < "$here/refs.txt"
cat "$out/exit-codes.txt"
