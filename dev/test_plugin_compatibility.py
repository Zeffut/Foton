#!/usr/bin/env python3
"""Checks for the combined plugin compatibility evidence."""

import json
import pathlib
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import plugin_compatibility


API_REPORT = {
    "api": {
        "all": {"referenced": 6345, "resolved": None, "missing": None},
        "classes": {
            "all": {"referenced": None, "resolved": None, "missing": None},
            "shared": {"referenced": None, "resolved": None, "missing": None},
        },
        "shared": {"referenced": 2487, "resolved": 2478, "missing": 9},
    },
    "ceiling": {
        "plugins_on_public_api": 41,
        "plugins_reaching_internals": 18,
        "plugins_scanned": 59,
    },
    "events": {"listened": 199, "emitted": 87, "missing": 112},
}

REQUIRED_SUITES = {
    "event", "lifecycle", "replacement", "dependency", "cross-call-alias",
    "library", "malformed-library", "paper",
}


def fixture_outcome(suite, fixture, loaded=True, enabled=True, reason=None):
    return {
        "discovered": True,
        "enabled": enabled,
        "fixture": fixture,
        "loaded": loaded,
        "phase": "enabled" if enabled else ("enable" if loaded else "load"),
        "reason": reason,
        "rejected": not enabled,
        "suite": suite,
    }


FIXTURE_SUITES = [
    {
        "suite": suite,
        "discovered": 1,
        "loaded": 1,
        "enabled": 1,
        "rejected": 0,
        "outcomes": [fixture_outcome(suite, f"{suite}.jar")],
    }
    for suite in sorted(REQUIRED_SUITES)
]
FIXTURE_REPORT = {
    "schema_version": 2,
    "aggregate": {
        "discovered": len(FIXTURE_SUITES),
        "loaded": len(FIXTURE_SUITES),
        "enabled": len(FIXTURE_SUITES),
        "rejected": 0,
    },
    "suites": FIXTURE_SUITES,
}


class CompatibilityReport(unittest.TestCase):
    def test_combined_report_keeps_evidence_kinds_separate(self):
        report = plugin_compatibility.combine(API_REPORT, FIXTURE_REPORT)

        self.assertEqual(2478, report["binary"]["shared_members"]["resolved"])
        self.assertEqual(
            len(FIXTURE_SUITES), report["fixtures"]["aggregate"]["enabled"])
        self.assertEqual(0, report["fixtures"]["aggregate"]["rejected"])
        self.assertEqual(
            {"binary", "ceiling", "events", "fixtures"}, set(report))
        self.assertNotIn("plugin_success_percent", report)

    def test_cli_writes_deterministic_json(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            api = root / "api.json"
            fixtures = root / "fixtures.json"
            output = root / "combined.json"
            api.write_text(json.dumps(API_REPORT), encoding="utf-8")
            fixtures.write_text(json.dumps(FIXTURE_REPORT), encoding="utf-8")

            with mock.patch.object(
                sys,
                "argv",
                [
                    "plugin_compatibility.py",
                    "--api-report",
                    str(api),
                    "--fixture-report",
                    str(fixtures),
                    "--output",
                    str(output),
                ],
            ):
                plugin_compatibility.main()

            first = output.read_text(encoding="utf-8")
            plugin_compatibility.main(
                [
                    "--api-report",
                    str(api),
                    "--fixture-report",
                    str(fixtures),
                    "--output",
                    str(output),
                ]
            )
            second = output.read_text(encoding="utf-8")

        self.assertEqual(first, second)
        self.assertTrue(first.endswith("\n"))
        self.assertEqual(
            json.loads(first)["fixtures"]["suites"], FIXTURE_REPORT["suites"])

    def test_combiner_rejects_incomplete_fixture_corpus(self):
        incomplete = dict(FIXTURE_REPORT)
        incomplete["suites"] = FIXTURE_REPORT["suites"][:-1]

        with self.assertRaisesRegex(ValueError, "missing fixture suites"):
            plugin_compatibility.combine(API_REPORT, incomplete)

    def test_combiner_rejects_inconsistent_or_noncausal_outcomes(self):
        invalid = json.loads(json.dumps(FIXTURE_REPORT))
        invalid["suites"][0]["outcomes"][0] = fixture_outcome(
            invalid["suites"][0]["suite"], "decoy.jar",
            loaded=False, enabled=False, reason=None)

        with self.assertRaisesRegex(ValueError, "causal reason"):
            plugin_compatibility.combine(API_REPORT, invalid)


if __name__ == "__main__":
    unittest.main()
