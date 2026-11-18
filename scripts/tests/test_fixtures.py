"""Structure checks for fixtures/corpus/*: every flake has what the runner and the gate need."""

import json
import re
import sys
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent
sys.path.insert(0, str(HERE.parent))
import spec_check as sc  # noqa: E402

CORPUS = REPO / "fixtures" / "corpus"
NAMES = sorted(p.name for p in CORPUS.iterdir() if p.is_dir())
FACETS = {"dev-flake", "package-set", "nixos-module", "flake-lib"}
LAYERS = {"source", "lint", "eval", "build-test", "coverage", "facet-layers", "laws"}
VERDICTS = {"pass", "fail"}
KIT_REV = json.loads((REPO / "flake.lock").read_text())["nodes"]["nixpkgs"]["locked"]["rev"]


class Corpus(unittest.TestCase):
    def test_seventeen_flakes_and_one_control_per_launch_facet(self):
        self.assertEqual(len(NAMES), 17, NAMES)
        controls = [n for n in NAMES if n.startswith("ctl-")]
        self.assertEqual(controls, ["ctl-dev-flake", "ctl-flake-lib", "ctl-nixos-module", "ctl-package-set"])
        for n in controls:
            text = (CORPUS / n / "nix-tops.toml").read_text()
            self.assertIn(f'facets = ["{n[4:]}"]', text)

    def test_each_fixture_has_its_files(self):
        for n in NAMES:
            d = CORPUS / n
            for f in ("flake.nix", "nix-tops.toml", "expected.json"):
                self.assertTrue((d / f).is_file(), f"{n}: no {f}")
            if n != "src-lock-missing" and not (d / "setup.sh").exists():
                inputs = re.search(r"\binputs\.", (d / "flake.nix").read_text())
                self.assertEqual(bool(inputs), (d / "flake.lock").is_file(), f"{n}: flake.lock iff it has inputs")

    def test_expected_json_shape(self):
        for n in NAMES:
            e = json.loads((CORPUS / n / "expected.json").read_text())
            self.assertIn(e["exit"], (0, 1, 2), n)
            self.assertTrue(set(e["layers"]) <= LAYERS, n)
            self.assertTrue(set(e["layers"].values()) <= VERDICTS, n)
            self.assertIn(e["raw"]["exit"], (0, 1), n)
            self.assertTrue(e["note"], n)
            if e["exit"] == 2:
                # an invalid configuration stops the gate before any layer runs
                self.assertEqual(e["layers"], {}, n)
                self.assertEqual(e["named"], "toolchain.systems", n)
            else:
                self.assertIn("source", e["layers"], n)
                # exit 0 means no layer failed
                self.assertEqual(e["exit"] == 0, "fail" not in e["layers"].values(), n)

    def test_gate_one_cases_are_scratch_cases(self):
        for n in NAMES:
            if n.startswith("src-"):
                self.assertTrue((CORPUS / n / "setup.sh").is_file(), n)
                self.assertTrue(json.loads((CORPUS / n / "expected.json").read_text()).get("scratch"), n)

    def test_facets_are_launch_facets_and_systems_are_valid(self):
        for n in NAMES:
            text = (CORPUS / n / "nix-tops.toml").read_text()
            facets = set(re.findall(r'"([a-z-]+)"', re.search(r"facets = \[(.*?)\]", text).group(1)))
            self.assertTrue(facets <= FACETS and len(facets) == 1, n)
            self.assertNotIn("x86_64-darwin", text, n)
            self.assertIn('enforce = "hard"', text, n)

    def test_nixpkgs_rev_is_the_kits(self):
        for n in NAMES:
            f = CORPUS / n / "flake.nix"
            for rev in re.findall(r"github:NixOS/nixpkgs/([0-9a-f]{40})", f.read_text()):
                self.assertEqual(rev, KIT_REV, n)
            lock = CORPUS / n / "flake.lock"
            if lock.is_file():
                data = json.loads(lock.read_text())
                self.assertEqual(data["nodes"]["nixpkgs"]["locked"]["rev"], KIT_REV, n)

    def test_configs_validate(self):
        cmd = sc.check_jsonschema_cmd()
        if cmd is None:
            self.skipTest("check-jsonschema is not installed")
        for n in NAMES:
            r = sc.validate(cmd, REPO / sc.SCHEMA, CORPUS / n / "nix-tops.toml")
            self.assertEqual(r.returncode, 0, f"{n}: {r.stdout}{r.stderr}")
