#!/usr/bin/env python3
"""Time `nix run .#nixtops -- version` on an empty store and say what it fetched and what it built.

Usage: measure.py <outdir>. Exits non-zero when the adopter lock does not pin the kit's inputs as
the kit's own lock does, when the store already holds the runner or a Rust toolchain, when the
command fails, or when the lock changes. The time itself never fails the run.
"""

import json
import os
import platform
import re
import subprocess
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
RUN = ["nix", "run", ".#nixtops", "--no-update-lock-file", "--no-accept-flake-config", "-L", "--", "version"]
# Names that mean the run would not be cold.
WARM = re.compile(r"^(nixtops|rustc|cargo)-[0-9]")
TARGET_SECONDS = 60


def nix(*args):
    return subprocess.run(["nix", *args], check=True, capture_output=True, text=True).stdout


def store():
    return json.loads(nix("path-info", "--all", "--json", "--json-format", "1"))


def name_of(path):
    return path.rsplit("/", 1)[-1].split("-", 1)[1]


def kind(path, info):
    """derivation, built (here), downloaded (a fixed-output build), fetched (substituted), source."""
    if path.endswith(".drv"):
        return "derivation"
    if info.get("ultimate") and info.get("deriver"):
        return "downloaded" if info.get("ca") else "built"
    if info.get("signatures"):
        return "fetched"
    return "source"


def lock_drift(kit, adopter):
    """The kit's inputs that the adopter's lock pins differently.

    Nix evaluates a relative path input with the nodes the adopter's lock already holds, so without
    this check a moved kit pin leaves the old runner being timed.
    """
    nodes = adopter["nodes"]
    mirrored = nodes[nodes[adopter["root"]]["inputs"]["nix-tops"]].get("inputs", {})
    drift = []
    for name, node in kit["nodes"][kit["root"]]["inputs"].items():
        want = kit["nodes"][node]["locked"]
        got = nodes[mirrored[name]]["locked"] if isinstance(mirrored.get(name), str) else {}
        for key in ("rev", "narHash"):
            if got.get(key) != want.get(key):
                drift.append(f"{name} {key}: the kit has {want.get(key)}, the adopter lock has {got.get(key)}")
    return drift


def timed(command, env, log):
    """Run the command; write each stderr line with the seconds since the start."""
    start = time.monotonic()
    with tempfile.TemporaryFile("w+") as out:
        proc = subprocess.Popen(command, cwd=HERE, env=env, stdout=out, stderr=subprocess.PIPE, text=True, errors="replace")
        with open(log, "w", encoding="utf-8") as fh:
            for line in proc.stderr:
                fh.write(f"{time.monotonic() - start:8.1f} {line}")
        code = proc.wait()
        seconds = time.monotonic() - start
        out.seek(0)
        return code, seconds, out.read()


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    outdir = Path(sys.argv[1]).resolve()
    outdir.mkdir(parents=True, exist_ok=True)

    drift = lock_drift(
        json.loads((HERE / ".." / ".." / "flake.lock").read_text(encoding="utf-8")),
        json.loads((HERE / "flake.lock").read_text(encoding="utf-8")),
    )
    if drift:
        sys.exit("fixtures/adopter/flake.lock is behind the kit's flake.lock; run `nix flake lock` here:\n" + "\n".join(drift))

    before = store()
    (outdir / "store-before.txt").write_text("".join(f"{p}\n" for p in sorted(before)), encoding="utf-8")
    warm = sorted(p for p in before if WARM.match(name_of(p)))
    if warm:
        sys.exit("the store is not empty of the runner and the toolchain:\n" + "\n".join(warm))

    lock = (HERE / "flake.lock").read_bytes()
    env = dict(os.environ, XDG_CACHE_HOME=tempfile.mkdtemp(prefix="cold-cache."))
    code, seconds, stdout = timed(RUN, env, outdir / "run.log")
    (outdir / "version.txt").write_text(stdout, encoding="utf-8")
    if code != 0:
        sys.stderr.write((outdir / "run.log").read_text(encoding="utf-8")[-6000:])
        sys.exit(f"the command exited {code}")
    if (HERE / "flake.lock").read_bytes() != lock:
        sys.exit("the command changed flake.lock")

    after = store()
    new = {p: i for p, i in after.items() if p not in before}
    groups = {k: [] for k in ("fetched", "downloaded", "built", "source", "derivation")}
    for path, info in sorted(new.items(), key=lambda kv: name_of(kv[0])):
        groups[kind(path, info)].append({"path": path, "narSize": info.get("narSize", 0)})
    for group, rows in groups.items():
        (outdir / f"{group}.txt").write_text("".join(f"{r['path']} {r['narSize']}\n" for r in rows), encoding="utf-8")

    result = {
        "command": " ".join(RUN),
        "seconds": round(seconds, 1),
        "over_target": seconds > TARGET_SECONDS,
        "nix": nix("--version").strip(),
        "machine": f"{platform.machine()}-{platform.system().lower()}",
        "cpus": os.cpu_count(),
        "config": {k: nix("config", "show", k).strip() for k in ("max-jobs", "cores", "sandbox", "substituters")},
        "store_paths_before": len(before),
        "new": {g: {"paths": len(r), "bytes": sum(x["narSize"] for x in r)} for g, r in groups.items()},
        "built": [name_of(r["path"]) for r in groups["built"]],
    }
    (outdir / "result.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")

    lines = [
        f"cold start: {result['seconds']} s ({'over' if result['over_target'] else 'within'} {TARGET_SECONDS} s)",
        f"{result['nix']}, {result['machine']}, {result['cpus']} cpus, " + ", ".join(f"{k}={v}" for k, v in result["config"].items()),
        f"store paths before: {len(before)}",
        "",
        "| New store paths | Count | MiB unpacked |",
        "|---|---|---|",
    ]
    for group in groups:
        n = result["new"][group]
        lines.append(f"| {group} | {n['paths']} | {n['bytes'] / 2**20:.1f} |")
    lines += ["", "built here: " + (", ".join(result["built"]) or "nothing")]
    (outdir / "summary.md").write_text("\n".join(lines) + "\n", encoding="utf-8")
    print("\n".join(lines))
    print(stdout)


if __name__ == "__main__":
    main()
