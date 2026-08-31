#!/usr/bin/env python3
"""Checks on the shared config-schema walker."""
import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import config_schema as cs


ROOT = {
    "definitions": {"Domain": {"type": "object", "properties": {"a": {"type": "integer"}}}},
    "type": "object",
    "properties": {
        "seed": {"type": "integer", "minimum": 0, "maximum": 9, "default": 3},
        "mode": {"enum": ["peaceful", "hard"]},
        "domain": {"$ref": "#/definitions/Domain"},
        "nested": {"type": "object", "properties": {"x": {"type": "string"}}},
        "list": {"type": "array", "items": {"type": "string"}, "minItems": 2},
    },
    "required": ["seed"],
}


# Mirrors the real "label" property in config.schema.json: a oneOf where one
# member is an enum (unhashable as a type_parts tuple) and the other is not.
LABEL_LIKE_UNION = {
    "oneOf": [
        {"type": "string", "enum": ["bug_report", "status", "feedback"]},
        {"type": "object", "properties": {"key": {"type": "string"}}},
    ],
}

# Two members equal to each other -- including two identical enum parts,
# which dict.fromkeys cannot hash -- plus one genuinely distinct member.
DEDUP_UNION = {
    "anyOf": [
        {"enum": ["auto", "manual"]},
        {"enum": ["auto", "manual"]},
        {"type": "integer"},
    ],
}


class TypeParts(unittest.TestCase):
    def test_a_ref_reports_the_definition_name(self):
        self.assertEqual(cs.type_parts(ROOT["properties"]["domain"], ROOT), ("ref", "Domain"))

    def test_an_enum_reports_its_values(self):
        self.assertEqual(cs.type_parts(ROOT["properties"]["mode"], ROOT),
                         ("enum", ["peaceful", "hard"]))

    def test_an_array_reports_its_item_type(self):
        self.assertEqual(cs.type_parts(ROOT["properties"]["list"], ROOT),
                         ("array", ("scalar", "string")))

    def test_a_plain_type_reports_itself(self):
        self.assertEqual(cs.type_parts(ROOT["properties"]["seed"], ROOT),
                         ("scalar", "integer"))


class UnionDedup(unittest.TestCase):
    def test_a_union_member_that_is_an_enum_does_not_raise_and_keeps_its_parts(self):
        self.assertEqual(
            cs.type_parts(LABEL_LIKE_UNION, LABEL_LIKE_UNION),
            ("union", [
                ("enum", ["bug_report", "status", "feedback"]),
                ("object", None),
            ]),
        )

    def test_equal_union_members_collapse_to_one_including_enums(self):
        self.assertEqual(
            cs.type_parts(DEDUP_UNION, DEDUP_UNION),
            ("union", [("enum", ["auto", "manual"]), ("scalar", "integer")]),
        )


class Limits(unittest.TestCase):
    def test_a_bounded_number_reports_its_range(self):
        self.assertEqual(cs.limits(ROOT["properties"]["seed"], ROOT), "0–9")

    def test_min_items_is_reported(self):
        self.assertEqual(cs.limits(ROOT["properties"]["list"], ROOT), "≥ 2 item(s)")


class Traversal(unittest.TestCase):
    def test_object_properties_become_their_own_section(self):
        self.assertEqual([n for n, _ in cs.subsections(ROOT, ROOT)], ["nested"])

    def test_and_are_left_out_of_the_table(self):
        self.assertNotIn("nested", [n for n, _, _ in cs.rows(ROOT, ROOT)])

    def test_required_is_reported_per_row(self):
        self.assertEqual({n: req for n, _, req in cs.rows(ROOT, ROOT)}["seed"], True)

    def test_a_ref_property_stays_in_the_table(self):
        self.assertIn("domain", [n for n, _, _ in cs.rows(ROOT, ROOT)])


class Defaults(unittest.TestCase):
    def test_a_present_default_is_returned_raw(self):
        self.assertEqual(cs.default_of(ROOT["properties"]["seed"], ROOT), 3)

    def test_an_absent_default_is_none(self):
        self.assertIsNone(cs.default_of(ROOT["properties"]["mode"], ROOT))


if __name__ == "__main__":
    unittest.main()
