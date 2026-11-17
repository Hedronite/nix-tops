"""Tests for scripts/spec_check.py. Each failing case plants one defect in a scratch copy."""

import shutil
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent
sys.path.insert(0, str(HERE.parent))
import spec_check as sc  # noqa: E402

HAVE_CJS = sc.check_jsonschema_cmd() is not None


class Scratch(unittest.TestCase):
    """A scratch copy of the whole repo, without .git."""

    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name)
        shutil.copytree(REPO, self.root, dirs_exist_ok=True, ignore=shutil.ignore_patterns(".git", "__pycache__"))

    def tearDown(self):
        self._tmp.cleanup()

    def edit(self, name, old, new, count=1):
        p = self.root / name
        text = p.read_text(encoding="utf-8")
        self.assertGreaterEqual(text.count(old), 1, f"{old!r} not in {name}")
        p.write_text(text.replace(old, new, count), encoding="utf-8")


class Slugs(unittest.TestCase):
    def test_github_slugs(self):
        self.assertEqual(sc.slugify("6. Scoring and reporting"), "6-scoring-and-reporting")
        self.assertEqual(sc.slugify("12. The `nixtops` CLI"), "12-the-nixtops-cli")
        self.assertEqual(
            sc.slugify("8.2 Law catalog (draft: 43 laws, N-01…N-43)"),
            "82-law-catalog-draft-43-laws-n-01n-43",
        )


class Links(Scratch):
    def test_repo_is_clean(self):
        self.assertEqual(sc.check_links(REPO), [])

    def test_broken_anchor_fails(self):
        self.edit("README.md", "NIX_TOPS.md#19-roadmap", "NIX_TOPS.md#19-no-such-heading")
        errs = sc.check_links(self.root)
        self.assertEqual(len(errs), 1)
        self.assertIn("no heading anchor", errs[0])

    def test_missing_file_fails(self):
        self.edit("README.md", "(AGENTS.md)", "(AGENTS-gone.md)")
        self.assertTrue(any("missing file" in e for e in sc.check_links(self.root)))

    def test_links_in_code_are_ignored(self):
        (self.root / "x.md").write_text("`[a](nope.md)`\n\n```\n[b](nope2.md)\n```\n", encoding="utf-8")
        self.assertEqual(sc.check_links(self.root), [])

    def test_duplicate_headings_get_suffixes(self):
        (self.root / "x.md").write_text("# A\n\n# A\n\n[t](#a-1)\n", encoding="utf-8")
        self.assertEqual(sc.check_links(self.root), [])


class Catalog(Scratch):
    def test_repo_is_clean(self):
        self.assertEqual(sc.check_catalog(REPO), [])

    def test_planted_check_mark_flip_fails(self):
        # N-37 is unjudged: mark it judged, and the counts in the text no longer match.
        self.edit("NIX_TOPS.md", "| A | ev | | module, configuration | |\n| N-38", "| A | ev | | module, configuration | ✓ |\n| N-38")
        errs = sc.check_catalog(self.root)
        self.assertTrue(any("table marks" in e for e in errs), errs)

    def test_flip_the_other_way_fails(self):
        self.edit("NIX_TOPS.md", "| ✓ |\n| N-02", "| |\n| N-02")
        self.assertTrue(sc.check_catalog(self.root))

    def test_heading_count_fails(self):
        self.edit("NIX_TOPS.md", "draft: 43 laws, N-01…N-43", "draft: 44 laws, N-01…N-44")
        self.assertTrue(any("heading" in e for e in sc.check_catalog(self.root)))

    def test_roadmap_row_fails(self):
        self.edit("NIX_TOPS.md", "Report shows \"29 of 43 judged\"", "Report shows \"28 of 43 judged\"")
        self.assertTrue(any("M3 row" in e for e in sc.check_catalog(self.root)))

    def test_missing_law_row_fails(self):
        text = (self.root / "NIX_TOPS.md").read_text(encoding="utf-8")
        kept = [l for l in text.splitlines() if not l.startswith("| N-20 |")]
        (self.root / "NIX_TOPS.md").write_text("\n".join(kept) + "\n", encoding="utf-8")
        self.assertTrue(sc.check_catalog(self.root))

    def test_laws_dir_one_file_per_row(self):
        laws = self.root / "laws"
        shutil.rmtree(laws, ignore_errors=True)
        laws.mkdir()
        for k in range(1, 44):
            (laws / f"N-{k:02d}.toml").write_text(f'id = "N-{k:02d}"\n', encoding="utf-8")
        self.assertEqual(sc.check_catalog(self.root), [])
        (laws / "N-07.toml").unlink()
        self.assertTrue(any("no file for N-07" in e for e in sc.check_catalog(self.root)))
        (laws / "N-07.toml").write_text("", encoding="utf-8")
        (laws / "N-99.toml").write_text("", encoding="utf-8")
        self.assertTrue(any("N-99" in e for e in sc.check_catalog(self.root)))


@unittest.skipUnless(HAVE_CJS, "check-jsonschema is not installed")
class Schema(Scratch):
    def test_repo_is_clean(self):
        self.assertEqual(sc.check_schema(REPO), [])

    def test_unknown_key_fails(self):
        self.edit("NIX_TOPS.md", 'enforce = "ratchet"', 'enforce = "ratchet"\nsurprise = 1')
        errs = sc.check_schema(self.root)
        self.assertTrue(errs and "surprise" in errs[0], errs)

    def test_unknown_nested_key_fails(self):
        self.edit("NIX_TOPS.md", "[eval]\nall_systems = true\nabort_on_warn = true\nallow_ifd = false\n\n[lint]", "[eval]\nall_systems = true\nnope = 1\nabort_on_warn = true\nallow_ifd = false\n\n[lint]")
        self.assertTrue(sc.check_schema(self.root))

    def test_fixed_option_cannot_change(self):
        self.edit("NIX_TOPS.md", "abort_on_warn = true\nallow_ifd = false\n\n[lint]", "abort_on_warn = false\nallow_ifd = false\n\n[lint]")
        self.assertTrue(sc.check_schema(self.root))

    def test_version_and_flake_are_optional(self):
        self.edit("NIX_TOPS.md", 'version = "0.1.0"\nflake = "github:Hedronite/example"\n', "")
        self.assertEqual(sc.check_schema(self.root), [])


class Cli(unittest.TestCase):
    def test_exit_status(self):
        self.assertEqual(sc.main(["links", "--root", str(REPO)]), 0)
        self.assertEqual(sc.main(["catalog", "--root", str(REPO)]), 0)
        with tempfile.TemporaryDirectory() as tmp:
            self.assertEqual(sc.main(["catalog", "--root", tmp]), 1)


if __name__ == "__main__":
    unittest.main()
