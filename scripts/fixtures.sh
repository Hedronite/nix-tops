#!/usr/bin/env bash
# Corpus runner for fixtures/corpus/* (and, from phase 2, fixtures/gates/*).
#
#   scripts/fixtures.sh raw [name...]    run `nix flake check` with gate 3's options on each fixture and
#                                        compare its exit with expected.json "raw.exit"; print a table.
#                                        Evaluation only, no build, no --impure, locks never updated.
#   scripts/fixtures.sh gate [name...]   run `nixtops gate` on each fixture: phase 2, not built here.
#
# A fixture with setup.sh is built into a scratch git repository first (a fixture inside this
# repository is tracked, so untracked and staged cases cannot be made in place).
# Environment: NIX (default nix). Needs python3 to read expected.json.
set -u
mode=${1:-}
[ $# -gt 0 ] && shift
root=$(cd "$(dirname "$0")/.." && pwd)
nix=${NIX:-nix}
case $mode in
  raw) ;;
  gate) echo "fixtures.sh gate: runs nixtops gate over the corpus; added in phase 2 (G4 built)" >&2; exit 4 ;;
  *) echo "usage: scripts/fixtures.sh raw|gate [name...]" >&2; exit 2 ;;
esac
names=("$@")
if [ ${#names[@]} -eq 0 ]; then
  for d in "$root"/fixtures/corpus/*/; do names+=("$(basename "$d")"); done
fi
cache=$(mktemp -d); scratch_all=$(mktemp -d)
trap 'rm -rf "$cache" "$scratch_all"' EXIT
bad=0
for n in "${names[@]}"; do
  dir=$root/fixtures/corpus/$n
  [ -f "$dir/expected.json" ] || { echo "no fixture $n" >&2; bad=1; continue; }
  want=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["raw"]["exit"])' "$dir/expected.json")
  target=path:$dir
  if [ -f "$dir/setup.sh" ]; then
    target=$scratch_all/$n; mkdir "$target"
    sh "$dir/setup.sh" "$target" >/dev/null 2>&1 || { echo "setup failed: $n" >&2; bad=1; continue; }
  fi
  log=$scratch_all/$n.log
  XDG_CACHE_HOME=$cache "$nix" --extra-experimental-features 'nix-command flakes' flake check \
    --no-build --all-systems --no-update-lock-file \
    --option abort-on-warn true --option allow-import-from-derivation false "$target" >"$log" 2>&1
  got=$?
  if [ "$got" = "$want" ]; then verdict=ok; else verdict=DIFF; bad=1; fi
  printf '%-28s raw exit %s (expected %s) %s\n' "$n" "$got" "$want" "$verdict"
  [ -n "${FIXTURES_LOGS:-}" ] && { mkdir -p "$FIXTURES_LOGS"; cp "$log" "$FIXTURES_LOGS/$n.log"; }
done
exit $bad
