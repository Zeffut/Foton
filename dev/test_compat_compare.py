"""Regression tests for the Paper/Foton observation comparator."""

import unittest
from pathlib import Path
import tempfile
from unittest.mock import patch

from dev.compat.compare import (
    collect_log_observations,
    compare_observations,
    find_unexpected_errors,
    install_inputs,
    marker_after,
    normalize_observation,
    validate_scenario,
)


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

    def test_uncollected_surfaces_rejected_before_launch(self):
        scenario = {"plugin": "Fixture", "server_version": "26.2", "setup": {"jars": []},
                    "actions": [{"type": "status_ping"}],
                    "observations": {"items": ["components"], "events": ["ItemEvent"]},
                    "normalizers": []}
        with self.assertRaisesRegex(ValueError, "uncollected observation surface.*events"):
            validate_scenario(scenario)

    def test_console_command_requires_post_command_effect(self):
        scenario = {"plugin": "Fixture", "server_version": "26.2", "setup": {"jars": []},
                    "actions": [{"type": "console_command", "command": "say hi"}],
                    "observations": {}, "normalizers": []}
        with self.assertRaisesRegex(ValueError, "expect_marker"):
            validate_scenario(scenario)

    def test_command_marker_must_arrive_after_command(self):
        with tempfile.TemporaryDirectory() as scratch:
            log = Path(scratch) / "server.log"
            log.write_text("ORACLE:command.done=ok\n", encoding="utf-8")
            offset = log.stat().st_size
            self.assertFalse(marker_after(log, offset, "command.done", "ok", 0.01))
            with log.open("a", encoding="utf-8") as output:
                output.write("ORACLE:command.done=ok\n")
            self.assertTrue(marker_after(log, offset, "command.done", "ok", 0.01))

    def test_unexpected_errors_fail_closed(self):
        for message in ("IllegalStateException: bad state", "JNI invocation failed",
                        "decoder error", "encoder failure", "buffer reference-count error",
                        "Plugin failed to enable"):
            with self.subTest(message=message):
                self.assertEqual([message], find_unexpected_errors([message]))
        self.assertEqual([], find_unexpected_errors(["INFO: ORACLE:lifecycle.enabled=Fixture"]))

    def test_duplicate_basename_cannot_overwrite(self):
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            (root / "a").mkdir()
            (root / "b").mkdir()
            (root / "plugins").mkdir()
            first = root / "a" / "same.jar"
            second = root / "b" / "same.jar"
            first.write_bytes(b"first")
            second.write_bytes(b"second")
            with self.assertRaisesRegex(ValueError, "duplicate plugin filename"):
                install_inputs([first, second], root / "plugins")
            self.assertEqual([], list((root / "plugins").iterdir()))

    def test_installed_input_hash_is_verified(self):
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            source = root / "source.jar"
            source.write_bytes(b"correct")
            destination = root / "plugins"
            destination.mkdir()
            with patch("dev.compat.compare.shutil.copy2", side_effect=lambda _src, dst: Path(dst).write_bytes(b"wrong")):
                with self.assertRaisesRegex(RuntimeError, "installed checksum mismatch"):
                    install_inputs([source], destination)

    def test_lifecycle_order_comes_from_structured_marker(self):
        observations = collect_log_observations([
            "Enabling Unrelated v1.0",
            "INFO: ORACLE:lifecycle.enabled=Fixture",
            "INFO: ORACLE:lifecycle.disabled=Fixture",
        ])
        self.assertEqual(["Fixture"], observations["startup_order"])
        self.assertEqual(2, len(observations["events"]))


if __name__ == "__main__":
    unittest.main()
