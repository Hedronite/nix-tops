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
the baseline    (the path the base's nix-tops.toml names) removed or unreadable; a lint tool
                dropped; a file's count risen, a new file included; acks risen; a judged law lost;
                a tool configuration narrower, or changed in a way its inputs do not explain.
                A first baseline and falling counts are not weakenings

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
import hashlib
import json
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


BASELINE = ".nix-tops/baseline.json"
# The lists of a lint config that switch checks off, by tool, as dotted keys.
ALLOW_KEYS = {
    "statix": ("disabled", "ignore"),
    "nixfmt": ("excludes", "global.excludes", "formatter.*.excludes"),
    "gitleaks": ("allowlist.paths", "allowlist.regexes", "allowlist.commits", "allowlist.stopwords"),
}
CONFIG_DEFAULTS = {
    "nixfmt": {"formatter": {"nix": {"command": "nixfmt", "includes": ["*.nix"]}}},
    "gitleaks": {"extend": {"useDefault": True}},
}


def preimage(tool, inputs):
    """The text lib.mkChecks hashes for a tool with an empty exclude."""
    config = inputs.get("config")
    if tool == "nixfmt":
        body = "default-nixfmt" if config is None else config
    elif tool == "statix":
        body = "default" if config is None else config
    elif tool == "deadnix":
        body = f"fail={inputs.get('fail')}"
    elif tool == "gitleaks":
        body = ("default" if config is None else config) + "|" + (inputs.get("ignore") or "")
    else:
        return None
    return body + "\nexclude:"


def explains(tool, entry):
    """The kept inputs, only when they give the entry's config_hash."""
    inputs = entry.get("inputs")
    if not isinstance(inputs, dict):
        return None
    text = preimage(tool, inputs)
    if text is None or hashlib.sha256(text.encode()).hexdigest() != entry.get("config_hash"):
        return None
    return inputs


def take(node, parts, label, out):
    """Remove the string list at a dotted key, recording its entries."""
    head, rest = parts[0], parts[1:]
    if not isinstance(node, dict):
        return
    if head == "*":
        for name, child in node.items():
            take(child, rest, f"{label}:{name}", out)
    elif rest:
        take(node.get(head), rest, label, out)
    elif isinstance(node.get(head), list) and all(isinstance(item, str) for item in node[head]):
        out.update(f"{label}:{item}" for item in node.pop(head))


def shape(tool, inputs):
    """(allow entries, everything else) of a tool's config, or None when it is not TOML."""
    text = inputs.get("config")
    try:
        doc = tomllib.loads(text) if text is not None else json.loads(json.dumps(CONFIG_DEFAULTS.get(tool, {})))
    except tomllib.TOMLDecodeError:
        return None
    allowed = set()
    for key in ALLOW_KEYS.get(tool, ()):
        label = "statix:" + key if tool == "statix" else key.replace(".*", "")
        take(doc, key.split("."), label, allowed)
    allowed.discard("statix:ignore:.direnv")
    if tool == "nixfmt" and doc.get("global") == {}:
        del doc["global"]
    if tool == "gitleaks":
        if isinstance(doc.get("allowlist"), dict):
            doc["allowlist"].pop("description", None)
        if doc.get("allowlist") == {}:
            del doc["allowlist"]
        lists = doc.pop("allowlists", [])
        allowed.update("allowlists:" + json.dumps(item, sort_keys=True, default=str) for item in lists)
    for line in (inputs.get("ignore") or "").splitlines():
        if line.strip() and not line.strip().startswith("#"):
            allowed.add("ignore:" + line.strip())
    return allowed, doc


def narrower(tool, old, new):
    """Why a tool's configuration cannot be shown to be as strict as before, or None."""
    if old.get("config_hash") == new.get("config_hash"):
        return None
    before, after = explains(tool, old), explains(tool, new)
    if before is None or after is None:
        return "changed and not explained"
    if tool == "deadnix":
        return "fail turned off" if before.get("fail") == "true" and after.get("fail") == "false" else None
    old_shape, new_shape = shape(tool, before), shape(tool, after)
    if old_shape is None or new_shape is None:
        return "not valid TOML"
    if old_shape[1] != new_shape[1]:
        return "a setting changed"
    added = sorted(new_shape[0] - old_shape[0])
    return "added " + ", ".join(added) if added else None


def read_baseline(path):
    """The baseline as a dict of the expected shape, or None."""
    try:
        doc = json.loads(path.read_text(encoding="utf-8"))
        tools = doc["lint"].items()
        counts = [n for _, tool in tools for n in tool["files"].values()]
        if not isinstance(doc["acks"], int) or not all(isinstance(n, int) for n in counts):
            return None
        set(doc["laws"])
    except (OSError, ValueError, KeyError, TypeError, AttributeError):
        return None
    return doc


def tool_findings(name, old, new):
    found = []
    why = narrower(name, old, new)
    if why:
        found.append((f"lint.{name}.config", "as recorded", why))
    for file, count in sorted(new["files"].items()):
        before = old["files"].get(file, 0)
        if count > before:
            found.append((f"lint.{name}.files.{file}", before, count))
    return found


def count_findings(base, head):
    found = []
    for name, old in sorted(base["lint"].items()):
        if name not in head["lint"]:
            found.append((f"lint.{name}", "present", "dropped"))
        else:
            found += tool_findings(name, old, head["lint"][name])
    if head["acks"] > base["acks"]:
        found.append(("acks", base["acks"], head["acks"]))
    found += [("laws", name, "dropped") for name in sorted(set(base["laws"]) - set(head["laws"]))]
    return found


def baseline_findings(base_dir, head_dir):
    """The committed ratchet may only tighten without the label."""
    config = read_toml(base_dir / "nix-tops.toml") or {}
    rel = config.get("baseline", BASELINE)
    if not isinstance(rel, str) or not (base_dir / rel).is_file():
        return []
    head = head_dir / rel
    if head.is_symlink() or behind_a_link(head_dir, Path(rel)):
        return [(rel, "file", "present", "a link")]
    if not head.is_file():
        return [(rel, "file", "present", "removed")]
    old, new = read_baseline(base_dir / rel), read_baseline(head)
    if old is None or new is None:
        return [(rel, "file", "a baseline", "unreadable")]
    return [(rel, *item) for item in count_findings(old, new)]


def weakenings(base_dir, head_dir):
    """Every weakening between two checkouts, as (file, key, old, new). Raises Unreadable."""
    base_dir, head_dir = Path(base_dir), Path(head_dir)
    found = compare_files(base_dir, head_dir, "nix-tops.toml", config_findings)
    found += compare_files(base_dir, head_dir, "laws/*.toml", law_findings)
    found += compare_files(base_dir, head_dir, "profiles/*.toml", settings_findings)
    found += baseline_findings(base_dir, head_dir)
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
