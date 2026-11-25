#!/usr/bin/env python3
"""Normalize lint tool output into Nix-TOPS findings JSON."""

import json
import re
import shlex
import sys
from pathlib import Path


def fail(message, detail=""):
    print(f"nixtops lint error: {message}", file=sys.stderr)
    if detail.strip():
        print(detail.rstrip(), file=sys.stderr)
    sys.exit(1)


def strip_dot_slash(path):
    p = str(path)
    while p.startswith("./"):
        p = p.removeprefix("./")
    return p


def strip_prefix(path, prefix):
    p = str(path)
    pref = str(prefix).rstrip("/") + "/"
    if p.startswith(pref):
        return p[len(pref):]
    return strip_dot_slash(p)


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
        rel = strip_dot_slash(rel)
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
        fail("gitleaks wrote empty output")
    docs = json.loads(text)
    if not isinstance(docs, list):
        fail("gitleaks output is not a JSON array")
    for res in docs:
        rel = strip_prefix(res.get("File", ""), src_prefix)
        item = {
            "line": res.get("StartLine", 0),
            "rule": res.get("RuleID", ""),
            "message": res.get("Description", "secret detected"),
        }
        add_finding(files, rel, item)
    return files


def collect_findings(tool, rc, raw_text, err_text, src_prefix):
    if tool == "nixfmt":
        text = raw_text + ("\n" + err_text if err_text else "")
        if "Error: failed to " in text or "config error:" in text:
            fail("nixfmt failed", text)
        files = parse_nixfmt(text, src_prefix)
        if rc == 0:
            if "Error:" in text or files:
                fail("nixfmt exited 0 with errors or findings", text)
            return {}
        if "Error: unexpected changes detected" not in text or not files:
            fail(f"nixfmt failed (exit {rc})", text)
        return files

    if tool == "statix":
        if err_text.strip() or rc not in (0, 1):
            fail(f"statix failed (exit {rc})", err_text or raw_text)
        try:
            files = parse_statix(raw_text, src_prefix)
        except json.JSONDecodeError as exc:
            fail(f"statix output is not valid JSON: {exc}", raw_text)
        if rc == 1 and sum(len(v) for v in files.values()) == 0:
            fail("statix exited 1 with no parsed findings", raw_text)
        return files

    if tool == "deadnix":
        if rc != 0 or err_text.strip():
            fail(f"deadnix failed (exit {rc})", err_text or raw_text)
        try:
            return parse_deadnix(raw_text, src_prefix)
        except json.JSONDecodeError as exc:
            fail(f"deadnix output is not valid JSON: {exc}", raw_text)

    if tool == "gitleaks":
        if rc != 0:
            fail(f"gitleaks failed (exit {rc})", err_text or raw_text)
        try:
            return parse_gitleaks(raw_text, src_prefix)
        except json.JSONDecodeError as exc:
            fail(f"gitleaks output is not valid JSON: {exc}", raw_text)

    fail(f"unknown lint tool: {tool}")


def main():
    mode = sys.argv[1]
    if mode == "findings":
        tool, version, config_hash, src_prefix, rc_str, raw_path, err_path, out_path = sys.argv[2:10]
        rc = int(rc_str)
        err_text = (
            Path(err_path).read_text(encoding="utf-8", errors="replace")
            if Path(err_path).exists()
            else ""
        )
        if not Path(raw_path).is_file():
            fail(f"{tool} failed before writing output (exit {rc})", err_text)
        raw_text = Path(raw_path).read_text(encoding="utf-8", errors="replace")
        files = collect_findings(tool, rc, raw_text, err_text, src_prefix)
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
