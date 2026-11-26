#!/usr/bin/env python3
"""One source for every copy. Python 3 standard library only.

    python3 scripts/sync_check.py [--root DIR]

workflow    .github/workflows/nix-tops.yml is config/workflow.template.yml (what `nixtops init`
            writes) plus exactly the additions NIX_TOPS.md section 11 names: the layer check and the
            law-diff job, with the label re-run law-diff needs. Full-line comments and blank lines
            are not compared.
agents      config/AGENTS.template.md, its fields filled from each facet's profile and golden
            nix-tops.toml, is that facet's golden AGENTS.md (what `nixtops init` writes) line for
            line, and the stated rules in AGENTS.md are the template's, as a list without numbers
quickstart  the flake of NIX_TOPS.md section 4 is examples/quickstart/flake.nix, byte for byte

Exit status is 0 when every pair matches, 1 on a drift, 2 when a file or block cannot be read.
"""

import argparse
import difflib
import re
import sys
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # Python before 3.11: check-jsonschema already pulls in tomli
    import tomli as tomllib

KIT_WORKFLOW = ".github/workflows/nix-tops.yml"
TEMPLATE_WORKFLOW = "config/workflow.template.yml"
SPEC = "NIX_TOPS.md"
QUICKSTART = "examples/quickstart/flake.nix"
AGENTS_TEMPLATE = "config/AGENTS.template.md"
AGENTS = "AGENTS.md"
GOLDENS = "crates/nixtops/tests/goldens"
PROFILES = "profiles"
RULES_BEGIN = "<!-- rules:begin -->"
RULES_END = "<!-- rules:end -->"

# ------------------------------------------------- the section 11 additions to the template

GATE_RUN = "        run: nix run .#nixtops --no-update-lock-file --no-accept-flake-config -- gate --max-tier 1 --system x86_64-linux"

SETUP_PYTHON = """\
      - uses: actions/setup-python@6d4fd22b20465ccc0f7354476588e8df7c9ca0e0 # v7.1.0
        with:
          python-version: "3.12"
"""

LAYER_CHECK = SETUP_PYTHON + """\
      - name: Every enabled layer ran
        run: python3 scripts/layers_ran.py
"""

LAW_DIFF = """\
  law-diff:
    name: law-diff (base branch script)
    if: github.event_name == 'pull_request'
    runs-on: ubuntu-latest
    timeout-minutes: 10
    steps:
      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1
        with:
          fetch-depth: 0
          persist-credentials: false
          path: head
      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1
        with:
          ref: ${{ github.event.pull_request.base.sha }}
          fetch-depth: 0
          persist-credentials: false
          path: base
""" + SETUP_PYTHON + """\
      - name: law-diff
        env:
          LABELS: ${{ join(github.event.pull_request.labels.*.name, ',') }}
        run: |
          if [ ! -f base/scripts/law_diff.py ]; then
            echo "law-diff: vacuous: the base branch has no scripts/law_diff.py yet"
            echo "::notice title=law-diff::vacuous: the base branch has no scripts/law_diff.py yet"
            exit 0
          fi
          python3 -I base/scripts/law_diff.py --base base --head head --labels "$LABELS"
"""

# (where, template line, times it must occur, the lines added)
ADDITIONS = [
    ("after", "  pull_request:", 1, "    types: [opened, synchronize, reopened, labeled, unlabeled]\n"),
    (
        "replace",
        "  group: ${{ github.workflow }}-${{ github.ref }}",
        1,
        "  group: ${{ github.workflow }}-${{ github.ref }}-${{ github.event.action == 'labeled' || github.event.action == 'unlabeled' }}\n",
    ),
    ("before", "    runs-on: ubuntu-latest", 2, "    if: github.event.action != 'labeled' && github.event.action != 'unlabeled'\n"),
    ("after", GATE_RUN, 1, LAYER_CHECK),
    ("after", GATE_RUN + " --evaluator host", 1, LAYER_CHECK),
]
APPENDED = LAW_DIFF


class Unreadable(Exception):
    pass


def read(root, rel):
    try:
        return (root / rel).read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as exc:
        raise Unreadable(f"{rel}: {exc}") from exc


def significant(text):
    """[(line number, line)] without blank lines and full-line comments."""
    return [(n, line.rstrip()) for n, line in enumerate(text.splitlines(), 1) if line.strip() and not line.lstrip().startswith("#")]


def expected_workflow(template):
    """The template with the section 11 additions, or the reasons they no longer apply."""
    lines = [line for _, line in significant(template)]
    problems = [
        f"{TEMPLATE_WORKFLOW}: expected the line {anchor.strip()!r} {times} time(s), found {lines.count(anchor)}; "
        "the section 11 additions in scripts/sync_check.py no longer apply"
        for _, anchor, times, _ in ADDITIONS
        if lines.count(anchor) != times
    ]
    out = []
    for line in lines:
        before, here, after = [], [line], []
        for where, anchor, _, added in ADDITIONS:
            if line != anchor:
                continue
            block = [l for _, l in significant(added)]
            if where == "before":
                before += block
            elif where == "after":
                after += block
            else:
                here = block
        out += before + here + after
    return out + [l for _, l in significant(APPENDED)], problems


def check_workflow(root):
    want, problems = expected_workflow(read(root, TEMPLATE_WORKFLOW))
    if problems:
        return problems
    got = significant(read(root, KIT_WORKFLOW))
    matcher = difflib.SequenceMatcher(a=want, b=[line for _, line in got], autojunk=False)
    for tag, i1, i2, j1, j2 in matcher.get_opcodes():
        if tag == "equal":
            continue
        for number, line in got[j1:j2]:
            problems.append(f"{KIT_WORKFLOW}:{number}: not in the template or the section 11 additions: {line.strip()!r}")
        if i2 - i1 > 0:
            number = got[j1][0] if j1 < len(got) else (got[-1][0] + 1 if got else 1)
            for line in want[i1:i2]:
                problems.append(f"{KIT_WORKFLOW}:{number}: missing here: {line.strip()!r}")
    return problems


