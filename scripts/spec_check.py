#!/usr/bin/env python3
"""Checks for the Nix-TOPS spec repo. Python 3 standard library only.

    python3 scripts/spec_check.py links|catalog|schema|all [--root DIR]

links    intra-repo Markdown links and heading anchors resolve
catalog  law rows, judged marks and the counts quoted in the text agree
schema   the section 9 TOML example validates against schema/nix-tops.schema.json
         (needs check-jsonschema, pinned below)

Exit status is 0 when every selected check passes, 1 otherwise.
"""

import argparse
import re
import shutil
import subprocess
import sys
import tempfile
import unicodedata
from pathlib import Path

CHECK_JSONSCHEMA_VERSION = "0.38.0"
SPEC = "NIX_TOPS.md"
SCHEMA = "schema/nix-tops.schema.json"
SKIP_DIRS = {".git", "node_modules", "target", "result"}

FENCE = re.compile(r"^\s*(```|~~~)")
HEADING = re.compile(r"^(#{1,6})\s+(.*?)\s*#*\s*$")
INLINE_CODE = re.compile(r"`+[^`]*`+")
LINK = re.compile(r"(?<!!)\[(?:[^\]\\]|\\.)*\]\(\s*<?([^)\s>]+)>?(?:\s+\"[^\"]*\")?\s*\)")
LAW_ROW = re.compile(r"^\|\s*(N-\d{2,})\s*\|")


# ---------------------------------------------------------------- markdown


def md_files(root):
    out = []
    for p in sorted(root.rglob("*.md")):
        if any(part in SKIP_DIRS for part in p.relative_to(root).parts):
            continue
        out.append(p)
    return out


def prose_lines(text):
    """Yield (lineno, line) for lines outside fenced code blocks."""
    fence = None
    for n, line in enumerate(text.splitlines(), 1):
        m = FENCE.match(line)
        if m:
            marker = m.group(1)
            if fence is None:
                fence = marker
            elif fence == marker:
                fence = None
            continue
        if fence is None:
            yield n, line


def slugify(heading):
    """GitHub's heading anchor: lowercase, drop punctuation, spaces to hyphens."""
    text = re.sub(r"!?\[([^\]]*)\]\([^)]*\)", r"\1", heading)
    text = text.replace("`", "")
    text = re.sub(r"[*_~]{1,3}(?=\S)|(?<=\S)[*~]{1,3}", "", text)
    text = unicodedata.normalize("NFKC", text).lower().strip()
    text = "".join(c for c in text if c.isalnum() or c in " -_")
    return text.replace(" ", "-")


def anchors_of(path, cache):
    if path in cache:
        return cache[path]
    seen, out = {}, set()
    for _, line in prose_lines(path.read_text(encoding="utf-8")):
        m = HEADING.match(line)
        if not m:
            continue
        slug = slugify(m.group(2))
        k = seen.get(slug, 0)
        seen[slug] = k + 1
        out.add(slug if k == 0 else f"{slug}-{k}")
    cache[path] = out
    return out


def check_links(root):
    errors, cache = [], {}
    for md in md_files(root):
        rel = md.relative_to(root)
        for n, line in prose_lines(md.read_text(encoding="utf-8")):
            for target in LINK.findall(INLINE_CODE.sub("", line)):
                if re.match(r"^[a-zA-Z][a-zA-Z0-9+.-]*:", target):
                    continue  # http:, https:, mailto: ...
                path_part, _, frag = target.partition("#")
                path_part = path_part.split("?", 1)[0]
                dest = md if path_part == "" else (md.parent / path_part)
                try:
                    dest = dest.resolve()
                    dest.relative_to(root.resolve())
                except (ValueError, OSError):
                    errors.append(f"{rel}:{n}: link leaves the repo: {target}")
                    continue
                if not dest.exists():
                    errors.append(f"{rel}:{n}: missing file: {target}")
                    continue
                if frag and dest.suffix == ".md" and dest.is_file():
                    if frag.lower() not in anchors_of(dest, cache):
                        errors.append(f"{rel}:{n}: no heading anchor: {target}")
    return errors


