"""Tests for schema/gate-report.schema.json and schema/core/: valid reports pass, planted defects fail."""

import copy
import json
import re
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent
sys.path.insert(0, str(HERE.parent))
import spec_check as sc  # noqa: E402

CMD = sc.check_jsonschema_cmd()
SCHEMA = REPO / "schema" / "gate-report.schema.json"
CORE = json.loads((REPO / "schema" / "core" / "core.schema.json").read_text(encoding="utf-8"))

SAMPLE = {
    "protocol": "0.2.0", "enforce": "ratchet", "passed": False,
    "baseline_identity": {"implementation": "cppnix", "version": "2.35.2", "system": "x86_64-linux", "lock_hash": "sha256-abc"},
    "facets": ["package-set"],
    "counts": {"verdicts": {"pass": 3, "refuse": 0, "ack": 1, "skip": 1, "not_run": 0, "not_evaluated": 1, "deferred": 1, "vacuous": 0}, "failed": 2},
    "layers": [
        {"id": "source", "verdict": "pass"}, {"id": "lint", "verdict": "pass"}, {"id": "eval", "verdict": "pass"},
        {"id": "build-test", "verdict": "deferred", "reason": "a configuration cannot build on this host"},
        {"id": "coverage", "verdict": "pass"}, {"id": "facet-layers", "verdict": "skip", "reason": "none enabled"},
        {"id": "laws", "verdict": "not-evaluated", "reason": "reader missing"},
    ],
    "laws": [
        {"id": "N-12", "verdict": "not-evaluated", "reader": "an", "required": True, "reason": "statix off"},
        {"id": "N-21", "verdict": "Ack", "reader": "ev", "required": True,
         "ack": {"kind": "source", "law": "N-21", "reason": "overrides an upstream default"}},
    ],
    "outputs": [{"attr": "packages.x86_64-linux.foo", "level": "tested", "ifd_reevaluated": False,
                 "closure": {"bytes": 1234, "delta_bytes": -5, "new_paths": [], "duplicate_packages": []},
                 "eval_cost": {"function_calls": 10, "thunks": 20, "normalized": 1.02}}],
    "warnings": [{"source": "config-warnings", "message": "option renamed", "verdict": "Ack",
                  "entry": {"kind": "ledger", "match": "renamed", "reason": "upstream", "until": "2026-12-31"}}],
    "acks": {"count": 2, "entries": [{"kind": "source", "law": "N-21", "reason": "overrides an upstream default"}]},
}


def spec_verdicts():
    """The verdict names in the section 3.3 table, in the spec's own words."""
    lines = (REPO / "NIX_TOPS.md").read_text(encoding="utf-8").splitlines()
    start = next(i for i, l in enumerate(lines) if l.startswith("### 3.3"))
    out = []
    for l in lines[start:]:
        if l.startswith("### 3.4"):
            break
        m = re.match(r"^\| `([A-Za-z-]+)` \|", l)
        if m:
            out.append(m.group(1))
        elif out and not l.startswith("|"):
            break  # only the first table: the verdict table, not the family-terms table
    return out


@unittest.skipUnless(CMD, "check-jsonschema is not installed")
class GateReport(unittest.TestCase):
    def run_schema(self, doc):
        with tempfile.TemporaryDirectory() as tmp:
            f = Path(tmp) / "report.json"
            f.write_text(json.dumps(doc), encoding="utf-8")
            return sc.validate(CMD, SCHEMA, f)

    def ok(self, doc):
        r = self.run_schema(doc)
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)

    def bad(self, doc, needle=""):
        r = self.run_schema(doc)
        self.assertNotEqual(r.returncode, 0, "expected a validation failure")
        self.assertIn(needle, r.stdout + r.stderr)

    def sample(self):
        return copy.deepcopy(SAMPLE)

    def test_sample_validates(self):
        self.ok(self.sample())

    def test_every_verdict_is_accepted_on_a_layer(self):
        for v in spec_verdicts():
            d = self.sample()
            d["layers"][2] = {"id": "eval", "verdict": v, "reason": "because"}
            self.ok(d)

    def test_core_verdicts_equal_the_spec_table(self):
        self.assertEqual(CORE["$defs"]["verdict"]["enum"], spec_verdicts())
        want = [v.lower().replace("-", "_") for v in spec_verdicts()]
        self.assertEqual(CORE["$defs"]["verdictKey"]["enum"], want)
        self.assertIn("deferred", want)
        self.assertIn("vacuous", want)

    def test_core_modes_equal_the_spec_table(self):
        text = (REPO / "NIX_TOPS.md").read_text(encoding="utf-8")
        sec = text[text.index("### 6.8"):]
        modes = re.findall(r"^\| `(hard|ratchet|advisory)` \|", sec, re.M)
        self.assertEqual(CORE["$defs"]["mode"]["enum"], modes)

    def test_unknown_verdict_fails(self):
        d = self.sample()
        d["layers"][0]["verdict"] = "passed"
        self.bad(d)

    def test_hyphen_key_in_counts_fails(self):
        d = self.sample()
        d["counts"]["verdicts"]["not-evaluated"] = d["counts"]["verdicts"].pop("not_evaluated")
        self.bad(d)

    def test_counts_must_include_deferred_and_vacuous(self):
        for k in ("deferred", "vacuous", "not_run", "not_evaluated"):
            d = self.sample()
            del d["counts"]["verdicts"][k]
            self.bad(d, k)

    def test_a_missing_layer_fails(self):
        d = self.sample()
        d["layers"] = [l for l in d["layers"] if l["id"] != "lint"]
        self.bad(d)

    def test_skip_needs_a_reason(self):
        d = self.sample()
        del d["layers"][5]["reason"]
        self.bad(d, "reason")

    def test_law_not_evaluated_needs_a_reason(self):
        d = self.sample()
        del d["laws"][0]["reason"]
        self.bad(d, "reason")

    def test_ledger_ack_needs_an_expiry(self):
        d = self.sample()
        del d["warnings"][0]["entry"]["until"]
        self.bad(d, "until")

    def test_ack_needs_a_reason(self):
        d = self.sample()
        d["acks"]["entries"][0]["reason"] = ""
        self.bad(d)

    def test_bad_date_fails(self):
        d = self.sample()
        d["warnings"][0]["entry"]["until"] = "31-12-2026"
        self.bad(d)

    def test_unknown_key_fails(self):
        d = self.sample()
        d["surprise"] = 1
        self.bad(d, "surprise")

    def test_bad_mode_fails(self):
        d = self.sample()
        d["enforce"] = "strict"
        self.bad(d)

    def test_baseline_identity_needs_all_four(self):
        d = self.sample()
        del d["baseline_identity"]["lock_hash"]
        self.bad(d, "lock_hash")


