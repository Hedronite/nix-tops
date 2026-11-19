#!/usr/bin/env bash
# Corpus runner for fixtures/corpus/* (and, from phase 2, fixtures/gates/*).
#
#   scripts/fixtures.sh raw [name...]    run `nix flake check` with gate 3's options on each fixture and
#                                        compare its exit with expected.json "raw.exit"; print a table.
#                                        Evaluation only, no build, no --impure, locks never updated.
#   scripts/fixtures.sh gate [name...]   run `nixtops gate --json` on each fixture of fixtures/corpus/* and
#                                        fixtures/gates/* and compare the exit code, each layer's verdict
#                                        and `named` with expected.json; print a table, exit 1 on any
#                                        difference. Eval only. Each fixture runs in a scratch directory
#                                        that is its own git repository (a clean tracked tree for gate 1).
#
# A fixture with setup.sh is built into a scratch git repository first (a fixture inside this
# repository is tracked, so untracked and staged cases cannot be made in place).
# Environment: NIX (default nix); NIXTOPS (default nixtops) and GATE_FLAGS (for example
# `--evaluator host`) for the gate mode. Needs python3 to read expected.json.
set -u
mode=${1:-}
[ $# -gt 0 ] && shift
root=$(cd "$(dirname "$0")/.." && pwd)
nix=${NIX:-nix}
case $mode in
  raw) ;;
  gate) ;;
  *) echo "usage: scripts/fixtures.sh raw|gate [name...]" >&2; exit 2 ;;
esac

# gate_main [corpus/<name> | gates/<name> ...]  -- a bare name is looked up in corpus, then gates.
gate_main() {
  local bin=${NIXTOPS:-nixtops} scratch_all fx bad=0 n dir target code out
  scratch_all=$(mktemp -d); trap 'rm -rf "$scratch_all"' RETURN
  local -a todo=("$@")
  if [ ${#todo[@]} -eq 0 ]; then
    for d in "$root"/fixtures/corpus/*/ "$root"/fixtures/gates/*/; do
      todo+=("$(basename "$(dirname "$d")")/$(basename "$d")")
    done
  fi
  for fx in "${todo[@]}"; do
    case $fx in */*) ;; *) if [ -d "$root/fixtures/corpus/$fx" ]; then fx=corpus/$fx; else fx=gates/$fx; fi ;; esac
    dir=$root/fixtures/$fx; n=$(basename "$fx")
    [ -f "$dir/expected.json" ] || { echo "no fixture $fx" >&2; bad=1; continue; }
    target=$scratch_all/$(echo "$fx" | tr / -); mkdir "$target"
    if [ -f "$dir/setup.sh" ]; then
      sh "$dir/setup.sh" "$target" >/dev/null 2>&1 || { echo "setup failed: $fx" >&2; bad=1; continue; }
    else
      cp -R "$dir"/. "$target"/ && rm -f "$target"/expected.json
      ( cd "$target" && git init -q . && git add -A &&
        git -c user.name=fixture -c user.email=fixture@example.invalid commit -q -m fixture )
    fi
    out=$scratch_all/$(echo "$fx" | tr / -).json
    # shellcheck disable=SC2086
    XDG_CACHE_HOME=$scratch_all/cache "$bin" gate "$target" --json ${GATE_FLAGS:-} >"$out" 2>"$out.err"
    code=$?
    [ -n "${FIXTURES_LOGS:-}" ] && { mkdir -p "$FIXTURES_LOGS"; t=$(echo "$fx" | tr / -); cp "$out" "$FIXTURES_LOGS/$t.json"; cp "$out.err" "$FIXTURES_LOGS/$t.err"; }
    python3 - "$dir/expected.json" "$out" "$code" "$fx" <<'PY' || bad=1
import json, sys
exp_path, rep_path, code, name = sys.argv[1:5]
exp = json.load(open(exp_path))
try:
    text = open(rep_path).read()
    rep = json.loads(text)
    got = {l["id"]: l["verdict"] for l in rep.get("layers", [])}
except Exception:
    text, got = "", {}
want = exp.get("layers", {})
problems = []
if int(code) != exp["exit"]:
    problems.append(f"exit {code}, expected {exp['exit']}")
for layer, verdict in want.items():
    if got.get(layer) != verdict:
        problems.append(f"{layer} {got.get(layer, '-')}, expected {verdict}")
if exp.get("named") and not any(alt in text for alt in exp["named"].split("|")):
    problems.append(f"report does not name {exp['named']!r}")
for needle in exp.get("failing_outputs", []):
    if needle not in text:
        problems.append(f"report does not name output {needle!r}")
print(f"{name:42} exit {code} " + ("ok" if not problems else "DIFF: " + "; ".join(problems)))
sys.exit(1 if problems else 0)
PY
  done
  return $bad
}
names=("$@")
if [ "$mode" = gate ]; then
  gate_main "$@"
  exit $?
fi
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