# ----------------------------------------------------------------- catalog


def section(lines, start_pat, level):
    """Lines from the heading matching start_pat up to the next heading of the same or higher level."""
    begin = None
    for i, line in enumerate(lines):
        m = HEADING.match(line)
        if begin is None:
            if m and re.search(start_pat, line):
                begin = i
        elif m and len(m.group(1)) <= level:
            return begin, lines[begin:i]
    return (begin, lines[begin:]) if begin is not None else (None, [])


def check_catalog(root):
    errors = []
    spec = root / SPEC
    if not spec.is_file():
        return [f"{SPEC}: not found"]
    lines = spec.read_text(encoding="utf-8").splitlines()

    start, sec = section(lines, r"^###\s+8\.2\b", 3)
    if start is None:
        return [f"{SPEC}: no section 8.2 heading"]
    heading = sec[0]

    ids, judged = [], 0
    for line in sec:
        m = LAW_ROW.match(line)
        if not m:
            continue
        ids.append(m.group(1))
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if cells[-1] == "✓":
            judged += 1
    total = len(ids)
    if total == 0:
        return [f"{SPEC}: section 8.2 has no law rows"]

    dupes = sorted({i for i in ids if ids.count(i) > 1})
    if dupes:
        errors.append(f"{SPEC}: duplicate law rows: {', '.join(dupes)}")
    want = [f"N-{k:02d}" for k in range(1, total + 1)]
    if sorted(set(ids)) != want:
        errors.append(f"{SPEC}: law IDs are not N-01..N-{total:02d} without gaps")

    # heading: "(draft: 43 laws, N-01…N-43)"
    m = re.search(r"(\d+)\s+laws,\s*N-01\W+N-(\d+)", heading)
    if not m:
        errors.append(f"{SPEC}:{start + 1}: cannot read the law count in the 8.2 heading")
    else:
        if int(m.group(1)) != total:
            errors.append(f"{SPEC}:{start + 1}: heading says {m.group(1)} laws, table has {total} rows")
        if int(m.group(2)) != total:
            errors.append(f"{SPEC}:{start + 1}: heading ends at N-{m.group(2)}, table has {total} rows")

    # target line under the table: "**v0.1 target:** 29 of 43 laws judged."
    found = False
    for i, line in enumerate(sec):
        m = re.search(r"\*\*v0\.1 target:\*\*\s*(\d+)\s+of\s+(\d+)\s+laws judged", line)
        if m:
            found = True
            at = f"{SPEC}:{start + i + 1}"
            if int(m.group(1)) != judged:
                errors.append(f"{at}: says {m.group(1)} judged, table marks {judged} with ✓")
            if int(m.group(2)) != total:
                errors.append(f"{at}: says {m.group(2)} laws, table has {total} rows")
    if not found:
        errors.append(f"{SPEC}: no '**v0.1 target:** N of M laws judged' line in section 8.2")

    # section 19, M3 row: "29 judged" and "29 of 43 judged"
    m3 = [(i, l) for i, l in enumerate(lines) if l.startswith("| **M3**")]
    if len(m3) != 1:
        errors.append(f"{SPEC}: expected one M3 row in the roadmap, found {len(m3)}")
    else:
        i, row = m3[0]
        at = f"{SPEC}:{i + 1}"
        a = re.search(r"(\d+)\s+judged", row)
        b = re.search(r"(\d+)\s+of\s+(\d+)\s+judged", row)
        if not (a and b):
            errors.append(f"{at}: cannot read the judged counts in the M3 row")
        else:
            for got, what in ((int(a.group(1)), "judged"), (int(b.group(1)), "of-judged")):
                if got != judged:
                    errors.append(f"{at}: M3 row {what} count is {got}, table marks {judged} with ✓")
            if int(b.group(2)) != total:
                errors.append(f"{at}: M3 row says {b.group(2)} laws, table has {total} rows")

    # laws/: one file per row and no extra
    laws = root / "laws"
    if laws.is_dir():
        files = {p.stem for p in laws.glob("N-*.toml")}
        for missing in sorted(set(ids) - files):
            errors.append(f"laws/: no file for {missing}")
        for extra in sorted(files - set(ids)):
            errors.append(f"laws/{extra}.toml: no row in section 8.2")
    return errors


