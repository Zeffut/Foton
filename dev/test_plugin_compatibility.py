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

FIXTURE_REPORT = {
    "discovered": 4,
    "loaded": 3,
    "enabled": 3,
    "rejected": 1,
    "rejections": [
        {"fixture": "Rejected.jar", "reason": "unsupported custom loader"},
    ],
}


class CompatibilityReport(unittest.TestCase):
    def test_combined_report_keeps_evidence_kinds_separate(self):
        report = plugin_compatibility.combine(API_REPORT, FIXTURE_REPORT)

        self.assertEqual(2478, report["binary"]["shared_members"]["resolved"])
        self.assertEqual(3, report["fixtures"]["enabled"])
        self.assertEqual(1, report["fixtures"]["rejected"])
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
        self.assertEqual(json.loads(first)["fixtures"]["rejections"],
                         FIXTURE_REPORT["rejections"])


if __name__ == "__main__":
    unittest.main()
