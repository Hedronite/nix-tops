#!/usr/bin/env python3
"""Normalize lint tool output into Nix-TOPS findings JSON."""

import json
import re
import shlex
import sys
from pathlib import Path


def strip_prefix(path, prefix):
    p = str(path)
    pref = str(prefix).rstrip("/") + "/"
    if p.startswith(pref):
        return p[len(pref):]
    return p.lstrip("./")


def add_finding(files, rel, item):
    files.setdefault(rel, []).append(item)


def parse_nixfmt(raw_text, _src_prefix):
    files = {}
    pat = re.compile(r'file has changed\s+path=("[^"]+"|\S+)')
    for line in raw_text.splitlines():
        m = pat.search(line)
        if not m:
            continue
        raw_path = m.group(1)
        rel = shlex.split(raw_path)[0] if raw_path.startswith('"') else raw_path
        rel = rel.lstrip("./")
        add_finding(files, rel, {"message": "file is not formatted with nixfmt"})
    return files


def iter_json_docs(text):
    dec = json.JSONDecoder()
    idx = 0
    n = len(text)
    while idx < n:
        while idx < n and text[idx].isspace():
            idx += 1
        if idx >= n:
            break
        obj, end = dec.raw_decode(text, idx)
        yield obj
        idx = end


def parse_statix(raw_text, src_prefix):
    files = {}
    for doc in iter_json_docs(raw_text):
        rel = strip_prefix(doc.get("file", ""), src_prefix)
        for rep in doc.get("report", []) or []:
            code = rep.get("code")
            note = rep.get("note", "")
            for diag in rep.get("diagnostics", []) or []:
                pos = (diag.get("at") or {}).get("from") or {}
                item = {
                    "line": pos.get("line", 0),
                    "column": pos.get("column", 0),
                    "code": code,
                    "message": diag.get("message") or note,
                }
                add_finding(files, rel, item)
    return files


def parse_deadnix(raw_text, src_prefix):
    files = {}
    for doc in iter_json_docs(raw_text):
        rel = strip_prefix(doc.get("file", ""), src_prefix)
        for res in doc.get("results", []) or []:
            item = {
                "line": res.get("line", 0),
                "column": res.get("column", 0),
                "message": res.get("message", "unused binding"),
            }
            add_finding(files, rel, item)
    return files


def parse_gitleaks(raw_text, src_prefix):
    files = {}
    text = raw_text.strip()
    if not text:
        return files
    docs = json.loads(text) or []
    for res in docs:
        rel = strip_prefix(res.get("File", ""), src_prefix)
        item = {
            "line": res.get("StartLine", 0),
            "rule": res.get("RuleID", ""),
            "message": res.get("Description", "secret detected"),
        }
        add_finding(files, rel, item)
    return files


PARSERS = {
    "nixfmt": parse_nixfmt,
    "statix": parse_statix,
    "deadnix": parse_deadnix,
    "gitleaks": parse_gitleaks,
}


def main():
    mode = sys.argv[1]
    if mode == "findings":
        tool, version, config_hash, src_prefix, raw_path, out_path = sys.argv[2:8]
        raw_text = (
            Path(raw_path).read_text(encoding="utf-8", errors="replace")
            if Path(raw_path).exists()
            else ""
        )
        files = PARSERS[tool](raw_text, src_prefix)
        sorted_files = {k: files[k] for k in sorted(files)}
        out = {
            "tool": tool,
            "version": version,
            "config_hash": config_hash,
            "files": sorted_files,
        }
        Path(out_path).write_text(json.dumps(out, indent=2) + "\n", encoding="utf-8")
    elif mode == "verdict":
        findings_path, out_path, fail_on_finding = sys.argv[2:5]
        data = json.loads(Path(findings_path).read_text(encoding="utf-8"))
        files = data.get("files", {})
        total = sum(len(v) for v in files.values())
        tool = data.get("tool", "lint")
        if total > 0 and fail_on_finding == "true":
            for path, items in files.items():
                for item in items:
                    line = item.get("line")
                    loc = f"{path}:{line}" if line else path
                    print(f"{tool}: {loc}: {item.get('message', 'finding')}", file=sys.stderr)
            print(f"{tool}: {total} finding(s) in {len(files)} file(s)", file=sys.stderr)
            sys.exit(1)
        Path(out_path).write_text(f"{tool}: {total} finding(s)\n", encoding="utf-8")
    else:
        sys.exit(2)


if __name__ == "__main__":
    main()
