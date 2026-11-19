"""Tests for scripts/layers_ran.py: the clean pair, and each failure planted once."""

import copy
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
import layers_ran  # noqa: E402

OFF = {name: {"enabled": False, "reason": "not built in M1"} for name in layers_ran.SWITCHABLE}
CONFIG = {"layers": OFF}


def report():
    rows = []
    for name in layers_ran.LAYERS:
        if name in OFF:
            rows.append({"id": name, "verdict": "skip", "reason": "not built in M1"})
        else:
            rows.append({"id": name, "verdict": "pass"})
    return {"layers": rows}


def without(rep, name):
    out = copy.deepcopy(rep)
    out["layers"] = [row for row in out["layers"] if row["id"] != name]
    return out


def with_verdict(rep, name, verdict, **extra):
    out = copy.deepcopy(rep)
    for row in out["layers"]:
        if row["id"] == name:
            row["verdict"] = verdict
            row.update(extra)
    return out


class Problems(unittest.TestCase):
    def test_the_clean_pair_passes(self):
        self.assertEqual(layers_ran.problems(CONFIG, report()), [])

    def test_a_layer_missing_from_the_report_fails(self):
        found = layers_ran.problems(CONFIG, without(report(), "eval"))
        self.assertEqual(found, ["layer eval is missing from the report"])

    def test_a_missing_switched_off_layer_fails_too(self):
        found = layers_ran.problems(CONFIG, without(report(), "lint"))
        self.assertEqual(found, ["layer lint is missing from the report"])

    def test_an_enabled_layer_that_did_not_run_fails(self):
        for verdict in layers_ran.NOT_RAN:
            found = layers_ran.problems(CONFIG, with_verdict(report(), "eval", verdict))
            self.assertEqual(found, [f"layer eval is enabled but did not run: {verdict}"])

    def test_the_plant_a_layer_enabled_and_not_built(self):
        config = {"layers": {k: v for k, v in OFF.items() if k != "lint"}}
        rep = with_verdict(report(), "lint", "not-run", reason="not built")
        found = layers_ran.problems(config, rep)
        self.assertEqual(found, ["layer lint is enabled but did not run: not-run"])

    def test_a_switched_off_layer_must_show_skip_with_the_reason(self):
        found = layers_ran.problems(CONFIG, with_verdict(report(), "laws", "pass"))
        self.assertEqual(found, ["layer laws is switched off but its verdict is pass"])
        found = layers_ran.problems(CONFIG, with_verdict(report(), "laws", "skip", reason="other"))
        self.assertEqual(found, ["layer laws is switched off with a reason the report does not show"])

    def test_an_unknown_or_repeated_layer_fails(self):
        rep = report()
        rep["layers"].append({"id": "bogus", "verdict": "pass"})
        rep["layers"].append({"id": "eval", "verdict": "pass"})
        found = layers_ran.problems(CONFIG, rep)
        self.assertEqual(
            found,
            ["layer bogus is not a layer of the report schema", "layer eval is in the report twice"],
        )

    def test_a_pass_refuse_ack_or_vacuous_verdict_counts_as_run(self):
        for verdict in ("pass", "Refuse", "Ack", "vacuous"):
            rep = with_verdict(report(), "eval", verdict)
            self.assertEqual(layers_ran.problems(CONFIG, rep), [], verdict)

    def test_switched_off_reads_only_enabled_false(self):
        config = {"layers": {"lint": {"enabled": False, "reason": "r"}, "custom": [], "rebuild": {"enabled": False}}}
        self.assertEqual(layers_ran.switched_off(config), {"lint": "r"})
        self.assertEqual(layers_ran.switched_off({}), {})


class Main(unittest.TestCase):
    TOML = "".join(
        f'[layers.{name}]\nenabled = false\nreason = "not built in M1"\n' for name in layers_ran.SWITCHABLE
    )

    def run_main(self, config_text, rep):
        with tempfile.TemporaryDirectory() as tmp:
            config = Path(tmp) / "nix-tops.toml"
            if config_text is not None:
                config.write_text(config_text)
            path = Path(tmp) / "report.json"
            path.write_text(rep if isinstance(rep, str) else json.dumps(rep))
            return layers_ran.main(["--config", str(config), "--report", str(path)])

    def test_exit_codes(self):
        self.assertEqual(self.run_main(self.TOML, report()), 0)
        self.assertEqual(self.run_main(self.TOML, without(report(), "eval")), 1)
        with self.assertRaises(SystemExit) as caught:
            self.run_main(self.TOML, "not json")
        self.assertEqual(caught.exception.code, 3)
        with self.assertRaises(SystemExit) as caught:
            self.run_main(None, report())
        self.assertEqual(caught.exception.code, 3)


if __name__ == "__main__":
    unittest.main()
