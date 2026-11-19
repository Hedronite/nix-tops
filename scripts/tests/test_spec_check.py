"""Tests for scripts/spec_check.py. Each failing case plants one defect in a scratch copy."""

import re
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

    def flip(self, law_id):
        """Toggle the check mark in the last cell of one law row, found by its ID."""
        p = self.root / "NIX_TOPS.md"
        lines = p.read_text(encoding="utf-8").splitlines()
        hits = [i for i, l in enumerate(lines) if l.startswith(f"| {law_id} |")]
        self.assertEqual(len(hits), 1, law_id)
        row = lines[hits[0]]
        flipped = re.sub(r"\|\s*✓\s*\|$", "| |", row) if re.search(r"\|\s*✓\s*\|$", row) else re.sub(r"\|\s*\|$", "| ✓ |", row)
        self.assertNotEqual(row, flipped, law_id)
        lines[hits[0]] = flipped
        p.write_text("\n".join(lines) + "\n", encoding="utf-8")

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
        self.flip("N-37")
        errs = sc.check_catalog(self.root)
        self.assertTrue(any("table marks" in e for e in errs), errs)

    def test_flip_the_other_way_fails(self):
        self.flip("N-01")
        self.assertTrue(any("table marks" in e for e in sc.check_catalog(self.root)))

    def test_heading_count_fails(self):
        self.edit("NIX_TOPS.md", "draft: 49 laws, N-01…N-49", "draft: 50 laws, N-01…N-50")
        self.assertTrue(any("heading" in e for e in sc.check_catalog(self.root)))

    def test_roadmap_row_fails(self):
        self.edit("NIX_TOPS.md", "Report shows \"29 of 49 judged\"", "Report shows \"28 of 49 judged\"")
        self.assertTrue(any("M3 row" in e for e in sc.check_catalog(self.root)))

    def test_readme_counts_join_the_check(self):
        self.edit("README.md", "29 of 49 judged", "29 of 43 judged")
        self.assertTrue(any(e.startswith("README.md") for e in sc.check_catalog(self.root)))

    def test_readme_without_the_count_fails(self):
        self.edit("README.md", "29 of 49 judged", "all laws judged")
        self.assertTrue(any("no 'N of M judged'" in e for e in sc.check_catalog(self.root)))

    def test_missing_law_row_fails(self):
        text = (self.root / "NIX_TOPS.md").read_text(encoding="utf-8")
        kept = [l for l in text.splitlines() if not l.startswith("| N-20 |")]
        (self.root / "NIX_TOPS.md").write_text("\n".join(kept) + "\n", encoding="utf-8")
        self.assertTrue(sc.check_catalog(self.root))

    def test_laws_dir_one_file_per_row(self):
        laws = self.root / "laws"
        shutil.rmtree(laws, ignore_errors=True)
        laws.mkdir()
        for k in range(1, 50):
            (laws / f"N-{k:02d}.toml").write_text(f'id = "N-{k:02d}"\n', encoding="utf-8")
        (laws / "N-07.toml").unlink()
        self.assertTrue(any("no file for N-07" in e for e in sc.check_catalog(self.root)))
        (laws / "N-07.toml").write_text("", encoding="utf-8")
        (laws / "N-99.toml").write_text("", encoding="utf-8")
        self.assertTrue(any("N-99" in e for e in sc.check_catalog(self.root)))


