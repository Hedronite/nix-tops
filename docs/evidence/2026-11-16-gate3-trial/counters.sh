#!/usr/bin/env bash
# Eval counters. NIX_SHOW_STATS runs against the rust-tops devShell pinned in refs.txt.
# Eval only (nix eval --raw ...drvPath), never --impure.
#
# usage: counters.sh cppnix|determinate
#   A  3 runs, no eval cache, aarch64-darwin devShell       (determinism)
#   B  2 runs, --option eval-cores 1                        (CppNix ignores the option)
#   C  2 runs with the eval cache, from an empty cache dir  (eval-cache delta)
#   D  2 runs, no eval cache, x86_64-linux devShell         (cross-system delta)
#   E  1 run with NIX_COUNT_CALLS=1                         (per-function counts)
set -u
impl=${1:?usage: counters.sh cppnix|determinate}
here=$(cd "$(dirname "$0")" && pwd)
export PATH=/nix/var/nix/profiles/default/bin:$PATH
out=$here/counters/$impl
mkdir -p "$out"
X=(--extra-experimental-features nix-command --extra-experimental-features flakes)
sha=$(awk '$1=="Hedronite/rust-tops"{print $2}' "$here/refs.txt")
R=github:Hedronite/rust-tops/$sha

nix --version > "$out/version.txt"
show() { tr ',{}' '\n\n\n' < "$1" | grep -E '"(nrFunctionCalls|nrThunks|nrPrimOpCalls|nrLookups)"' | tr -d ' \n'; echo; }
run() { # run <tag> <env-prefix...> -- <nix args...>
  local tag=$1; shift
  NIX_SHOW_STATS=1 NIX_SHOW_STATS_PATH=$out/$tag.json "$@" >/dev/null 2>"$out/$tag.err"
  [ -s "$out/$tag.err" ] || rm -f "$out/$tag.err"
}
{
echo "== A: no eval cache, aarch64-darwin devShell, x3"
for i in 1 2 3; do run sa$i nix "${X[@]}" eval --no-eval-cache --raw "$R#devShells.aarch64-darwin.default.drvPath"; echo "sa$i $(show "$out/sa$i.json")"; done
echo "== B: --option eval-cores 1, x2"
for i in 1 2; do run sb$i nix "${X[@]}" eval --no-eval-cache --option eval-cores 1 --raw "$R#devShells.aarch64-darwin.default.drvPath"; echo "sb$i $(show "$out/sb$i.json")"; done
echo "== C: eval cache on, empty cache dir, x2 (run 1 cold, run 2 warm)"
cache=$(mktemp -d)
for i in 1 2; do run sc$i env XDG_CACHE_HOME="$cache" nix "${X[@]}" eval --raw "$R#devShells.aarch64-darwin.default.drvPath"; echo "sc$i $(show "$out/sc$i.json")"; done
rm -rf "$cache"
echo "== D: no eval cache, x86_64-linux devShell, x2"
for i in 1 2; do run sd$i nix "${X[@]}" eval --no-eval-cache --raw "$R#devShells.x86_64-linux.default.drvPath"; echo "sd$i $(show "$out/sd$i.json")"; done
echo "== E: NIX_COUNT_CALLS"
run se env NIX_COUNT_CALLS=1 nix "${X[@]}" eval --no-eval-cache --raw "$R#devShells.aarch64-darwin.default.drvPath"
python3 -I - "$out/se.json" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
f = d.get("functions", [])
print("top-level keys:", ", ".join(sorted(d)))
print("function entries:", len(f))
print("entries outside nixpkgs:", len([x for x in f if "nixpkgs" not in str(x.get("file", "")) and x.get("file")]))
PY
} | tee "$out/summary.txt"
