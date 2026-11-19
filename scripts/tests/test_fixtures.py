"""Structure checks for fixtures/corpus/*: every flake has what the runner and the gate need."""

import json
import os
import re
import subprocess
import sys
import tempfile
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
VERDICTS = {"pass", "Refuse", "Ack", "skip", "not-run", "not-evaluated", "deferred", "vacuous"}
OFF = ["lint", "build-test", "coverage", "facet-layers", "laws"]
EXIT2_NAMES = {"toolchain.systems", "facets[0]"}
RUNNER = REPO / "scripts" / "fixtures.sh"
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
            # N-05: every flake carries a committed lock, even one with no inputs. The two cases about
            # the lock are the exception: one has none, and the other builds its stale lock in setup.sh
            lockless = n in ("src-lock-missing", "src-lock-stale")
            self.assertEqual((d / "flake.lock").is_file(), not lockless, f"{n}: flake.lock")

    def test_layers_block_switches_the_five_off_and_leaves_gates_one_and_three(self):
        for n in NAMES:
            text = (CORPUS / n / "nix-tops.toml").read_text()
            self.assertIn("[layers]", text, n)
            for k in OFF:
                self.assertRegex(text, rf'(?m)^{k} = \{{ enabled = false, reason = "[^"]+" \}}$', f"{n}: {k}")
            self.assertNotRegex(text, r"(?m)^(source|eval) =", n)

    def test_expected_json_shape(self):
        for n in NAMES:
            e = json.loads((CORPUS / n / "expected.json").read_text())
            self.assertIn(e["exit"], (0, 1, 2), n)
            # every layer is present (section 6.7), with the verdict the gate reports
            self.assertEqual(set(e["layers"]), LAYERS, n)
            self.assertTrue(set(e["layers"].values()) <= VERDICTS, n)
            self.assertIn(e["raw"]["exit"], (0, 1), n)
            self.assertTrue(e["note"], n)
            if e["exit"] == 2:
                # an invalid configuration stops the gate before any layer runs: every layer is not-run
                self.assertEqual(set(e["layers"].values()), {"not-run"}, n)
                self.assertIn(e["named"], EXIT2_NAMES, n)
            else:
                for k in OFF:
                    self.assertEqual(e["layers"][k], "skip", f"{n}: {k}")
                self.assertIn(e["layers"]["source"], ("pass", "Refuse"), n)
                self.assertEqual(e["exit"] == 0, "Refuse" not in e["layers"].values(), n)
                if e["layers"]["source"] == "Refuse":
                    # gate 1 stops the run; gate 3 is not-run
                    self.assertEqual(e["layers"]["eval"], "not-run", n)

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
                node = json.loads(lock.read_text())["nodes"].get("nixpkgs")
                if node:
                    self.assertEqual(node["locked"]["rev"], KIT_REV, n)

    def test_configs_validate(self):
        cmd = sc.check_jsonschema_cmd()
        if cmd is None:
            self.skipTest("check-jsonschema is not installed")
        for n in NAMES:
            r = sc.validate(cmd, REPO / sc.SCHEMA, CORPUS / n / "nix-tops.toml")
            self.assertEqual(r.returncode, 0, f"{n}: {r.stdout}{r.stderr}")


STUB = """#!/bin/sh
# A stand-in for `nixtops gate`: prints STUB_REPORT and exits with STUB_EXIT.
cat "$STUB_REPORT"
exit "$STUB_EXIT"
"""


class GateRunner(unittest.TestCase):
    """scripts/fixtures.sh gate compares the exit, each layer's verdict and `named` with expected.json."""

    def run_gate(self, name, mutate=None):
        e = json.loads((CORPUS / name / "expected.json").read_text())
        layers = dict(e["layers"])
        if mutate:
            mutate(layers)
        reason = (e.get("named") or "").split("|")[0]
        report = {"layers": [{"id": k, "verdict": v, "reason": reason} for k, v in layers.items()]}
        with tempfile.TemporaryDirectory() as tmp:
            stub = Path(tmp) / "nixtops"
            stub.write_text(STUB)
            stub.chmod(0o755)
            rep = Path(tmp) / "report.json"
            rep.write_text(json.dumps(report))
            env = dict(os.environ, NIXTOPS=str(stub), STUB_REPORT=str(rep), STUB_EXIT=str(e["exit"]))
            r = subprocess.run(["bash", str(RUNNER), "gate", name], env=env, capture_output=True, text=True)
        return r

    def test_a_matching_report_passes(self):
        for n in ["ctl-dev-flake", "src-lock-missing", "eval-one-system-throws", "eval-zero-outputs"]:
            r = self.run_gate(n)
            self.assertEqual(r.returncode, 0, n + r.stdout + r.stderr)
            self.assertIn("ok", r.stdout)

    def test_a_changed_verdict_fails_and_names_the_layer(self):
        r = self.run_gate("ctl-dev-flake", lambda l: l.update(eval="Refuse"))
        self.assertEqual(r.returncode, 1, r.stdout)
        self.assertIn("DIFF", r.stdout)
        self.assertIn("eval Refuse, expected pass", r.stdout)

    def test_a_switched_off_layer_that_is_not_skip_fails(self):
        r = self.run_gate("ctl-dev-flake", lambda l: l.update(laws="pass"))
        self.assertEqual(r.returncode, 1, r.stdout)
        self.assertIn("laws pass, expected skip", r.stdout)

    def test_a_missing_name_fails(self):
        e = json.loads((CORPUS / "src-lock-missing" / "expected.json").read_text())
        with tempfile.TemporaryDirectory() as tmp:
            stub = Path(tmp) / "nixtops"
            stub.write_text(STUB)
            stub.chmod(0o755)
            rep = Path(tmp) / "report.json"
            rep.write_text(json.dumps({"layers": [{"id": k, "verdict": v} for k, v in e["layers"].items()]}))
            env = dict(os.environ, NIXTOPS=str(stub), STUB_REPORT=str(rep), STUB_EXIT="1")
            r = subprocess.run(["bash", str(RUNNER), "gate", "src-lock-missing"], env=env, capture_output=True, text=True)
        self.assertEqual(r.returncode, 1, r.stdout)
        self.assertIn("does not name 'N-05'", r.stdout)

    def test_a_changed_exit_fails(self):
        e = json.loads((CORPUS / "ctl-dev-flake" / "expected.json").read_text())
        with tempfile.TemporaryDirectory() as tmp:
            stub = Path(tmp) / "nixtops"
            stub.write_text(STUB)
            stub.chmod(0o755)
            rep = Path(tmp) / "report.json"
            rep.write_text(json.dumps({"layers": [{"id": k, "verdict": v} for k, v in e["layers"].items()]}))
            env = dict(os.environ, NIXTOPS=str(stub), STUB_REPORT=str(rep), STUB_EXIT="1")
            r = subprocess.run(["bash", str(RUNNER), "gate", "ctl-dev-flake"], env=env, capture_output=True, text=True)
        self.assertEqual(r.returncode, 1, r.stdout)
        self.assertIn("exit 1, expected 0", r.stdout)


if __name__ == "__main__":
    unittest.main()
