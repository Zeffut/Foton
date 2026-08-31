#!/usr/bin/env python3
"""The site may only state facts that come from the repository."""
import pathlib
import re
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import facts


class Facts(unittest.TestCase):
    def setUp(self):
        self.f = facts.all()

    def test_version_matches_cargo_toml(self):
        cargo = (pathlib.Path(facts.REPO) / "Cargo.toml").read_text(encoding="utf-8")
        declared = re.search(r'^version = "([^"]+)"', cargo, re.M).group(1)
        self.assertEqual(self.f["version"], declared)

    def test_minecraft_target_comes_out_of_the_version(self):
        self.assertEqual(self.f["mc_target"], self.f["version"].split("+mc")[1])

    def test_protocol_is_the_extracted_number(self):
        self.assertRegex(self.f["protocol"], r"^\d+$")

    def test_coverage_percentages_are_whole_numbers(self):
        for key in ("blocks_percent", "items_percent", "entities_percent"):
            self.assertRegex(self.f[key], r"^\d+$")

    def test_covered_never_exceeds_total(self):
        for kind in ("blocks", "items", "entities"):
            self.assertLessEqual(int(self.f[f"{kind}_covered"]), int(self.f[f"{kind}_total"]))

    def test_missing_blocks_are_named_not_counted(self):
        self.assertIn("Block", self.f["blocks_missing_list"])

    def test_in_world_script_count_matches_the_directory(self):
        found = len(list((pathlib.Path(facts.REPO) / "dev").glob("*-test.sh")))
        self.assertEqual(int(self.f["inworld_scripts"]), found)

    def test_every_value_is_a_string(self):
        for key, value in self.f.items():
            self.assertIsInstance(value, str, key)

    def test_no_value_is_empty(self):
        for key, value in self.f.items():
            self.assertTrue(value.strip(), key)


if __name__ == "__main__":
    unittest.main()