# ------------------------------------------------------------------ schema


def section9_toml(root):
    lines = (root / SPEC).read_text(encoding="utf-8").splitlines()
    _, sec = section(lines, r"^##\s+9\.", 2)
    if not sec:
        return None, f"{SPEC}: no section 9"
    block, inside = [], False
    for line in sec:
        if not inside and re.match(r"^```toml\s*$", line):
            inside = True
        elif inside and line.strip() == "```":
            return "\n".join(block) + "\n", None
        elif inside:
            block.append(line)
    return None, f"{SPEC}: section 9 has no complete ```toml block"


def check_jsonschema_cmd():
    exe = shutil.which("check-jsonschema")
    if exe:
        return [exe]
    probe = subprocess.run([sys.executable, "-c", "import check_jsonschema"], capture_output=True)
    return [sys.executable, "-m", "check_jsonschema"] if probe.returncode == 0 else None


def check_schema(root):
    cmd = check_jsonschema_cmd()
    if cmd is None:
        return [f"check-jsonschema {CHECK_JSONSCHEMA_VERSION} is not installed (pip install check-jsonschema=={CHECK_JSONSCHEMA_VERSION})"]
    ver = subprocess.run(cmd + ["--version"], capture_output=True, text=True)
    got = ver.stdout.strip()
    print(f"schema: {got}")
    if not got.endswith(CHECK_JSONSCHEMA_VERSION):
        return [f"check-jsonschema must be {CHECK_JSONSCHEMA_VERSION}, found: {got or ver.stderr.strip()}"]

    schema = root / SCHEMA
    if not schema.is_file():
        return [f"{SCHEMA}: not found"]
    text, err = section9_toml(root)
    if err:
        return [err]
    try:
        import tomllib
        tomllib.loads(text)
    except ModuleNotFoundError:
        pass  # Python before 3.11: check-jsonschema parses it
    except ValueError as exc:
        return [f"{SPEC}: the section 9 example is not valid TOML: {exc}"]

    errors = []
    meta = subprocess.run(cmd + ["--check-metaschema", str(schema)], capture_output=True, text=True)
    if meta.returncode != 0:
        errors.append(f"{SCHEMA}: not a valid 2020-12 schema:\n{(meta.stdout + meta.stderr).strip()}")
    with tempfile.TemporaryDirectory() as tmp:
        inst = Path(tmp) / "section9.toml"
        inst.write_text(text, encoding="utf-8")
        run = subprocess.run(cmd + ["--schemafile", str(schema), str(inst)], capture_output=True, text=True)
    if run.returncode != 0:
        errors.append(f"the section 9 example does not validate:\n{(run.stdout + run.stderr).strip()}")
    return errors


# -------------------------------------------------------------------- main

CHECKS = {"links": check_links, "catalog": check_catalog, "schema": check_schema}


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("command", choices=[*CHECKS, "all"])
    ap.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    args = ap.parse_args(argv)
    root = args.root.resolve()
    names = list(CHECKS) if args.command == "all" else [args.command]
    failed = 0
    for name in names:
        errors = CHECKS[name](root)
        if errors:
            failed += 1
            print(f"FAIL {name}")
            for e in errors:
                print(f"  {e}")
        else:
            print(f"ok   {name}")
    if failed:
        print(f"{failed} of {len(names)} checks failed")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
