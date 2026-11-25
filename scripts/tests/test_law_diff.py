"""Tests for scripts/law_diff.py: one pair of trees per row of the comparison, with and without the label."""

import contextlib
import io
import os
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
import law_diff  # noqa: E402

CONFIG = """protocol = "0.2.0"
facets = ["package-set", "dev-flake"]
enforce = "hard"
baseline = ".nix-tops/baseline.json"

[toolchain]
systems = ["x86_64-linux", "aarch64-linux"]

[eval]
all_systems = true
abort_on_warn = true
allow_ifd = false

[coverage]
built_floor = 100
tested_floor = 50

[budgets.closure]
mode = "ratchet"
outputs = ["a", "b"]

[budgets.eval_cost]
mode = "advisory"
tolerance_pct = 10

[lint.deadnix]
fail = true

[lint.statix]
config = "statix.toml"

[laws]
enable = ["N-01", "N-02"]
ack_ledger = ".nix-tops/acks.toml"

[layers]
lint = { enabled = false, reason = "not built in M1" }
custom = [{ id = "extra", command = "true", tier = 1 }]

[[warnings.allow]]
match = "unknown setting"
reason = "daemon only"
until = "2027-01-01"
"""

LAW = """id = "N-01"
severity = "Refuse"
artifacts = ["output", "flake"]
reader = "ev"
judged = true
"""

PROFILE = """facet = "dev-flake"
[eval]
all_systems = true
[coverage]
built_floor = 100
[layers]
rebuild = { enabled = false }
"""


EXTRAS = {
    "NIX_TOPS.md": "# protocol\n",
    "schema/nix-tops.schema.json": "{}\n",
    "laws/diagnostics.toml": "x = 1\n",
    ".github/workflows/nix-tops.yml": "name: x\n",
    "scripts/law_diff.py": "# base\n",
    "scripts/layers_ran.py": "# base\n",
}


def tree(root, config=CONFIG, law=LAW, profile=PROFILE):
    root = Path(root)
    (root / "laws").mkdir(parents=True)
    (root / "profiles").mkdir()
    for name, text in EXTRAS.items():
        (root / name).parent.mkdir(parents=True, exist_ok=True)
        (root / name).write_text(text)
    if config is not None:
        (root / "nix-tops.toml").write_text(config)
    if law is not None:
        (root / "laws" / "N-01.toml").write_text(law)
    if profile is not None:
        (root / "profiles" / "dev-flake.toml").write_text(profile)


