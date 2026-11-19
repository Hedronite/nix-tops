#!/usr/bin/env python3
"""Fail when fewer layers ran than nix-tops.toml enables. Python 3.11 or later, standard library only.

    python3 scripts/layers_ran.py [--config nix-tops.toml] [--report .nix-tops/report.json]

The runner already fails a layer that is enabled and not run, but a report with a layer missing
must not pass either, so the job reads both files after the gate (NIX_TOPS.md section 11):

1. every layer id of the report schema is in the report, once;
2. a layer that is not switched off in nix-tops.toml has run: its verdict is not skip, not-run,
   not-evaluated or deferred;
3. a layer that is switched off shows skip with the reason the config gives.

Exit status is 0 when all of that holds, 1 when it does not, 3 when a file cannot be read.
"""

import argparse
import json
import sys
import tomllib
from pathlib import Path

LAYERS = ("source", "lint", "eval", "build-test", "coverage", "facet-layers", "laws")
SWITCHABLE = ("lint", "build-test", "coverage", "facet-layers", "laws")
NOT_RAN = ("skip", "not-run", "not-evaluated", "deferred")


def switched_off(config):
    """The layers nix-tops.toml switches off, each with its reason."""
    layers = config.get("layers", {})
    off = {}
    for layer in SWITCHABLE:
        entry = layers.get(layer)
        if isinstance(entry, dict) and entry.get("enabled") is False:
            off[layer] = entry.get("reason", "")
    return off


def problems(config, report):
    """Every way the report disagrees with the config, one line each."""
    found = []
    rows = {}
    for row in report.get("layers", []):
        layer = row.get("id")
        if layer not in LAYERS:
            found.append(f"layer {layer} is not a layer of the report schema")
        elif layer in rows:
            found.append(f"layer {layer} is in the report twice")
        else:
            rows[layer] = row
    off = switched_off(config)
    for layer in LAYERS:
        row = rows.get(layer)
        if row is None:
            found.append(f"layer {layer} is missing from the report")
            continue
        verdict = row.get("verdict")
        if layer in off:
            if verdict != "skip":
                found.append(f"layer {layer} is switched off but its verdict is {verdict}")
            elif row.get("reason") != off[layer]:
                found.append(f"layer {layer} is switched off with a reason the report does not show")
        elif verdict in NOT_RAN:
            found.append(f"layer {layer} is enabled but did not run: {verdict}")
    return found


def load(path, parse):
    try:
        with open(path, "rb") as handle:
            return parse(handle)
    except (OSError, ValueError) as error:
        print(f"layers_ran: cannot read {path}: {error}", file=sys.stderr)
        sys.exit(3)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--config", default="nix-tops.toml", type=Path)
    parser.add_argument("--report", default=".nix-tops/report.json", type=Path)
    args = parser.parse_args(argv)
    config = load(args.config, tomllib.load)
    report = load(args.report, json.load)
    found = problems(config, report)
    for line in found:
        print(f"layers_ran: {line}")
    if found:
        return 1
    print(f"layers_ran: all {len(LAYERS)} layers accounted for ({len(switched_off(config))} switched off)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
