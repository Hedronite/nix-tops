#!/usr/bin/env python3
"""Fail a change that weakens the gate unless it carries the protocol-change label.
Python 3.11 or later, standard library only.

    python3 scripts/law_diff.py --base DIR --head DIR --labels "a,b"

The workflow runs this script from the BASE commit, never from the change under review, and the
change's files are read as data: TOML is parsed and never run (NIX_TOPS.md section 11). It compares
the files of two checkouts:

nix-tops.toml   enforce lowered; a layer switched off, or a custom layer removed or moved to a
                later tier; eval options weakened; coverage floors lowered or removed; a budget
                mode lowered, a tolerance raised, an output dropped from the closure budget; a lint
                key removed or deadnix.fail turned off; laws.enable narrowed; ack_ledger or
                baseline moved; a facet or system dropped; a [[warnings.allow]] entry added, with
                a changed match, or with a later until; an [advisory] exemption added or with a
                later until
laws/*.toml     a law removed; severity lowered; judged turned off; reader changed; an artifact
                dropped
profiles/*.toml a profile removed; the same floor, layer, eval and budget rules as nix-tops.toml

Any file under a protected path (see PROTECTED) that is added, removed or different: a change
to the protocol, a schema, a law, a profile, a workflow or these two scripts is a protocol change
and needs the label even when no key above moved. This catches what the key rules cannot name,
such as a new diagnostics entry or a schema loosened. Links are never followed: a link at or
under a protected path counts as a change, because a link into the base checkout reads the same
as the file it replaces.

A weakening with no protocol-change label exits 1 and lists each one. With the label the same list
prints, marked allowed, and the exit is 0. Exit 2: a file of the change cannot be read, which no
label clears. A change with no weakening prints nothing and exits 0.
"""

import argparse
import os
import sys
import tomllib
from pathlib import Path

LABEL = "protocol-change"
MODE_RANK = {"advisory": 0, "ratchet": 1, "hard": 2}
SEVERITY_RANK = {"Ack": 0, "Refuse": 1}
# The value that keeps the gate strict; anything else is weaker.
EVAL_STRONG = {"all_systems": True, "abort_on_warn": True, "allow_ifd": False}


# Paths whose change is a protocol change by itself (the label rule of the review).
PROTECTED = (
    "NIX_TOPS.md",
    "schema",
    "laws",
    "profiles",
    ".github/workflows",
    "scripts/law_diff.py",
    "scripts/layers_ran.py",
)


class Unreadable(Exception):
    """A file of the change that is not valid TOML."""


def read_toml(path):
    """The parsed file, or None when it is absent."""
    if not path.is_file():
        return None
    try:
        with path.open("rb") as handle:
            return tomllib.load(handle)
    except (OSError, tomllib.TOMLDecodeError) as error:
        raise Unreadable(f"{path}: {error}") from error


def table(doc, *keys):
    """A nested table, or an empty one when a key on the way is not a table."""
    for key in keys:
        doc = doc.get(key) if isinstance(doc, dict) else None
    return doc if isinstance(doc, dict) else {}


def lowered(label, old, new, rank):
    """One finding when a ranked value went down (a missing value ranks as the lowest)."""
    if old in rank and rank.get(new, -1) < rank[old]:
        return [(label, old, new)]
    return []


def number_lowered(label, old, new):
    if isinstance(old, (int, float)) and (not isinstance(new, (int, float)) or new < old):
        return [(label, old, "removed" if new is None else new)]
    return []


def enforce_findings(base, head):
    old = base.get("enforce", "ratchet")
    new = head.get("enforce", "ratchet")
    return lowered("enforce", old, new, MODE_RANK)


def is_off(entry):
    return isinstance(entry, dict) and entry.get("enabled") is False


def layer_findings(base, head):
    found = []
    old, new = table(base, "layers"), table(head, "layers")
    for layer, entry in new.items():
        if layer != "custom" and is_off(entry) and not is_off(old.get(layer)):
            found.append((f"layers.{layer}", "on", "switched off"))
    old_custom = {c.get("id"): c for c in old.get("custom", []) if isinstance(c, dict)}
    new_custom = {c.get("id"): c for c in new.get("custom", []) if isinstance(c, dict)}
    for name, entry in old_custom.items():
        if name not in new_custom:
            found.append((f"layers.custom.{name}", "present", "removed"))
        elif new_custom[name].get("tier", 0) > entry.get("tier", 0):
            found.append((f"layers.custom.{name}.tier", entry.get("tier", 0), new_custom[name]["tier"]))
    return found