class LawFields(Scratch):
    """One planted flip per compared field: the check must fail and name the ID, the field and both values."""

    def law(self, law_id, old, new):
        """Change one line of a law file (the file side)."""
        self.edit(f"laws/{law_id}.toml", old, new)

    def errors(self):
        return sc.check_catalog(self.root)

    def assert_flagged(self, law_id, field, *values):
        errs = [e for e in self.errors() if f"{law_id} field '{field}'" in e]
        self.assertEqual(len(errs), 1, self.errors())
        for v in values:
            self.assertIn(v, errs[0])

    def test_repo_is_clean(self):
        self.assertEqual(self.errors(), [])

    def test_law_text_flip(self):
        self.law("N-05", "is committed and current", "is committed and stale")
        self.assert_flagged("N-05", "law", "committed and stale", "committed and current")

    def test_emphasis_and_whitespace_are_not_differences(self):
        self.law("N-17", "**no unmatched warnings**", "no   unmatched  warnings")
        self.assertEqual(self.errors(), [])

    def test_row_text_flip(self):
        self.edit("NIX_TOPS.md", "| N-05 | `flake.lock` is committed and current;", "| N-05 | `flake.lock` is committed and old;")
        self.assert_flagged("N-05", "law", "committed and old", "committed and current")

    def test_severity_flip(self):
        self.law("N-05", 'severity = "Refuse"', 'severity = "Ack"')
        self.assert_flagged("N-05", "severity", "Ack", "Refuse")

    def test_primary_reader_flip(self):
        self.law("N-05", 'reader = "ev"', 'reader = "an"')
        self.assert_flagged("N-05", "reader", "an", "ev")

    def test_further_readers_flip(self):
        self.law("N-01", 'reader_also = ["an"]', 'reader_also = ["tl"]')
        self.assert_flagged("N-01", "reader_also", "tl", "an")

    def test_reader_detail_flip(self):
        self.law("N-01", 'reader_detail = "ev, an: syntax"', 'reader_detail = "ev, an: types"')
        self.assert_flagged("N-01", "reader_detail", "ev, an: types", "ev, an: syntax")

    def test_bare_reader_may_omit_detail(self):
        text = (self.root / "laws/N-05.toml").read_text(encoding="utf-8")
        self.assertNotIn("reader_detail", text)
        self.assertEqual(self.errors(), [])

    def test_sketch_flip(self):
        self.law("N-01", 'sketch = "\u00a72"', 'sketch = "\u00a73"')
        self.assert_flagged("N-01", "sketch", "\u00a73", "\u00a72")

    def test_sketch_added_to_a_law_with_none_fails(self):
        self.law("N-05", 'judged = true', 'judged = true\nsketch = "\u00a71"')
        self.assert_flagged("N-05", "sketch", "\u00a71")

    def test_artifacts_flip(self):
        self.law("N-34", 'artifacts = ["module", "configuration"]', 'artifacts = ["module"]')
        self.assert_flagged("N-34", "artifacts", "['module']", "['module', 'configuration']")

    def test_judged_flip_file_side(self):
        self.law("N-05", "judged = true", "judged = false")
        self.assert_flagged("N-05", "judged", "False", "True")

    def test_judged_flip_row_side_also_trips_the_counts(self):
        self.flip("N-05")
        errs = self.errors()
        self.assertTrue(any("N-05 field 'judged'" in e for e in errs), errs)

    def test_id_mismatch(self):
        self.law("N-05", 'id = "N-05"', 'id = "N-06"')
        self.assertTrue(any("N-05.toml: field 'id'" in e for e in self.errors()))

    def test_row_with_wrong_cell_count_fails(self):
        self.edit("NIX_TOPS.md", "| N-05 | `flake.lock` is committed and current; evaluation with `--no-update-lock-file` succeeds | R |", "| N-05 | `flake.lock` is committed and current; evaluation with `--no-update-lock-file` succeeds |")
        self.assertTrue(any("N-05 has" in e for e in self.errors()))

    def test_escaped_pipe_stays_in_its_cell(self):
        self.assertEqual(sc.split_row(r"| N-99 | a \| b | R | ev | | flake | |"), ["N-99", "a | b", "R", "ev", "", "flake", ""])

    def test_readers_of_the_composite_cell(self):
        f = sc.row_fields(["N-99", "x", "R", "ev: derivation (base against head); an: diff; tl at tier 2", "", "flake", ""])
        self.assertEqual((f["reader"], f["reader_also"]), ("ev", ["an", "tl"]))