def toml(root, rel):
    try:
        return tomllib.loads(read(root, rel))
    except tomllib.TOMLDecodeError as exc:
        raise Unreadable(f"{rel}: {exc}") from exc


def fields(root, facet):
    """The values `nixtops init --facet <facet>` puts in the template."""
    config = toml(root, f"{GOLDENS}/{facet}/nix-tops.toml")
    profile = toml(root, f"{PROFILES}/{facet}.toml")
    # A floor a profile does not set is 0 and its closure mode is advisory, as `init` has it.
    coverage = profile.get("coverage", {})
    try:
        return {
            "FACETS": facet,
            "FLAKE": ".",
            "SYSTEMS": ", ".join(config["toolchain"]["systems"]),
            "ENFORCE": config["enforce"],
            "BUILT_FLOOR": f"{coverage.get('built_floor', 0):g}",
            "TESTED_FLOOR": f"{coverage.get('tested_floor', 0):g}",
            "CLOSURE_MODE": profile.get("budgets", {}).get("closure", {}).get("mode", "advisory"),
            "CLOSURE_OUTPUTS": "every output",
        }
    except (KeyError, TypeError, ValueError) as exc:
        raise Unreadable(f"{facet}: its golden nix-tops.toml or its profile lacks a field: {exc}") from exc


def differences(want, got):
    """[(line number in want, line number in got, want line or None, got line or None)]."""
    out = []
    matcher = difflib.SequenceMatcher(a=want, b=got, autojunk=False)
    for tag, i1, i2, j1, j2 in matcher.get_opcodes():
        if tag == "equal":
            continue
        for k in range(max(i2 - i1, j2 - j1)):
            i, j = i1 + k, j1 + k
            out.append((min(i, i2) + 1, min(j, j2) + 1, want[i] if i < i2 else None, got[j] if j < j2 else None))
    return out


def shown(line):
    return "no such line" if line is None else repr(line)


def rules(root, rel):
    """[(line number, rule text)] between the rules markers, without the list marker."""
    lines = read(root, rel).splitlines()
    try:
        begin, end = lines.index(RULES_BEGIN), lines.index(RULES_END)
    except ValueError as exc:
        raise Unreadable(f"{rel}: no {RULES_BEGIN} / {RULES_END} pair") from exc
    return [(n + 1, re.sub(r"^(\d+\.|-) ", "", lines[n])) for n in range(begin + 1, end)]


def check_agents(root):
    template = read(root, AGENTS_TEMPLATE)
    facets = sorted(p.stem for p in (root / PROFILES).glob("*.toml"))
    if not facets:
        raise Unreadable(f"{PROFILES}: no profiles")
    problems = []
    for facet in facets:
        rel = f"{GOLDENS}/{facet}/AGENTS.md"
        rendered = template
        for name, value in fields(root, facet).items():
            rendered = rendered.replace("{" + name + "}", value)
        golden = read(root, rel)
        if rendered == golden:
            continue
        found = differences(rendered.split("\n"), golden.split("\n"))
        problems += [f"{rel}:{j}: {shown(g)}; {AGENTS_TEMPLATE}:{i} renders {shown(w)}" for i, j, w, g in found]
    ours, theirs = rules(root, AGENTS), rules(root, AGENTS_TEMPLATE)
    for i, j, want, got in differences([t for _, t in theirs], [t for _, t in ours]):
        at = lambda rows, k: rows[min(k, len(rows)) - 1][0] if rows else 0
        problems.append(f"{AGENTS}:{at(ours, j)}: {shown(got)}; {AGENTS_TEMPLATE}:{at(theirs, i)} has {shown(want)}")
    return problems


def spec_block(spec):
    """(first line number, text) of the nix block in section 4."""
    section = re.search(r"^## 4\. Quick start.*?(?=^## |\Z)", spec, re.M | re.S)
    block = section and re.search(r"^```nix\n(.*?)^```", section.group(0), re.M | re.S)
    if not block:
        raise Unreadable(f"{SPEC}: no nix block in section 4")
    return spec.count("\n", 0, section.start() + block.start(1)) + 1, block.group(1)


def check_quickstart(root):
    start, block = spec_block(read(root, SPEC))
    flake = read(root, QUICKSTART)
    if block == flake:
        return []
    a, b = block.splitlines(), flake.splitlines()
    i = next((i for i, (x, y) in enumerate(zip(a, b)) if x != y), min(len(a), len(b)))
    show = lambda lines: repr(lines[i]) if i < len(lines) else "the end"
    return [f"{QUICKSTART}:{i + 1}: {show(b)} differs from {SPEC}:{start + i}: {show(a)}"]


CHECKS = {"workflow": check_workflow, "agents": check_agents, "quickstart": check_quickstart}


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    args = ap.parse_args(argv)
    status = 0
    for name, check in CHECKS.items():
        try:
            problems = check(args.root)
        except Unreadable as exc:
            print(f"error {name}: {exc}", file=sys.stderr)
            status = 2
            continue
        if problems:
            status = status or 1
            print(f"DRIFT {name}")
            for p in problems:
                print(f"  {p}")
        else:
            print(f"ok   {name}")
    return status


if __name__ == "__main__":
    sys.exit(main())
