#!/usr/bin/env bash
# The kit's own command, `nixtops outputs`, over two public flakes pinned by commit.
#
# usage: run.sh <label> <nixtops>
#   label    the daemon under test, used in the log names (for example determinate-daemon)
#   nixtops  the kit's runner binary (the nix build of this repo)
#
# Eval only: the runner uses the kit's pinned CppNix with gate 3's options (abort-on-warn on,
# import from derivation off) and an empty evaluation cache per run, never --impure.
set -u
label=${1:?usage: run.sh <label> <nixtops>}
bin=${2:?}
here=$(cd "$(dirname "$0")" && pwd)
out=$here/logs/$label
mkdir -p "$out"
export PATH=/nix/var/nix/profiles/default/bin:$PATH

: > "$out/exit-codes.txt"
while read -r name ref; do
  [ -n "$name" ] || continue
  [[ $ref =~ ^[0-9a-f]{40}$ ]] || { echo "refs.txt: not a 40-hex commit: $name" >&2; exit 2; }
  target="github:Hedronite/$name/$ref"
  "$bin" outputs --json "$target" > "$out/$name.json" 2> "$out/$name.err"
  echo "$name json exit=$?" >> "$out/exit-codes.txt"
  "$bin" outputs "$target" > "$out/$name.txt" 2> /dev/null
  echo "$name text exit=$?" >> "$out/exit-codes.txt"
done < "$here/refs.txt"
"$bin" version > "$out/version.txt"
nix --version > "$out/host-nix.txt"
# Logs must not carry the home directory.
for f in "$out"/*; do
  sed -e "s#$HOME#<home>#g" "$f" > "$f.tmp" && mv "$f.tmp" "$f"
done
cat "$out/exit-codes.txt"