def eval_findings(base, head):
    found = []
    old, new = table(base, "eval"), table(head, "eval")
    for key, strong in EVAL_STRONG.items():
        before, after = old.get(key, strong), new.get(key, strong)
        if before == strong and after != strong:
            found.append((f"eval.{key}", before, after))
    return found


def coverage_findings(base, head):
    found = []
    old, new = table(base, "coverage"), table(head, "coverage")
    for key, value in old.items():
        found += number_lowered(f"coverage.{key}", value, new.get(key))
    return found


def budget_findings(base, head):
    found = []
    for name in ("closure", "eval_cost"):
        old, new = table(base, "budgets", name), table(head, "budgets", name)
        found += lowered(f"budgets.{name}.mode", old.get("mode"), new.get("mode"), MODE_RANK)
    old_tol = table(base, "budgets", "eval_cost").get("tolerance_pct")
    new_tol = table(head, "budgets", "eval_cost").get("tolerance_pct")
    if isinstance(old_tol, (int, float)) and isinstance(new_tol, (int, float)) and new_tol > old_tol:
        found.append(("budgets.eval_cost.tolerance_pct", old_tol, new_tol))
    old_out = set(table(base, "budgets", "closure").get("outputs", []))
    new_out = set(table(head, "budgets", "closure").get("outputs", []))
    found += [("budgets.closure.outputs", name, "removed") for name in sorted(old_out - new_out)]
    return found


def settings_findings(base, head):
    """The sections a config and a profile share."""
    return (
        layer_findings(base, head)
        + eval_findings(base, head)
        + coverage_findings(base, head)
        + budget_findings(base, head)
    )


def lint_findings(base, head):
    found = []
    old, new = table(base, "lint"), table(head, "lint")
    found += [(f"lint.{key}", "present", "removed") for key in old if key not in new]
    if table(old, "deadnix").get("fail") is True and table(new, "deadnix").get("fail") is False:
        found.append(("lint.deadnix.fail", True, False))
    return found


def law_set_findings(base, head):
    found = []
    old, new = table(base, "laws"), table(head, "laws")
    before, after = old.get("enable", "default"), new.get("enable", "default")
    if isinstance(after, list):
        dropped = sorted(set(before) - set(after)) if isinstance(before, list) else ["every default law not listed"]
        found += [("laws.enable", name, "dropped") for name in dropped]
    if old.get("ack_ledger") != new.get("ack_ledger") and "ack_ledger" in old:
        found.append(("laws.ack_ledger", old.get("ack_ledger"), new.get("ack_ledger", "removed")))
    return found


def scope_findings(base, head):
    found = []
    found += [("facets", name, "dropped") for name in base.get("facets", []) if name not in head.get("facets", [])]
    old, new = table(base, "toolchain").get("systems", []), table(head, "toolchain").get("systems", [])
    found += [("toolchain.systems", name, "dropped") for name in old if name not in new]
    if "baseline" in base and base["baseline"] != head.get("baseline"):
        found.append(("baseline", base["baseline"], head.get("baseline", "removed")))
    return found


def ledger_findings(base, head):
    """A new, changed or later-dated [[warnings.allow]] entry widens what the gate lets through."""
    old = table(base, "warnings").get("allow", [])
    new = table(head, "warnings").get("allow", [])
    found = []
    for entry in new:
        match, until = entry.get("match"), str(entry.get("until", ""))
        covered = any(o.get("match") == match and str(o.get("until", "")) >= until for o in old)
        if not covered:
            found.append(("warnings.allow", "no entry for this match through that date", f"{match} until {until}"))
    return found


def advisory_findings(base, head):
    """A new or later-dated [advisory] exemption switches enforcement off for longer."""
    old, new = table(base, "advisory"), table(head, "advisory")
    if not new:
        return []
    until = str(new.get("until", ""))
    if old and str(old.get("until", "")) >= until:
        return []
    before = f"until {old.get('until')}" if old else "no exemption"
    return [("advisory", before, f"until {until}")]


def config_findings(base, head):
    return (
        enforce_findings(base, head)
        + settings_findings(base, head)
        + lint_findings(base, head)
        + law_set_findings(base, head)
        + scope_findings(base, head)
        + ledger_findings(base, head)
        + advisory_findings(base, head)
    )


