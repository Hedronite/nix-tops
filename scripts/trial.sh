#!/usr/bin/env bash
# The kit's own command over the eight pinned trial flakes, eval only.
#
#   scripts/trial.sh [out-dir]
#
# For each line of docs/evidence/2026-11-16-gate3-trial/refs.txt (`owner/repo <40-hex commit>`):
#   1. fetch that commit into a temporary directory (the ref itself is never edited);
#   2. `nixtops outputs --json` gives the failing outputs: a failing output is one with an `error`;
#   3. a flake with no failing output also goes through the gate: `nixtops init` writes a throwaway
#      config there, the trial sets `enforce = "hard"`, drops the baseline key and switches `lint`,
#      `build-test`, `coverage`, `facet-layers` and `laws` off with the reason "trial: eval only",
#      commits it in the temporary directory, and `nixtops gate` runs (pinned judge, no build,
#      no --impure). A flake whose outputs fail (the `x86_64-darwin` rows) cannot get a valid
#      `[toolchain].systems`, so for those the failing set of step 2 is the result and the table says so.
# The result is compared with EXPECTED below: the exit code, the number of failing outputs and the
# shape of their names, never log text. A difference is a finding: the table prints it, the exit
# code is 1, and the expected column is changed only with the cause written next to it.
#
# Writes <out-dir>/table.md and <out-dir>/results.json. Environment: NIXTOPS (default nixtops).
# Needs git, python3 and network access to GitHub and the binary cache.
set -u
root=$(cd "$(dirname "$0")/.." && pwd)
out=${1:-trial-out}
bin=${NIXTOPS:-nixtops}
refs=$root/docs/evidence/2026-11-16-gate3-trial/refs.txt
mkdir -p "$out"; out=$(cd "$out" && pwd)
work=$(mktemp -d); trap 'rm -rf "$work"' EXIT
: > "$out/rows.tsv"
while read -r repo sha; do
  [ -n "$repo" ] || continue
  case $sha in *[!0-9a-f]*|"") echo "refs.txt: $repo: not a 40-hex commit" >&2; exit 2 ;; esac
  [ ${#sha} -eq 40 ] || { echo "refs.txt: $repo: not a 40-hex commit" >&2; exit 2; }
  dir=$work/$(echo "$repo" | tr / -); mkdir "$dir"
  ( cd "$dir" && git init -q . && git fetch -q --depth 1 "https://github.com/$repo" "$sha" && git checkout -q FETCH_HEAD ) \
    || { printf '%s\t%s\tfetch failed\t-\t-\n' "$repo" "$sha" >> "$out/rows.tsv"; continue; }
  name=$(basename "$repo")
  "$bin" outputs --json "$dir" > "$out/$name.outputs.json" 2> "$out/$name.outputs.err"
  method=outputs; gate_exit=-
  failing=$(python3 -c 'import json,sys
try:
    r = json.load(open(sys.argv[1])); print(len([o for o in r["outputs"] if o.get("error")]))
except Exception:
    print("x")' "$out/$name.outputs.json")
  if [ "$failing" = 0 ]; then
    method=gate
    ( cd "$dir" && "$bin" init >/dev/null 2>"$out/$name.init.err" ) || method="gate (init failed)"
    if [ "$method" = gate ]; then
      python3 - "$dir/nix-tops.toml" <<'PY'
import re, sys
p = sys.argv[1]
t = open(p).read()
t = t.replace('enforce = "ratchet"', 'enforce = "hard"')
t = re.sub(r'^baseline = .*\n', '', t, flags=re.M)
off = "".join(f'{k} = {{ enabled = false, reason = "trial: eval only" }}\n'
              for k in ["lint", "build-test", "coverage", "facet-layers", "laws"])
if "[layers]\n" in t:
    t = t.replace("[layers]\n", "[layers]\n" + off, 1)
else:
    t = t.rstrip("\n") + "\n\n[layers]\n" + off  # init writes the table only when a facet maps a layer off
open(p, "w").write(t)
PY
      ( cd "$dir" && git add -A && git -c user.name=trial -c user.email=trial@example.invalid commit -q -m trial )
      "$bin" gate "$dir" --json > "$out/$name.gate.json" 2> "$out/$name.gate.err"
      gate_exit=$?
    fi
  fi
  printf '%s\t%s\t%s\t%s\t%s\n' "$repo" "$sha" "$method" "$gate_exit" "$failing" >> "$out/rows.tsv"
  cp "$out/$name.outputs.json" "$work/$name.outputs.json"
done < "$refs"
python3 - "$out" <<'PY'
import json, re, sys
from pathlib import Path
out = Path(sys.argv[1])
DARWIN = r"\.x86_64-darwin(\.|$)"
# The 2026-11-17 re-run in docs/evidence/2026-11-16-gate3-trial/README.md, one row per ref: the exit
# code of the check, how many outputs fail, and what their names look like. The refs predate the drop
# of x86_64-darwin, so those rows stay failing at these pins.
EXPECTED = {
    "luminosi-ts":    {"exit": 1, "failing": 1,    "names": DARWIN},
    "rust-tops":      {"exit": 1, "failing": 5,    "names": DARWIN},
    "python-gate":    {"exit": 0, "failing": 0,    "names": None},
    "Omahedron":      {"exit": 0, "failing": 0,    "names": None},
    "lapis-lattice":  {"exit": 1, "failing": 5,    "names": DARWIN},
    "hedronetes-h3s": {"exit": 1, "failing": None, "names": r"^packages\.x86_64-linux(\.|$)"},
    "facet":          {"exit": 0, "failing": 0,    "names": None},
    "bezel":          {"exit": 1, "failing": 29,   "names": DARWIN},
}
# Differences from EXPECTED that are understood. A known difference is printed with its cause and does
# not fail the run; any other difference does. The expected column above is the README's and is not
# edited to make a row agree.
KNOWN = {
    "rust-tops": {"failing": 6, "cause":
        "the inventory also lists packages.x86_64-darwin, the whole-set record of the system's packages that the "
        "nixpkgs warning fails under abort-on-warn, besides the five attribute paths the README counted; all six are "
        "x86_64-darwin"},
    "hedronetes-h3s": {"names": r"^devShells\.x86_64-linux$", "cause":
        "flake.nix line 163 sets devShells.\"x86_64-linux\" to the same unsupported-system throw as packages, so two "
        "outputs fail on a cold cache too (the second reads as a cached failure within one evaluation); the README "
        "names only packages"},
}
rows, bad = [], 0
for line in (out / "rows.tsv").read_text().splitlines():
    repo, sha, method, gate_exit, failing = line.split("\t")
    name = repo.split("/")[1]
    want = EXPECTED[name]
    problems, names, errs = [], [], {}
    try:
        outs = json.loads((out / f"{name}.outputs.json").read_text())["outputs"]
        names = [o["attr"] for o in outs if o.get("error")]
        errs = {o["attr"]: str(o["error"])[:300] for o in outs if o.get("error")}
    except Exception:
        problems.append("no outputs inventory" if method != "fetch failed" else "fetch failed")
    if method.startswith("gate"):
        exit_code = int(gate_exit) if gate_exit.isdigit() else -1
        if method != "gate":
            problems.append("init failed")
    else:
        exit_code = 1 if names else 0
    if not problems or names:
        if exit_code != want["exit"]:
            problems.append(f"exit {exit_code}, expected {want['exit']}")
        if want["failing"] is not None and len(names) != want["failing"]:
            problems.append(f"{len(names)} failing outputs, expected {want['failing']}")
        if want["names"] and any(not re.search(want["names"], n) for n in names):
            problems.append("a failing output outside the expected shape: " + ", ".join(n for n in names if not re.search(want["names"], n))[:120])
    known = KNOWN.get(name, {})
    notes = []
    kept = []
    for pr in problems:
        if "failing outputs, expected" in pr and known.get("failing") == len(names):
            notes.append(known["cause"])
        elif pr.startswith("a failing output outside") and known.get("names") and \
                all(re.search(want["names"], n) or re.search(known["names"], n) for n in names):
            notes.append(known["cause"])
        else:
            kept.append(pr)
    problems = kept
    bad += bool(problems)
    rows.append({"repo": repo, "commit": sha, "method": method, "exit": exit_code, "failing": names,
                 "errors": errs, "expected": want, "problems": problems, "known": sorted(set(notes))})
md = ["| Flake | Commit | Method | Exit | Failing outputs | Expected | Result |", "|---|---|---|---|---|---|---|"]
for r in rows:
    w = r["expected"]
    exp = f"exit {w['exit']}, " + ("any number" if w["failing"] is None else f"{w['failing']}") + " failing"
    how = r["method"] if r["method"].startswith("gate") else "outputs only (the config cannot be written)" if r["failing"] else r["method"]
    md.append(f"| {r['repo']} | `{r['commit'][:7]}` | {how} | {r['exit']} | {len(r['failing'])} | {exp} | "
              + ("DIFFERENT: " + "; ".join(r["problems"]) if r["problems"]
                 else "known difference: " + " ".join(r["known"]) if r["known"] else "same") + " |")
(out / "table.md").write_text("\n".join(md) + "\n")
(out / "results.json").write_text(json.dumps(rows, indent=2) + "\n")
print("\n".join(md))
sys.exit(1 if bad else 0)
PY