CONFIG_SCHEMA = REPO / "schema" / "nix-tops.schema.json"
OFF = {"enabled": False, "reason": "not built yet"}
LAYER_IDS = ["lint", "build-test", "coverage", "facet-layers", "laws"]


class ConfigLayers(unittest.TestCase):
    """[layers] keys that switch a report layer off (section 9)."""

    def run_schema(self, layers):
        doc = {"protocol": "0.2.0", "facets": ["dev-flake"], "toolchain": {"systems": ["x86_64-linux"]},
               "layers": layers}
        with tempfile.TemporaryDirectory() as tmp:
            f = Path(tmp) / "nix-tops.json"
            f.write_text(json.dumps(doc), encoding="utf-8")
            return sc.validate(CMD, CONFIG_SCHEMA, f)

    def ok(self, layers):
        r = self.run_schema(layers)
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)

    def bad(self, layers, needle=""):
        r = self.run_schema(layers)
        self.assertNotEqual(r.returncode, 0, "expected a validation failure")
        self.assertIn(needle, r.stdout + r.stderr)

    def test_each_layer_id_switches_off_with_a_reason(self):
        for k in LAYER_IDS:
            self.ok({k: dict(OFF)})
        self.ok({k: dict(OFF) for k in LAYER_IDS})

    def test_reason_is_required(self):
        for k in LAYER_IDS:
            self.bad({k: {"enabled": False}}, "reason")

    def test_empty_reason_fails(self):
        self.bad({"laws": {"enabled": False, "reason": ""}})

    def test_enabled_true_fails(self):
        self.bad({"lint": {"enabled": True, "reason": "x"}})

    def test_enabled_is_required(self):
        self.bad({"lint": {"reason": "x"}}, "enabled")

    def test_tier_is_not_allowed_on_a_switch_off(self):
        self.bad({"coverage": {"enabled": False, "reason": "x", "tier": 1}})

    def test_gates_one_and_three_cannot_be_switched_off(self):
        self.bad({"source": dict(OFF)}, "source")
        self.bad({"eval": dict(OFF)}, "eval")

    def test_a_name_outside_the_five_fails(self):
        self.bad({"lints": dict(OFF)})
        self.bad({"facet_layers": dict(OFF)})

    def test_sublayers_keep_their_old_shape(self):
        self.ok({"rebuild": {"enabled": False}, "vulnix": {"enabled": False}, "vm_tests": {"enabled": True, "tier": 2}})


class OutputLevel(GateReport):
    def level(self, value):
        d = self.sample()
        d["outputs"][0]["level"] = value
        return d

    def test_instantiated_is_a_level(self):
        self.ok(self.level("instantiated"))

    def test_every_defined_level_is_accepted_and_others_fail(self):
        for v in ["evaluated", "instantiated", "built", "tested"]:
            self.ok(self.level(v))
        self.bad(self.level("compiled"))


class Shape(unittest.TestCase):
    def test_both_schemas_use_core_refs(self):
        self.assertIn("core/core.schema.json#", SCHEMA.read_text(encoding="utf-8"))

    def test_core_names_the_sync_rule(self):
        self.assertIn("A tops-core sync replaces this whole directory", CORE["$comment"])


if __name__ == "__main__":
    unittest.main()