@unittest.skipUnless(HAVE_CJS, "check-jsonschema is not installed")
class Schema(Scratch):
    def test_repo_is_clean(self):
        self.assertEqual(sc.check_schema(REPO), [])

    def test_unknown_key_fails(self):
        self.edit("NIX_TOPS.md", 'enforce = "ratchet"', 'enforce = "ratchet"\nsurprise = 1')
        errs = sc.check_schema(self.root)
        self.assertTrue(errs and "surprise" in errs[0], errs)

    SYSTEMS = 'systems = ["x86_64-linux", "aarch64-linux", "aarch64-darwin"]   # the systems the flake exports'

    def test_unknown_nested_key_fails(self):
        self.edit("NIX_TOPS.md", self.SYSTEMS, self.SYSTEMS + "\nnope = 1")
        self.assertTrue(sc.check_schema(self.root))

    def test_fixed_option_cannot_change(self):
        self.edit("NIX_TOPS.md", "allow_ifd = false\n\n[[warnings.allow]]", "allow_ifd = true\n\n[[warnings.allow]]")
        self.assertTrue(sc.check_schema(self.root))

    def test_version_and_flake_are_dropped(self):
        self.edit("NIX_TOPS.md", 'enforce = "ratchet"', 'version = "0.2.0"\nenforce = "ratchet"')
        self.assertTrue(sc.check_schema(self.root))
        self.edit("NIX_TOPS.md", 'version = "0.2.0"\n', 'flake = "github:Hedronite/example"\n')
        self.assertTrue(sc.check_schema(self.root))

    def test_x86_64_darwin_is_not_a_system(self):
        self.edit("NIX_TOPS.md", self.SYSTEMS, 'systems = ["x86_64-linux", "x86_64-darwin"]')
        self.assertTrue(sc.check_schema(self.root))

    def test_config_needs_facets_and_systems(self):
        self.edit("NIX_TOPS.md", 'facets = ["dev-flake"]               # a subset of the facets the outputs show (§5.4)\n', "")
        self.assertTrue(sc.check_schema(self.root))

    def test_warning_entry_needs_all_three_fields(self):
        self.edit("NIX_TOPS.md", 'until = "2026-12-31"                 # UTC date; an expired entry fails (§3.1)\n', "")
        self.assertTrue(sc.check_schema(self.root))

    def test_profile_cannot_carry_both_facet_and_facets(self):
        p = self.root / "profiles" / "dev-flake.toml"
        p.write_text('facets = ["dev-flake"]\n' + p.read_text(encoding="utf-8"), encoding="utf-8")
        self.assertTrue(sc.check_schema(self.root))

    def test_profile_facet_must_match_the_file_name(self):
        p = self.root / "profiles" / "flake-lib.toml"
        p.write_text(p.read_text(encoding="utf-8").replace('facet = "flake-lib"', 'facet = "package-set"'), encoding="utf-8")
        self.assertTrue(any("file name says" in e for e in sc.check_schema(self.root)))

    def test_profile_with_unknown_key_fails(self):
        p = self.root / "profiles" / "package-set.toml"
        p.write_text(p.read_text(encoding="utf-8").replace('protocol = "0.2.0"', 'protocol = "0.2.0"\nsurprise = 1'), encoding="utf-8")
        self.assertTrue(sc.check_schema(self.root))

    def test_example_config_is_validated(self):
        p = self.root / "examples" / "quickstart" / "nix-tops.toml"
        p.write_text('class = "dev-flake"\n' + p.read_text(encoding="utf-8"), encoding="utf-8")
        self.assertTrue(sc.check_schema(self.root))


class Ids(Scratch):
    def test_repo_is_clean(self):
        self.assertEqual(sc.check_ids(REPO), [])

    def test_planted_labels_fail(self):
        for label in ("N1", "X2", "Q5", "R4", "P7", "G7", "M4.6"):
            with self.subTest(label=label):
                self.edit("README.md", "## Status", f"## Status\n\nSee {label} for why.")
                self.assertTrue(any(label in e for e in sc.check_ids(self.root)))
                self.edit("README.md", f"See {label} for why.", "See the plan for why.")

    def test_law_ids_and_code_spans_are_fine(self):
        self.edit("README.md", "## Status", "## Status\n\nLaw N-17, N-01 to N-49, `N1` and `R4` in code.")
        self.assertEqual(sc.check_ids(self.root), [])

    def test_labels_in_a_law_file_fail(self):
        p = self.root / "laws" / "N-01.toml"
        p.write_text(p.read_text(encoding="utf-8") + '\nrationale_note = "see Q5 for why"\n', encoding="utf-8")
        self.assertTrue(any("laws/N-01.toml" in e for e in sc.check_ids(self.root)))

    def test_inline_code_in_a_law_file_is_left_out(self):
        p = self.root / "laws" / "N-01.toml"
        p.write_text(p.read_text(encoding="utf-8") + '\nnote = "the `R4` span"\n', encoding="utf-8")
        self.assertEqual(sc.check_ids(self.root), [])

    def test_labels_in_a_fragment_fail(self):
        (self.root / "changelog.d").mkdir(exist_ok=True)
        (self.root / "changelog.d" / "x.md").write_text("- Closes R6.\n", encoding="utf-8")
        self.assertTrue(sc.check_ids(self.root))


class Cli(unittest.TestCase):
    def test_exit_status(self):
        self.assertEqual(sc.main(["links", "--root", str(REPO)]), 0)
        self.assertEqual(sc.main(["catalog", "--root", str(REPO)]), 0)
        self.assertEqual(sc.main(["ids", "--root", str(REPO)]), 0)
        with tempfile.TemporaryDirectory() as tmp:
            self.assertEqual(sc.main(["catalog", "--root", tmp]), 1)


if __name__ == "__main__":
    unittest.main()
