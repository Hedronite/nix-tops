"""Structure checks for the fixtures and trial workflows and for scripts/trial.sh."""

import re
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent.parent
WORKFLOWS = REPO / ".github" / "workflows"
TRIAL = (REPO / "scripts" / "trial.sh").read_text(encoding="utf-8")
REFS = (REPO / "docs" / "evidence" / "2026-11-16-gate3-trial" / "refs.txt").read_text(encoding="utf-8").split("\n")


class Workflows(unittest.TestCase):
    def test_every_action_is_pinned_by_a_full_commit(self):
        for name in ("fixtures.yml", "trial.yml"):
            uses = re.findall(r"^\s*-?\s*uses:\s*(\S+)", (WORKFLOWS / name).read_text(encoding="utf-8"), re.M)
            self.assertTrue(uses, name)
            for u in uses:
                self.assertRegex(u, r"@[0-9a-f]{40}$", f"{name}: {u}")

    def test_no_privileged_trigger_and_read_only_token(self):
        for name in ("fixtures.yml", "trial.yml"):
            text = (WORKFLOWS / name).read_text(encoding="utf-8")
            self.assertNotRegex(text, r"(?m)^\s*pull_request_target\s*:", name)
            self.assertIn("contents: read", text, name)
            self.assertIn("persist-credentials: false", text, name)

    def test_fixtures_runs_on_pull_requests_and_trial_only_for_its_own_files(self):
        fx = (WORKFLOWS / "fixtures.yml").read_text(encoding="utf-8")
        tr = (WORKFLOWS / "trial.yml").read_text(encoding="utf-8")
        self.assertRegex(fx, r"(?m)^  pull_request:")
        self.assertRegex(tr, r"(?m)^  pull_request:\n    paths:\n      - \.github/workflows/trial\.yml\n      - scripts/trial\.sh\n  schedule:")
        self.assertEqual(len(re.findall(r"(?m)^    paths:", tr)), 1)
        self.assertNotIn("pull_request_target", tr)
        self.assertRegex(tr, r"(?m)^permissions:\n  contents: read\n")
        self.assertRegex(tr, r"(?m)^  schedule:")
        self.assertRegex(tr, r"(?m)^  workflow_dispatch:")

    def test_fixtures_runs_the_gate_on_both_implementations_without_ignoring_failure(self):
        fx = (WORKFLOWS / "fixtures.yml").read_text(encoding="utf-8")
        self.assertEqual(fx.count("scripts/fixtures.sh gate"), 2)
        self.assertNotIn("continue-on-error", fx)
        self.assertIn("determinate-nix-action", fx)

    def test_no_impure_and_no_build_of_the_trial_flakes(self):
        def code(text):
            return "\n".join(l for l in text.splitlines() if not l.lstrip().startswith("#"))

        for name in ("fixtures.yml", "trial.yml"):
            self.assertNotIn("--impure", code((WORKFLOWS / name).read_text(encoding="utf-8")), name)
        self.assertNotIn("--impure", code(TRIAL))
        self.assertNotRegex(code(TRIAL), r"nix (build|run)\b")


class Trial(unittest.TestCase):
    def test_every_ref_has_an_expected_row_and_back(self):
        refs = [l.split()[0].split("/")[1] for l in REFS if l.strip()]
        block = TRIAL[TRIAL.index("EXPECTED = {"):]
        block = block[: block.index("\n}\n")]
        expected = re.findall(r'^\s+"([^"]+)":\s+\{"exit"', block, re.M)
        self.assertEqual(sorted(refs), sorted(expected))
        self.assertEqual(len(refs), 8)

    def test_refs_are_full_commits(self):
        for l in REFS:
            if l.strip():
                self.assertRegex(l.split()[1], r"^[0-9a-f]{40}$", l)

    def test_the_trial_switches_the_five_layers_off_with_its_reason(self):
        self.assertIn('"trial: eval only"', TRIAL)
        for k in ("lint", "build-test", "coverage", "facet-layers", "laws"):
            self.assertIn(f'"{k}"', TRIAL)

    def test_known_differences_carry_a_cause(self):
        block = TRIAL[TRIAL.index("KNOWN = {"):]
        block = block[: block.index("\n}\n")]
        self.assertEqual(block.count('"cause"'), len(re.findall(r'^    "[^"]+": \{', block, re.M)))


if __name__ == "__main__":
    unittest.main()
