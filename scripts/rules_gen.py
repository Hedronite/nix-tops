#!/usr/bin/env python3
"""Generate the copies of the stated rules (NIX_TOPS.md section 3.4). Python 3 standard library only.

    python3 scripts/rules_gen.py render|write|check [--root DIR] [--allow-unmarked]

The source is the numbered list under the "### 3.4" heading of NIX_TOPS.md. Each copy sits
between a marker pair and is replaced whole:

    <!-- rules:begin -->
    ...generated, do not edit...
    <!-- rules:end -->

render   print every copy
write    rewrite the text between the markers in every target
check    fail (exit 1, with a diff) when a target differs from what write would produce

A target with no marker pair is an error unless --allow-unmarked is given.
"""

import argparse
import difflib
import re
import sys
from pathlib import Path

SPEC = "NIX_TOPS.md"
BEGIN = "<!-- rules:begin -->"
END = "<!-- rules:end -->"
NOTE = "<!-- generated from NIX_TOPS.md section 3.4 by scripts/rules_gen.py; edit the source, not this copy -->"

# (file, style). "numbered" keeps the rule numbers; "bullets" is a plain list.
TARGETS = [
    ("NIX_TOPS.md", "numbered"),
    ("config/AGENTS.template.md", "numbered"),
    ("AGENTS.md", "bullets"),
]

HEADING = re.compile(r"^(#{1,6})\s+(.*?)\s*$")
ITEM = re.compile(r"^(\d+)\.\s+(\S.*)$")


class GenError(Exception):
    pass


def parse_rules(spec_text):
    """Return [(number, text)] from the numbered list in section 3.4."""
    lines = spec_text.splitlines()
    start = next((i for i, l in enumerate(lines) if re.match(r"^###\s+3\.4\b", l)), None)
    if start is None:
        raise GenError(f"{SPEC}: no section 3.4 heading")
    rules, fence = [], False
    for line in lines[start + 1:]:
        if line.lstrip().startswith("```"):
            fence = not fence
        if not fence and (HEADING.match(line) or line.strip() == "---"):
            break
        m = ITEM.match(line)
        if m:
            rules.append([int(m.group(1)), m.group(2).strip()])
        elif rules and line.startswith((" ", "\t")) and line.strip():
            rules[-1][1] += " " + line.strip()
        elif line.strip() and rules:
            raise GenError(f"{SPEC}: section 3.4 has text that is not a rule: {line.strip()[:60]!r}")
    if not rules:
        raise GenError(f"{SPEC}: section 3.4 has no numbered rules")
    nums = [n for n, _ in rules]
    if nums != list(range(1, len(rules) + 1)):
        raise GenError(f"{SPEC}: section 3.4 rules are not numbered 1..{len(rules)}: {nums}")
    return [(n, t) for n, t in rules]


def render(rules, style):
    if style == "numbered":
        body = [f"{n}. {t}" for n, t in rules]
    elif style == "bullets":
        body = [f"- {t}" for _, t in rules]
    else:
        raise GenError(f"unknown style {style}")
    return "\n".join([NOTE, "", *body])


def splice(text, block, name):
    """Replace the text between the one marker pair in `text`. Returns None when unmarked."""
    nb, ne = text.count(BEGIN), text.count(END)
    if nb == 0 and ne == 0:
        return None
    if nb != 1 or ne != 1:
        raise GenError(f"{name}: expected one {BEGIN} and one {END}, found {nb} and {ne}")
    a, b = text.index(BEGIN), text.index(END)
    if b < a:
        raise GenError(f"{name}: {END} comes before {BEGIN}")
    return text[: a + len(BEGIN)] + "\n" + block + "\n" + text[b:]


def run(root, mode, allow_unmarked=False):
    """Return (exit status, output text)."""
    root = Path(root)
    rules = parse_rules((root / SPEC).read_text(encoding="utf-8"))
    out, status = [], 0
    for rel, style in TARGETS:
        path = root / rel
        block = render(rules, style)
        if mode == "render":
            out.append(f"== {rel} ({style})\n{block}")
            continue
        if not path.is_file():
            raise GenError(f"{rel}: not found")
        old = path.read_text(encoding="utf-8")
        new = splice(old, block, rel)
        if new is None:
            if allow_unmarked:
                out.append(f"skip {rel}: no markers")
                continue
            raise GenError(f"{rel}: no {BEGIN} / {END} pair")
        if mode == "write":
            if new != old:
                path.write_text(new, encoding="utf-8")
                out.append(f"wrote {rel}")
            else:
                out.append(f"ok   {rel}")
        elif new != old:
            status = 1
            diff = difflib.unified_diff(old.splitlines(), new.splitlines(), f"{rel} (on disk)", f"{rel} (generated)", lineterm="")
            out.append(f"DRIFT {rel}\n" + "\n".join(diff))
        else:
            out.append(f"ok   {rel}")
    return status, "\n".join(out)


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("command", choices=["render", "write", "check"])
    ap.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    ap.add_argument("--allow-unmarked", action="store_true")
    args = ap.parse_args(argv)
    try:
        status, text = run(args.root, args.command, args.allow_unmarked)
    except GenError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2
    print(text)
    return status


if __name__ == "__main__":
    sys.exit(main())
