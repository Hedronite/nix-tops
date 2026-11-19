#!/usr/bin/env bash
# The nine gate fixtures (fixtures/gates/) run through the kit's own `nixtops gate`.
#
# usage: run.sh <label> <nixtops> [extra gate flags...]
#   label    what is under test, used in the log names (for example determinate-pinned)
#   nixtops  the kit's runner binary (the nix build of this repository)
#   flags    passed to `nixtops gate`, for example `--evaluator host`
#
# Each fixture is copied into a scratch directory that is its own git repository, so the source
# layer sees a clean tracked tree. Eval only: no build, no --impure, locks never updated.
# For every fixture it writes <label>/<name>.json (the report, paths scrubbed), <name>.err and a
# line in summary.txt. The exit code is the gate's own.
set -u
label=${1:?usage: run.sh <label> <nixtops> [flags...]}
bin=${2:?}
shift 2
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../.." && pwd)
out=$here/logs/$label
mkdir -p "$out"
: > "$out/summary.txt"
scratch=$(mktemp -d)
for fixture in "$repo"/fixtures/gates/*/; do
  name=$(basename "$fixture")
  dir=$scratch/$name
  cp -R "$fixture" "$dir"
  ( cd "$dir" && git init -q . && git config user.email a@b && git config user.name n && git add -A && git commit -q -m fixture )
  "$bin" gate "$dir" --json "$@" > "$out/$name.json" 2> "$out/$name.err"
  code=$?
  python3 - "$out/$name.json" "$name" "$code" >> "$out/summary.txt" <<'PY'
import json, sys
path, name, code = sys.argv[1:4]
try:
    layers = {l["id"]: l["verdict"] for l in json.load(open(path))["layers"]}
except Exception:
    layers = {}
print(f'{name:24} exit {code}  source {layers.get("source", "-"):8} eval {layers.get("eval", "-")}')
PY
  for f in "$out/$name.json" "$out/$name.err"; do
    sed -e "s#$scratch#<scratch>#g" -e "s#$HOME#<home>#g" "$f" > "$f.tmp" && mv "$f.tmp" "$f"
  done
done
rm -rf "$scratch"
cat "$out/summary.txt"
