"""Each pair passes on the tree, and a drift on either side fails naming where."""

import contextlib
import io
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(REPO / "scripts"))

import sync_check  # noqa: E402

FACETS = sorted(p.stem for p in (REPO / sync_check.PROFILES).glob("*.toml"))
FILES = [
    sync_check.KIT_WORKFLOW,
    sync_check.TEMPLATE_WORKFLOW,
    sync_check.SPEC,
    sync_check.QUICKSTART,
    sync_check.AGENTS,
    sync_check.AGENTS_TEMPLATE,
    *(f"{sync_check.PROFILES}/{f}.toml" for f in FACETS),
    *(f"{sync_check.GOLDENS}/{f}/{name}" for f in FACETS for name in ("AGENTS.md", "nix-tops.toml")),
]
GOLDEN = f"{sync_check.GOLDENS}/dev-flake/AGENTS.md"


class Sync(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.root = Path(tmp.name)
        for rel in FILES:
            (self.root / rel).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy(REPO / rel, self.root / rel)

    def run_check(self):
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            status = sync_check.main(["--root", str(self.root)])
        return status, out.getvalue() + err.getvalue()

    def edit(self, rel, old, new, count=1):
        path = self.root / rel
        text = path.read_text(encoding="utf-8")
        self.assertIn(old, text)
        path.write_text(text.replace(old, new, count), encoding="utf-8")

    def line_of(self, rel, needle):
        lines = (self.root / rel).read_text(encoding="utf-8").splitlines()
        return next(n for n, line in enumerate(lines, 1) if needle in line)

    def assert_drift(self, pair, *expected):
        status, out = self.run_check()
        self.assertEqual(status, 1, out)
        self.assertIn(f"DRIFT {pair}", out)
        for text in expected:
            self.assertIn(text, out)

    def test_the_tree_is_in_sync(self):
        self.assertEqual(self.run_check(), (0, "ok   workflow\nok   agents\nok   quickstart\n"))

    def test_a_step_only_in_the_kit_workflow_names_the_file_and_line(self):
        self.edit(sync_check.KIT_WORKFLOW, "      - name: Gate\n", "      - run: echo extra\n      - name: Gate\n")
        line = self.line_of(sync_check.KIT_WORKFLOW, "echo extra")
        self.assert_drift("workflow", f"{sync_check.KIT_WORKFLOW}:{line}: not in the template")

    def test_a_step_after_the_last_job_is_drift_too(self):
        path = self.root / sync_check.KIT_WORKFLOW
        path.write_text(path.read_text(encoding="utf-8") + "      - run: echo tail\n", encoding="utf-8")
        line = self.line_of(sync_check.KIT_WORKFLOW, "echo tail")
        self.assert_drift("workflow", f"{sync_check.KIT_WORKFLOW}:{line}: not in the template")

    def test_a_dropped_addition_is_missing(self):
        self.edit(sync_check.KIT_WORKFLOW, "      - name: Every enabled layer ran\n        run: python3 scripts/layers_ran.py\n", "")
        self.assert_drift("workflow", "missing here: '- name: Every enabled layer ran'")

    def test_a_template_change_the_kit_did_not_take(self):
        self.edit(sync_check.TEMPLATE_WORKFLOW, "timeout-minutes: 45", "timeout-minutes: 30")
        self.assert_drift("workflow", "missing here: 'timeout-minutes: 30'", "'timeout-minutes: 45'")

    def test_a_template_change_that_removes_an_anchor(self):
        self.edit(sync_check.TEMPLATE_WORKFLOW, "  pull_request:\n", "  merge_group:\n")
        self.assert_drift("workflow", "no longer apply")

    def test_comments_are_not_compared(self):
        self.edit(sync_check.KIT_WORKFLOW, "jobs:\n", "# a note\n\njobs:\n")
        self.assertEqual(self.run_check()[0], 0)

    def test_there_are_six_facets(self):
        self.assertEqual(len(FACETS), 6)

    def test_an_edit_to_the_template_outside_the_rules_fails_every_golden(self):
        self.edit(sync_check.AGENTS_TEMPLATE, "## Commands", "## Command list")
        line = self.line_of(sync_check.AGENTS_TEMPLATE, "## Command list")
        self.assert_drift("agents", *(f"{sync_check.GOLDENS}/{f}/AGENTS.md:" for f in FACETS), f"{sync_check.AGENTS_TEMPLATE}:{line} renders '## Command list'")

    def test_an_edit_to_the_template_rules_fails_the_goldens_and_agents_md(self):
        self.edit(sync_check.AGENTS_TEMPLATE, "**Oracle first.**", "**Oracle second.**")
        line = self.line_of(sync_check.AGENTS_TEMPLATE, "Oracle second")
        mine = self.line_of(sync_check.AGENTS, "**Oracle first.**")
        self.assert_drift("agents", GOLDEN, f"{sync_check.AGENTS}:{mine}:", f"{sync_check.AGENTS_TEMPLATE}:{line} has")

    def test_an_edit_to_one_golden_names_its_line(self):
        self.edit(GOLDEN, "**Enforce:** `ratchet`", "**Enforce:** `advisory`")
        line = self.line_of(GOLDEN, "**Enforce:**")
        self.assert_drift("agents", f"{GOLDEN}:{line}:")

    def test_a_line_added_to_or_removed_from_a_golden(self):
        path = self.root / GOLDEN
        path.write_text(path.read_text(encoding="utf-8") + "more\n", encoding="utf-8")
        line = self.line_of(GOLDEN, "more")
        self.assert_drift("agents", f"{GOLDEN}:{line}:")
        path.write_text(path.read_text(encoding="utf-8").replace("## Commands\n", ""), encoding="utf-8")
        self.assert_drift("agents", "'## Commands'")

    def test_a_field_the_template_no_longer_names_is_drift(self):
        self.edit(sync_check.AGENTS_TEMPLATE, "{ENFORCE}", "{MODE}")
        self.assert_drift("agents", "{MODE}")

    def test_an_edit_to_the_rules_in_agents_md(self):
        self.edit(sync_check.AGENTS, "**Pure or nothing.**", "**Mostly pure.**")
        line = self.line_of(sync_check.AGENTS, "Mostly pure")
        self.assert_drift("agents", f"{sync_check.AGENTS}:{line}:")

    def test_text_in_agents_md_outside_the_rules_is_free(self):
        self.edit(sync_check.AGENTS, "## Git", "## Version control")
        self.assertEqual(self.run_check()[0], 0)

    def test_a_missing_golden_is_an_error(self):
        (self.root / GOLDEN).unlink()
        status, out = self.run_check()
        self.assertEqual(status, 2, out)
        self.assertIn(GOLDEN, out)

    def test_an_edit_to_the_quickstart_flake(self):
        self.edit(sync_check.QUICKSTART, "nix-tops.url", "nixtops.url")
        line = self.line_of(sync_check.QUICKSTART, "nixtops.url")
        self.assert_drift("quickstart", f"{sync_check.QUICKSTART}:{line}:")

    def test_an_edit_to_the_section_4_block(self):
        self.edit(sync_check.SPEC, "nix-tops.url", "nixtops.url")
        line = self.line_of(sync_check.SPEC, "nixtops.url")
        self.assert_drift("quickstart", f"{sync_check.SPEC}:{line}:")

    def test_a_line_added_at_the_end_of_the_flake(self):
        path = self.root / sync_check.QUICKSTART
        path.write_text(path.read_text(encoding="utf-8") + "# more\n", encoding="utf-8")
        self.assert_drift("quickstart", "the end")

    def test_a_missing_block_or_file_is_an_error(self):
        self.edit(sync_check.SPEC, "## 4. Quick start", "## 4. Start")
        (self.root / sync_check.KIT_WORKFLOW).unlink()
        status, out = self.run_check()
        self.assertEqual(status, 2, out)
        self.assertIn("no nix block in section 4", out)
        self.assertIn(sync_check.KIT_WORKFLOW, out)


if __name__ == "__main__":
    unittest.main()