class Pair(unittest.TestCase):
    """Two trees: the base as written above, the head with one change."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.base = Path(self.tmp.name) / "base"
        self.head = Path(self.tmp.name) / "head"
        tree(self.base)

    def make_head(self, **kwargs):
        shutil.rmtree(self.head, ignore_errors=True)
        tree(self.head, **kwargs)

    def keys(self):
        """The findings of the key rules; the path rule has its own tests."""
        return [(f, k) for f, k, _, _ in law_diff.weakenings(self.base, self.head) if k != "protected path"]

    def paths(self):
        found = law_diff.weakenings(self.base, self.head)
        return [(f, new) for f, k, _, new in found if k == "protected path"]

    def edit(self, old, new, **kwargs):
        config = kwargs.pop("config", CONFIG)
        assert old in config, old
        self.make_head(config=config.replace(old, new), **kwargs)

    def run_main(self, labels=""):
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = law_diff.main(["--base", str(self.base), "--head", str(self.head), "--labels", labels])
        return code, out.getvalue()


class Unchanged(Pair):
    def test_the_same_trees_have_no_weakening_and_no_output(self):
        self.make_head()
        self.assertEqual(self.keys(), [])
        self.assertEqual(self.run_main(), (0, ""))

    def test_a_strengthening_is_not_a_weakening(self):
        config = CONFIG.replace("tested_floor = 50", "tested_floor = 60").replace('mode = "ratchet"', 'mode = "hard"')
        config = config.replace("tolerance_pct = 10", "tolerance_pct = 5").replace('"N-02"]', '"N-02", "N-03"]')
        self.make_head(config=config)
        self.assertEqual(self.keys(), [])


class Paths(Pair):
    def test_a_change_under_each_protected_path_is_found(self):
        for name in EXTRAS:
            self.make_head()
            (self.head / name).write_text("x = 3\n")
            self.assertEqual(self.paths(), [(name, "changed")], name)

    def test_a_file_added_or_removed_under_a_protected_path_is_found(self):
        self.make_head()
        (self.head / "laws" / "N-90.toml").write_text('id = "N-90"\n')
        (self.head / "schema" / "new.schema.json").write_text("{}\n")
        (self.head / "scripts" / "layers_ran.py").unlink()
        (self.head / ".github" / "workflows" / "other.yml").write_text("name: y\n")
        self.assertEqual(
            sorted(self.paths()),
            [
                (".github/workflows/other.yml", "added"),
                ("laws/N-90.toml", "added"),
                ("schema/new.schema.json", "added"),
                ("scripts/layers_ran.py", "removed"),
            ],
        )

    def test_a_change_elsewhere_is_not_a_protected_path(self):
        self.make_head()
        (self.head / "README.md").write_text("changed\n")
        (self.head / "scripts" / "spec_check.py").write_text("changed\n")
        self.assertEqual(self.paths(), [])

    def test_the_label_rule_covers_a_path_with_no_key_moved(self):
        self.make_head()
        (self.head / "laws" / "diagnostics.toml").write_text("x = 2\n")
        code, out = self.run_main("docs")
        self.assertEqual(code, 1)
        self.assertIn("law-diff: weakening: laws/diagnostics.toml: protected path: unchanged -> changed", out)
        self.assertEqual(self.run_main("protocol-change")[0], 0)


class Links(Pair):
    """A link under a protected path is a change, however it points."""

    def swap(self, name, target):
        """Put a link where the head's file or directory was."""
        path = self.head / name
        if path.is_dir() and not path.is_symlink():
            shutil.rmtree(path)
        elif path.exists() or path.is_symlink():
            path.unlink()
        path.parent.mkdir(parents=True, exist_ok=True)
        os.symlink(target, path)

    def test_a_relative_link_into_the_base_checkout_on_a_file(self):
        self.make_head()
        self.swap("laws/N-01.toml", "../../base/laws/N-01.toml")
        self.assertEqual(self.paths(), [("laws/N-01.toml", "symlink")])
        self.assertEqual(self.run_main("docs")[0], 1)
        self.assertEqual(self.run_main("protocol-change")[0], 0)

    def test_a_relative_link_on_a_directory_is_one_finding_and_is_not_entered(self):
        for name in ("laws", "schema", ".github/workflows", "profiles"):
            self.make_head()
            self.swap(name, os.path.relpath(self.base / name, (self.head / name).parent))
            self.assertEqual(self.paths(), [(name, "symlink")], name)

    def test_a_link_on_the_way_to_a_protected_path_is_the_finding(self):
        self.make_head()
        self.swap(".github", "../base/.github")
        self.assertEqual(self.paths(), [(".github", "symlink")])

    def test_an_absolute_link(self):
        self.make_head()
        self.swap("scripts/law_diff.py", str(self.base / "scripts" / "law_diff.py"))
        self.assertEqual(self.paths(), [("scripts/law_diff.py", "symlink")])
        self.make_head()
        self.swap("NIX_TOPS.md", str(self.base / "NIX_TOPS.md"))
        self.assertEqual(self.paths(), [("NIX_TOPS.md", "symlink")])

    def test_a_link_added_where_there_was_no_file(self):
        self.make_head()
        self.swap("laws/N-77.toml", "nowhere.toml")
        self.assertEqual(self.paths(), [("laws/N-77.toml", "symlink")])

    def test_a_link_in_the_base_replaced_by_a_file_is_a_change(self):
        shutil.rmtree(self.base)
        tree(self.base)
        (self.base / "scripts" / "layers_ran.py").unlink()
        os.symlink("law_diff.py", self.base / "scripts" / "layers_ran.py")
        self.make_head()
        self.assertEqual(self.paths(), [("scripts/layers_ran.py", "changed")])

    def test_a_link_outside_the_protected_paths_is_not_a_finding(self):
        self.make_head()
        os.symlink("../base/README.md", self.head / "README.md")
        self.assertEqual(self.paths(), [])