def law_findings(base, head):
    found = lowered("severity", base.get("severity"), head.get("severity"), SEVERITY_RANK)
    if base.get("judged") is True and head.get("judged") is not True:
        found.append(("judged", True, head.get("judged", "removed")))
    if "reader" in base and base["reader"] != head.get("reader"):
        found.append(("reader", base["reader"], head.get("reader", "removed")))
    found += [("artifacts", name, "dropped") for name in base.get("artifacts", []) if name not in head.get("artifacts", [])]
    return found


def compare_files(base_dir, head_dir, pattern, compare):
    """Compare each file the base has under pattern with the head's file of the same name."""
    found = []
    for path in sorted(base_dir.glob(pattern)):
        rel = path.relative_to(base_dir)
        old = read_toml(path) or {}
        new = read_toml(head_dir / rel)
        if new is None:
            found.append((str(rel), "file", "present", "removed"))
            continue
        found += [(str(rel), *item) for item in compare(old, new)]
    return found


def behind_a_link(root, rel):
    """True when a directory on the way to rel is a symbolic link."""
    return any((root / parent).is_symlink() for parent in list(rel.parents)[:-1])


def entries_under(root, entry):
    """What sits at a protected path, relative to the root, without following a link.

    A link on the way to the path, or at the path, is the entry itself. Below a directory, every file
    and every link (a link to a directory is not entered) is an entry.
    """
    parts = Path(entry).parts
    for count in range(1, len(parts) + 1):
        if (root / Path(*parts[:count])).is_symlink():
            return {Path(*parts[:count])}
    path = root / entry
    if path.is_file():
        return {Path(entry)}
    found = set()
    if path.is_dir():
        for dirpath, dirnames, filenames in os.walk(path, followlinks=False):
            for name in filenames + dirnames:
                item = Path(dirpath) / name
                if name in filenames or item.is_symlink():
                    found.add(item.relative_to(root))
    return found


def snapshot(root, rel):
    """A link by its target, a file by its bytes, anything else (or behind a link) as nothing."""
    path = root / rel
    if path.is_symlink():
        return ("link", os.readlink(path))
    if not path.is_file():
        return None
    return ("file", path.read_bytes())


def path_state(old, new):
    """How a protected path differs, or None. Any link in the change is a change: a link can point
    at the base checkout, so it reads the same as the file it replaces and says nothing."""
    if new is not None and new[0] == "link":
        return "symlink"
    if old == new:
        return None
    if old is None:
        return "added"
    return "removed" if new is None else "changed"


def path_findings(base_dir, head_dir):
    found = []
    for entry in PROTECTED:
        for rel in sorted(entries_under(base_dir, entry) | entries_under(head_dir, entry)):
            if behind_a_link(base_dir, rel) or behind_a_link(head_dir, rel):
                continue  # the link above it is the finding
            state = path_state(snapshot(base_dir, rel), snapshot(head_dir, rel))
            if state:
                found.append((str(rel), "protected path", "unchanged", state))
    return found


def weakenings(base_dir, head_dir):
    """Every weakening between two checkouts, as (file, key, old, new). Raises Unreadable."""
    base_dir, head_dir = Path(base_dir), Path(head_dir)
    found = compare_files(base_dir, head_dir, "nix-tops.toml", config_findings)
    found += compare_files(base_dir, head_dir, "laws/*.toml", law_findings)
    found += compare_files(base_dir, head_dir, "profiles/*.toml", settings_findings)
    found += path_findings(base_dir, head_dir)
    # A head file with no base twin is not compared, but it must still parse.
    for path in sorted(head_dir.glob("nix-tops.toml")) + sorted(head_dir.glob("laws/*.toml")) + sorted(
        head_dir.glob("profiles/*.toml")
    ):
        read_toml(path)
    return found


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--base", required=True, type=Path)
    parser.add_argument("--head", required=True, type=Path)
    parser.add_argument("--labels", default="")
    args = parser.parse_args(argv)
    labels = {name.strip() for name in args.labels.split(",") if name.strip()}
    try:
        found = weakenings(args.base, args.head)
    except Unreadable as error:
        print(f"law-diff: cannot read a file of the change: {error}")
        return 2
    allowed = LABEL in labels
    for file, key, old, new in found:
        print(f"law-diff: weakening: {file}: {key}: {old} -> {new}")
    if not found:
        return 0
    if allowed:
        print(f"law-diff: {len(found)} weakening(s) allowed by {LABEL}")
        return 0
    print(f"law-diff: {len(found)} weakening(s) without the {LABEL} label")
    return 1


if __name__ == "__main__":
    sys.exit(main())
