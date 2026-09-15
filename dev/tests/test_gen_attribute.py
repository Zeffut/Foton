#!/usr/bin/env python3
"""Regression tests for the Bukkit Attribute source generator."""

from __future__ import annotations

import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]


class AttributeGeneratorTests(unittest.TestCase):
    def test_output_order_and_metadata_follow_the_asset(self) -> None:
        attributes = json.loads(
            (ROOT / "foton-registry/build_assets/attributes.json").read_text(
                encoding="utf-8"
            )
        )
        self.assertFalse(
            any("sentiment" in attribute for attribute in attributes),
            "Paper compatibility metadata must stay outside the extracted asset",
        )

        armor = next(attribute for attribute in attributes if attribute["name"] == "armor")
        air_drag = next(
            attribute for attribute in attributes if attribute["name"] == "air_drag_modifier"
        )
        armor["id"], air_drag["id"] = air_drag["id"], armor["id"]
        armor["translation_key"] = "fixture.attribute.armor"
        armor["default_value"] = 17.25

        with tempfile.TemporaryDirectory(prefix="foton attribute generator ") as temporary:
            root = Path(temporary)
            asset = root / "fixture attributes.json"
            output = root / "generated output"
            asset.write_text(json.dumps(attributes), encoding="utf-8")

            result = subprocess.run(
                [sys.executable, str(ROOT / "dev/gen-attribute.py"), str(asset), str(output)],
                capture_output=True,
                text=True,
            )

            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            generated = (
                output / "org/bukkit/attribute/Attribute.java"
            ).read_text(encoding="utf-8")
            self.assertLess(
                generated.index("static final Attribute ARMOR = value("),
                generated.index("static final Attribute AIR_DRAG_MODIFIER = value("),
            )
            self.assertIn(
                '"armor", "fixture.attribute.armor", 0, 17.25, '
                "Attribute.Sentiment.POSITIVE",
                generated,
            )
            self.assertIn(
                '"burning_time", "attribute.name.burning_time", 10, 1.0, '
                "Attribute.Sentiment.NEGATIVE",
                generated,
            )
            self.assertIn(
                '"gravity", "attribute.name.gravity", 18, 0.08, '
                "Attribute.Sentiment.NEUTRAL",
                generated,
            )


if __name__ == "__main__":
    unittest.main()
