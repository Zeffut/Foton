"""Regression tests for the Paper/Foton observation comparator."""

import unittest

from dev.compat.compare import compare_observations, normalize_observation


class ComparisonTests(unittest.TestCase):
    def test_identical_fixture_passes(self):
        observed = {
            "startup_order": ["PacketEvents", "Observer"],
            "events": [{"type": "PlayerMoveEvent", "cancelled": False}],
            "items": [{"components": {"minecraft:custom_name": "Sword"}}],
        }
        self.assertEqual([], compare_observations(observed, observed, []))

    def test_missing_event_and_changed_component_both_reported(self):
        paper = {
            "events": [{"type": "PlayerMoveEvent"}],
            "items": [{"components": {"minecraft:custom_name": "Sword"}}],
        }
        foton = {
            "events": [],
            "items": [{"components": {"minecraft:custom_name": "Axe"}}],
        }
        differences = compare_observations(paper, foton, [])
        self.assertTrue(any("/events/0" in line and "missing" in line for line in differences))
        self.assertTrue(any("/items/0/components/minecraft:custom_name" in line for line in differences))

    def test_explicit_normalizers_only_change_declared_paths(self):
        rules = [{"path": "/server/port", "kind": "port"}]
        paper = {"server": {"port": 25565, "online": 1}}
        foton = {"server": {"port": 25566, "online": 2}}
        differences = compare_observations(paper, foton, rules)
        self.assertEqual(1, len(differences))
        self.assertIn("/server/online", differences[0])

    def test_normalizer_rejects_wrong_type(self):
        with self.assertRaises(ValueError):
            normalize_observation({"port": "wrong"}, [{"path": "/port", "kind": "port"}])

    def test_extra_callback_fails(self):
        differences = compare_observations({"events": []}, {"events": ["ExtraEvent"]}, [])
        self.assertTrue(any("/events/0" in line and "extra" in line for line in differences))


if __name__ == "__main__":
    unittest.main()