class Config(Pair):
    def test_enforce_lowered(self):
        self.edit('enforce = "hard"', 'enforce = "ratchet"')
        self.assertEqual(self.keys(), [("nix-tops.toml", "enforce")])
        self.edit('enforce = "hard"', 'enforce = "advisory"')
        self.assertEqual(self.keys(), [("nix-tops.toml", "enforce")])

    def test_a_missing_enforce_is_ratchet(self):
        self.edit('enforce = "hard"\n', "")
        self.assertEqual(self.keys(), [("nix-tops.toml", "enforce")])

    def test_a_layer_switched_off(self):
        config = CONFIG + '\n[layers.coverage]\nenabled = false\nreason = "later"\n'
        config = config.replace("[layers]\n", "[layers]\n", 1)
        self.make_head(config=config.replace('custom = [{ id = "extra", command = "true", tier = 1 }]\n', ""))
        keys = self.keys()
        self.assertIn(("nix-tops.toml", "layers.coverage"), keys)
        self.assertIn(("nix-tops.toml", "layers.custom.extra"), keys)

    def test_a_layer_already_off_with_a_new_reason_is_fine(self):
        self.edit("not built in M1", "another reason")
        self.assertEqual(self.keys(), [])

    def test_a_custom_layer_moved_to_a_later_tier(self):
        self.edit("tier = 1", "tier = 2")
        self.assertEqual(self.keys(), [("nix-tops.toml", "layers.custom.extra.tier")])

    def test_eval_options_weakened(self):
        self.edit("all_systems = true", "all_systems = false")
        self.assertEqual(self.keys(), [("nix-tops.toml", "eval.all_systems")])
        self.edit("allow_ifd = false", "allow_ifd = true")
        self.assertEqual(self.keys(), [("nix-tops.toml", "eval.allow_ifd")])
        self.edit("abort_on_warn = true", "abort_on_warn = false")
        self.assertEqual(self.keys(), [("nix-tops.toml", "eval.abort_on_warn")])

    def test_coverage_floors_lowered_or_removed(self):
        self.edit("built_floor = 100", "built_floor = 90")
        self.assertEqual(self.keys(), [("nix-tops.toml", "coverage.built_floor")])
        self.edit("tested_floor = 50\n", "")
        self.assertEqual(self.keys(), [("nix-tops.toml", "coverage.tested_floor")])

    def test_budgets(self):
        self.edit('mode = "ratchet"', 'mode = "advisory"')
        self.assertEqual(self.keys(), [("nix-tops.toml", "budgets.closure.mode")])
        self.edit("tolerance_pct = 10", "tolerance_pct = 20")
        self.assertEqual(self.keys(), [("nix-tops.toml", "budgets.eval_cost.tolerance_pct")])
        self.edit('["a", "b"]', '["a"]')
        self.assertEqual(self.keys(), [("nix-tops.toml", "budgets.closure.outputs")])

    def test_lint(self):
        self.edit("fail = true", "fail = false")
        self.assertEqual(self.keys(), [("nix-tops.toml", "lint.deadnix.fail")])
        self.edit('[lint.statix]\nconfig = "statix.toml"\n', "")
        self.assertEqual(self.keys(), [("nix-tops.toml", "lint.statix")])

    def test_laws_narrowed_and_the_ledger_moved(self):
        self.edit('["N-01", "N-02"]', '["N-01"]')
        self.assertEqual(self.keys(), [("nix-tops.toml", "laws.enable")])
        self.edit(".nix-tops/acks.toml", "other.toml")
        self.assertEqual(self.keys(), [("nix-tops.toml", "laws.ack_ledger")])
        self.edit('["N-01", "N-02"]', "[]")
        self.assertEqual(len(self.keys()), 2)

    def test_a_default_law_set_replaced_by_a_list_is_narrowing(self):
        self.base = Path(self.tmp.name) / "base2"
        tree(self.base, config=CONFIG.replace('enable = ["N-01", "N-02"]', 'enable = "default"'))
        self.make_head()
        self.assertEqual(self.keys(), [("nix-tops.toml", "laws.enable")])

    def test_facets_systems_and_baseline_dropped(self):
        self.edit('"package-set", ', "")
        self.assertEqual(self.keys(), [("nix-tops.toml", "facets")])
        self.edit(', "aarch64-linux"', "")
        self.assertEqual(self.keys(), [("nix-tops.toml", "toolchain.systems")])
        self.edit(".nix-tops/baseline.json", "lenient.json")
        self.assertEqual(self.keys(), [("nix-tops.toml", "baseline")])

    def test_the_config_file_removed(self):
        self.make_head(config=None)
        self.assertEqual(self.keys(), [("nix-tops.toml", "file")])


class Ledger(Pair):
    ENTRY = '\n[[warnings.allow]]\nmatch = "{m}"\nreason = "r"\nuntil = "{u}"\n'

    def test_a_new_entry_is_a_weakening(self):
        self.make_head(config=CONFIG + self.ENTRY.format(m="another", u="2026-12-01"))
        self.assertEqual(self.keys(), [("nix-tops.toml", "warnings.allow")])

    def test_a_later_until_or_a_wider_match_is_a_weakening(self):
        self.edit("2027-01-01", "2027-06-01")
        self.assertEqual(self.keys(), [("nix-tops.toml", "warnings.allow")])
        self.edit('"unknown setting"', '"unknown"')
        self.assertEqual(self.keys(), [("nix-tops.toml", "warnings.allow")])

    def test_an_earlier_until_or_a_removed_entry_is_fine(self):
        self.edit("2027-01-01", "2026-12-01")
        self.assertEqual(self.keys(), [])
        self.make_head(config=CONFIG.split("[[warnings.allow]]")[0])
        self.assertEqual(self.keys(), [])


