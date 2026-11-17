"""Tests for profiles/*.toml: shape and the section 5.2 floors."""

import sys
import tomllib
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent.parent
FACETS = ["dev-flake", "package-set", "nixos-module", "flake-lib", "nixos-host", "home-config"]
UNPROVEN = {"nixos-host", "home-config"}
BUILT = {"dev-flake": 100, "package-set": 100, "flake-lib": 100, "nixos-host": 100, "home-config": 100}
TESTED = {"dev-flake": 50, "package-set": 80, "nixos-module": 100, "flake-lib": 80, "nixos-host": 100}
VULNIX_TIER2 = {"package-set", "nixos-host", "home-config"}


def load(facet):
    with (REPO / "profiles" / f"{facet}.toml").open("rb") as f:
        return tomllib.load(f)


class Profiles(unittest.TestCase):
    def test_exactly_six_files(self):
        got = sorted(p.stem for p in (REPO / "profiles").glob("*.toml"))
        self.assertEqual(got, sorted(FACETS))

    def test_facet_matches_filename_and_no_laws_list(self):
        for f in FACETS:
            p = load(f)
            self.assertEqual(p["facet"], f)
            self.assertNotIn("laws", p, f)
            self.assertNotIn("class", p, f)

    def test_fixed_eval_options(self):
        for f in FACETS:
            e = load(f)["eval"]
            self.assertIs(e["abort_on_warn"], True, f)
            self.assertIs(e["allow_ifd"], False, f)

    def test_floors_match_section_5_2(self):
        for f in FACETS:
            cov = load(f).get("coverage", {})
            self.assertEqual(cov.get("built_floor"), BUILT.get(f), f)
            self.assertEqual(cov.get("tested_floor"), TESTED.get(f), f)

    def test_eval_cost_advisory_everywhere(self):
        for f in FACETS:
            self.assertEqual(load(f)["budgets"]["eval_cost"]["mode"], "advisory", f)

    def test_closure_budget_where_the_table_has_one(self):
        for f in FACETS:
            has = "closure" in load(f)["budgets"]
            self.assertEqual(has, f in {"dev-flake", "package-set", "nixos-host", "home-config"}, f)

    def test_vulnix_tier(self):
        for f in FACETS:
            v = load(f)["layers"]["vulnix"]
            self.assertEqual(v.get("tier") == 2, f in VULNIX_TIER2, f)

    def test_unproven_facets_say_so(self):
        for f in FACETS:
            text = (REPO / "profiles" / f"{f}.toml").read_text(encoding="utf-8")
            self.assertEqual("Specified, unproven" in text, f in UNPROVEN, f)


if __name__ == "__main__":
    unittest.main()
