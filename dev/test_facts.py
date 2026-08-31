#!/usr/bin/env python3
"""The site may only state facts that come from the repository."""
import json
import pathlib
import re
import sys
import tempfile
import unittest
from unittest import mock

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

    def test_zero_total_section_does_not_divide_by_zero(self):
        """A section reporting no classes at all (e.g. a registry section that
        stops existing) must not crash all()'s percentage -- the max(total, 1)
        guard in facts.all() exists precisely for this."""
        empty = {"covered": 0, "total": 0, "missing": []}
        fake = {"blocks": empty, "items": empty, "entities": empty}
        with mock.patch.object(facts.coverage, "counts", return_value=fake):
            f = facts.all()
        for kind in ("blocks", "items", "entities"):
            self.assertEqual(f[f"{kind}_percent"], "0")

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


class MissingList(unittest.TestCase):
    """Pins the "or 'none'" guard directly -- none of blocks/items/entities is
    at 100% coverage today, so the committed suite never hits it through
    facts.all() alone."""

    def test_empty_list_reads_as_none(self):
        self.assertEqual(facts._missing_list([]), "none")

    def test_single_missing_name(self):
        self.assertEqual(facts._missing_list(["Item"]), "Item")

    def test_several_missing_names_are_comma_joined(self):
        self.assertEqual(facts._missing_list(["A", "B", "C"]), "A, B, C")


class MalformedSources(unittest.TestCase):
    """A source file that exists but is truncated, empty or has had a key
    renamed must fail the build with a clear SystemExit, not a raw
    JSONDecodeError/KeyError traceback -- this is what a fresh, incomplete
    Vercel checkout would look like mid-build."""

    def test_protocol_rejects_truncated_json(self):
        with tempfile.TemporaryDirectory() as tmp:
            repo = pathlib.Path(tmp)
            packets_dir = repo / "foton-registry" / "build_assets"
            packets_dir.mkdir(parents=True)
            (packets_dir / "packets.json").write_text("{not json", encoding="utf-8")
            with mock.patch.object(facts, "REPO", repo):
                self.assertRaises(SystemExit, facts.protocol)

    def test_protocol_rejects_renamed_version_key(self):
        with tempfile.TemporaryDirectory() as tmp:
            repo = pathlib.Path(tmp)
            packets_dir = repo / "foton-registry" / "build_assets"
            packets_dir.mkdir(parents=True)
            (packets_dir / "packets.json").write_text(
                json.dumps({"proto_version": 776}), encoding="utf-8")
            with mock.patch.object(facts, "REPO", repo):
                self.assertRaises(SystemExit, facts.protocol)

    def test_test_counts_rejects_truncated_json(self):
        with tempfile.TemporaryDirectory() as tmp:
            dev = pathlib.Path(tmp)
            (dev / "test-counts.json").write_text("{not json", encoding="utf-8")
            with mock.patch.object(facts, "DEV", dev):
                self.assertRaises(SystemExit, facts.test_counts)

    def test_test_counts_rejects_missing_key(self):
        with tempfile.TemporaryDirectory() as tmp:
            dev = pathlib.Path(tmp)
            (dev / "test-counts.json").write_text(
                json.dumps({"unit_tests": 5}), encoding="utf-8")
            with mock.patch.object(facts, "DEV", dev):
                self.assertRaises(SystemExit, facts.test_counts)

    def test_coverage_counts_rejects_truncated_json(self):
        with tempfile.TemporaryDirectory() as tmp:
            classes_path = pathlib.Path(tmp) / "classes.json"
            classes_path.write_text("{not json", encoding="utf-8")
            with mock.patch.object(facts.coverage, "CLASSES", classes_path):
                self.assertRaises(SystemExit, facts.coverage.counts)

    def test_coverage_counts_rejects_renamed_section(self):
        with tempfile.TemporaryDirectory() as tmp:
            classes_path = pathlib.Path(tmp) / "classes.json"
            classes_path.write_text(
                json.dumps({"blocks": [], "items": []}), encoding="utf-8")
            with mock.patch.object(facts.coverage, "CLASSES", classes_path):
                self.assertRaises(SystemExit, facts.coverage.counts)


if __name__ == "__main__":
    unittest.main()