class Advisory(Pair):
    TABLE = '\n[advisory]\nreason = "r"\nuntil = "{u}"\n'

    def based_on(self, until):
        shutil.rmtree(self.base)
        tree(self.base, config=CONFIG + self.TABLE.format(u=until))

    def test_a_new_exemption_is_a_weakening(self):
        self.make_head(config=CONFIG + self.TABLE.format(u="2027-01-01"))
        self.assertEqual(self.keys(), [("nix-tops.toml", "advisory")])

    def test_a_later_until_is_a_weakening(self):
        self.based_on("2027-01-01")
        self.make_head(config=CONFIG + self.TABLE.format(u="2027-02-01"))
        self.assertEqual(self.keys(), [("nix-tops.toml", "advisory")])

    def test_an_earlier_until_or_a_removed_exemption_is_fine(self):
        self.based_on("2027-01-01")
        for head in (CONFIG + self.TABLE.format(u="2026-12-01"), CONFIG):
            self.make_head(config=head)
            self.assertEqual(self.keys(), [])


class LawsAndProfiles(Pair):
    def test_a_law_removed_lowered_or_narrowed(self):
        self.make_head(law=None)
        self.assertEqual(self.keys(), [("laws/N-01.toml", "file")])
        self.make_head(law=LAW.replace("Refuse", "Ack"))
        self.assertEqual(self.keys(), [("laws/N-01.toml", "severity")])
        self.make_head(law=LAW.replace("judged = true", "judged = false"))
        self.assertEqual(self.keys(), [("laws/N-01.toml", "judged")])
        self.make_head(law=LAW.replace('reader = "ev"', 'reader = "src"'))
        self.assertEqual(self.keys(), [("laws/N-01.toml", "reader")])
        self.make_head(law=LAW.replace('["output", "flake"]', '["output"]'))
        self.assertEqual(self.keys(), [("laws/N-01.toml", "artifacts")])

    def test_a_law_added_is_not_a_weakening(self):
        self.make_head()
        (self.head / "laws" / "N-02.toml").write_text(LAW.replace("N-01", "N-02"))
        self.assertEqual(self.keys(), [])

    def test_a_profile_removed_or_its_floor_lowered(self):
        self.make_head(profile=None)
        self.assertEqual(self.keys(), [("profiles/dev-flake.toml", "file")])
        self.make_head(profile=PROFILE.replace("built_floor = 100", "built_floor = 80"))
        self.assertEqual(self.keys(), [("profiles/dev-flake.toml", "coverage.built_floor")])
        self.make_head(profile=PROFILE.replace("all_systems = true", "all_systems = false"))
        self.assertEqual(self.keys(), [("profiles/dev-flake.toml", "eval.all_systems")])

    def test_a_profile_layer_newly_switched_off(self):
        self.make_head(profile=PROFILE + "vulnix = { enabled = false }\n")
        self.assertEqual(self.keys(), [("profiles/dev-flake.toml", "layers.vulnix")])


class Main(Pair):
    def test_without_the_label_a_weakening_exits_1_and_names_it(self):
        self.edit('enforce = "hard"', 'enforce = "ratchet"')
        code, out = self.run_main("docs, bug")
        self.assertEqual(code, 1)
        self.assertIn("law-diff: weakening: nix-tops.toml: enforce: hard -> ratchet", out)
        self.assertIn("without the protocol-change label", out)

    def test_with_the_label_the_same_list_prints_and_the_exit_is_0(self):
        self.edit('enforce = "hard"', 'enforce = "ratchet"')
        code, out = self.run_main("docs, protocol-change")
        self.assertEqual(code, 0)
        self.assertIn("nix-tops.toml: enforce: hard -> ratchet", out)
        self.assertIn("allowed by protocol-change", out)

    def test_a_label_does_not_match_by_prefix(self):
        self.edit('enforce = "hard"', 'enforce = "ratchet"')
        self.assertEqual(self.run_main("protocol-change-not")[0], 1)

    def test_a_head_file_that_does_not_parse_exits_2_and_the_label_does_not_clear_it(self):
        self.make_head(config="enforce = [")
        self.assertEqual(self.run_main("protocol-change")[0], 2)

    def test_a_new_head_file_that_does_not_parse_exits_2(self):
        self.make_head()
        (self.head / "laws" / "N-09.toml").write_text("id = [")
        self.assertEqual(self.run_main()[0], 2)

    def test_a_base_with_no_files_has_nothing_to_weaken(self):
        empty = Path(self.tmp.name) / "empty"
        empty.mkdir()
        self.make_head()
        found = [f for f in law_diff.weakenings(empty, self.head) if f[1] != "protected path"]
        self.assertEqual(found, [])


class RealRepo(unittest.TestCase):
    def test_the_repo_compared_with_itself_has_no_weakening(self):
        repo = Path(__file__).resolve().parent.parent.parent
        self.assertEqual(law_diff.weakenings(repo, repo), [])


if __name__ == "__main__":
    unittest.main()
