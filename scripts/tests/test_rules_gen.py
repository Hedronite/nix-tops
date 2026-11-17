"""Tests for scripts/rules_gen.py. They build scratch trees; the real repo has no markers until G8 phase 2."""

import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent
sys.path.insert(0, str(HERE.parent))
import rules_gen as rg  # noqa: E402

SPEC = """# Title

### 3.4 Stated rules

1. **Oracle first.** Write the oracle
   from the contract.
2. **Pure or nothing.** No `--impure`. (N-25)

---

## 13. Policies

Intro.

<!-- rules:begin -->
stale
<!-- rules:end -->

Tail.
"""
TEMPLATE = "head\n<!-- rules:begin -->\n<!-- rules:end -->\ntail\n"
AGENTS = "# A\n\n## Forbidden\n\n<!-- rules:begin -->\nhand kept\n<!-- rules:end -->\n"


def tree(spec=SPEC, template=TEMPLATE, agents=AGENTS):
    tmp = tempfile.TemporaryDirectory()
    root = Path(tmp.name)
    (root / "config").mkdir()
    (root / "NIX_TOPS.md").write_text(spec, encoding="utf-8")
    (root / "config/AGENTS.template.md").write_text(template, encoding="utf-8")
    (root / "AGENTS.md").write_text(agents, encoding="utf-8")
    return tmp, root


class Parse(unittest.TestCase):
    def test_wrapped_items_join(self):
        rules = rg.parse_rules(SPEC)
        self.assertEqual(rules[0], (1, "**Oracle first.** Write the oracle from the contract."))
        self.assertEqual(len(rules), 2)

    def test_stops_at_rule_line(self):
        self.assertEqual(len(rg.parse_rules(SPEC)), 2)

    def test_gap_in_numbers_fails(self):
        with self.assertRaises(rg.GenError):
            rg.parse_rules("### 3.4 R\n\n1. a\n3. b\n")

    def test_prose_in_list_fails(self):
        with self.assertRaises(rg.GenError):
            rg.parse_rules("### 3.4 R\n\n1. a\nloose prose\n")

    def test_no_section_fails(self):
        with self.assertRaises(rg.GenError):
            rg.parse_rules("# T\n")

    def test_real_spec_parses(self):
        rules = rg.parse_rules((REPO / "NIX_TOPS.md").read_text(encoding="utf-8"))
        self.assertGreaterEqual(len(rules), 1)


class Render(unittest.TestCase):
    def test_styles(self):
        rules = [(1, "a"), (2, "b")]
        self.assertIn("\n1. a\n2. b", rg.render(rules, "numbered"))
        self.assertIn("\n- a\n- b", rg.render(rules, "bullets"))


class WriteCheck(unittest.TestCase):
    def test_write_then_check_clean(self):
        tmp, root = tree()
        self.addCleanup(tmp.cleanup)
        self.assertEqual(rg.run(root, "check")[0], 1)  # stale copies drift
        rg.run(root, "write")
        self.assertEqual(rg.run(root, "check")[0], 0)
        self.assertIn("1. **Oracle first.**", (root / "config/AGENTS.template.md").read_text())
        self.assertIn("- **Pure or nothing.**", (root / "AGENTS.md").read_text())
        self.assertIn("Tail.", (root / "NIX_TOPS.md").read_text())

    def test_write_is_idempotent(self):
        tmp, root = tree()
        self.addCleanup(tmp.cleanup)
        rg.run(root, "write")
        before = [(root / f).read_text() for f, _ in rg.TARGETS]
        status, out = rg.run(root, "write")
        self.assertEqual(status, 0)
        self.assertNotIn("wrote", out)
        self.assertEqual(before, [(root / f).read_text() for f, _ in rg.TARGETS])

    def test_editing_one_copy_fails_check(self):
        tmp, root = tree()
        self.addCleanup(tmp.cleanup)
        rg.run(root, "write")
        p = root / "AGENTS.md"
        p.write_text(p.read_text().replace("Pure or nothing", "Pure-ish"), encoding="utf-8")
        status, out = rg.run(root, "check")
        self.assertEqual(status, 1)
        self.assertIn("DRIFT AGENTS.md", out)
        self.assertNotIn("DRIFT config", out)

    def test_editing_the_source_fails_check(self):
        tmp, root = tree()
        self.addCleanup(tmp.cleanup)
        rg.run(root, "write")
        p = root / "NIX_TOPS.md"
        p.write_text(p.read_text().replace("Oracle first", "Oracle second"), encoding="utf-8")
        self.assertEqual(rg.run(root, "check")[0], 1)

    def test_text_outside_markers_is_kept(self):
        tmp, root = tree()
        self.addCleanup(tmp.cleanup)
        rg.run(root, "write")
        t = (root / "config/AGENTS.template.md").read_text()
        self.assertTrue(t.startswith("head\n") and t.endswith("tail\n"))

    def test_marker_problems_fail(self):
        for bad in ("x\n", "<!-- rules:begin -->\n", "<!-- rules:end -->\n<!-- rules:begin -->\n",
                    TEMPLATE + TEMPLATE):
            tmp, root = tree(template=bad)
            self.addCleanup(tmp.cleanup)
            with self.assertRaises(rg.GenError, msg=bad):
                rg.run(root, "check")

    def test_unmarked_allowed_only_on_request(self):
        tmp, root = tree(agents="# A\n")
        self.addCleanup(tmp.cleanup)
        with self.assertRaises(rg.GenError):
            rg.run(root, "check")
        status, out = rg.run(root, "check", allow_unmarked=True)
        self.assertIn("skip AGENTS.md", out)

    def test_main_exit_codes(self):
        tmp, root = tree()
        self.addCleanup(tmp.cleanup)
        self.assertEqual(rg.main(["check", "--root", str(root)]), 1)
        self.assertEqual(rg.main(["write", "--root", str(root)]), 0)
        self.assertEqual(rg.main(["check", "--root", str(root)]), 0)
        self.assertEqual(rg.main(["check", "--root", str(root / "config")]), 2)

    def test_render_prints_all_targets(self):
        tmp, root = tree()
        self.addCleanup(tmp.cleanup)
        status, out = rg.run(root, "render")
        self.assertEqual(status, 0)
        for f, _ in rg.TARGETS:
            self.assertIn(f"== {f}", out)


if __name__ == "__main__":
    unittest.main()
