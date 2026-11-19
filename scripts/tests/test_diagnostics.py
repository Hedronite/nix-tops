"""Tests for laws/diagnostics.toml: the table that classifies the lines `nix` prints itself."""

import re
import tomllib
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent.parent
TABLE = tomllib.loads((REPO / "laws" / "diagnostics.toml").read_text(encoding="utf-8"))
ENTRIES = TABLE["diagnostic"]
SETTINGS = ["eval-cores", "lazy-trees", "bash-prompt-prefix", "upgrade-nix-store-path-url", "build-provenance-tags"]


def classify(line):
    """The entries a line matches; the runner takes the first, and none means the gate fails."""
    return [e for e in ENTRIES if re.fullmatch(e["match"], line)]


class Table(unittest.TestCase):
    def test_ids_are_unique_and_in_order(self):
        ids = [e["id"] for e in ENTRIES]
        self.assertEqual(ids, [f"D-{i:02d}" for i in range(1, len(ids) + 1)])

    def test_each_fixture_is_accepted_by_its_own_entry(self):
        for e in ENTRIES:
            self.assertIn(e["id"], [m["id"] for m in classify(e["fixture"])], e["id"])

    def test_every_effect_is_report_or_fail(self):
        for e in ENTRIES:
            self.assertIn(e["effect"], ("report", "fail"), e["id"])

    def test_the_table_version_moved_with_the_new_entries(self):
        self.assertEqual(TABLE["table_version"], 3)


class HostSettings(unittest.TestCase):
    def test_the_five_names_classify_as_report(self):
        for n in SETTINGS:
            hits = classify(f"warning: unknown setting '{n}'")
            self.assertEqual(len(hits), 1, n)
            self.assertEqual(hits[0]["effect"], "report")
            self.assertEqual(hits[0]["implementation"], "cppnix")

    def test_one_entry_per_name_and_no_general_pattern(self):
        for e in ENTRIES:
            if e["kind"] == "host-setting":
                name = re.fullmatch(r"\^warning: unknown setting '([^']+)'\$", e["match"])
                self.assertIsNotNone(name, e["id"])
                self.assertIn(name.group(1), SETTINGS)
        self.assertEqual(sum(e["kind"] == "host-setting" for e in ENTRIES), len(SETTINGS))

    def test_any_other_unknown_setting_stays_unclassified(self):
        for n in ["eval-core", "eval-coress", "lazy-tree", "experimental-features", "my-typo", "",
                  "eval-cores'; x", "bash-prompt", "EVAL-CORES"]:
            self.assertEqual(classify(f"warning: unknown setting '{n}'"), [], n)
        self.assertEqual(classify("warning: unknown setting 'eval-cores' extra"), [])
        self.assertEqual(classify("  warning: unknown setting 'eval-cores'"), [])


class DirtyTree(unittest.TestCase):
    PATHS = ["/home/user/flake", "/Users/someone/tmp/a b/flake", "/nix/store/x-source", "path:/tmp/x", "."]

    def test_each_implementation_has_one_entry_matching_any_path(self):
        for p in self.PATHS:
            c = classify(f"warning: Git tree '{p}' is dirty")
            self.assertEqual([(e["implementation"], e["effect"]) for e in c], [("cppnix", "report")], p)
            d = classify(f"warning: Git tree '{p}' has uncommitted changes")
            self.assertEqual([(e["implementation"], e["effect"]) for e in d], [("determinate", "report")], p)

    def test_the_wording_of_the_other_implementation_does_not_cross(self):
        self.assertEqual(classify("warning: Git tree is dirty"), [])
        self.assertEqual(classify("warning: Git tree '/x' is clean"), [])


if __name__ == "__main__":
    unittest.main()
