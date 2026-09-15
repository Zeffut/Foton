#!/usr/bin/env python3
"""Regression tests for the plugin API and Minecraft source build scripts."""

from __future__ import annotations

import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
GENERATED_REGISTRY_INPUTS = (
    "vanilla_entities.rs",
    "vanilla_enchantments.rs",
    "vanilla_potions.rs",
)


def export_tracked_checkout(destination: Path) -> None:
    archive_path = destination.parent / "checkout.tar"
    with archive_path.open("wb") as archive:
        subprocess.run(
            ["git", "-C", str(ROOT), "archive", "HEAD"],
            check=True,
            stdout=archive,
        )
    with tarfile.open(archive_path) as archive:
        archive.extractall(destination, filter="data")

    # Exercise the scripts from the working tree, including the change under test.
    for relative in (
        "dev/build-plugin-api.sh",
        "dev/gen-entity-type.py",
        "dev/gen-enchantment.py",
        "dev/gen-potion-type.py",
        "update-minecraft-src.sh",
    ):
        shutil.copy2(ROOT / relative, destination / relative)


class PluginApiBuildScriptTests(unittest.TestCase):
    def test_fresh_checkout_builds_from_a_path_with_spaces(self) -> None:
        with tempfile.TemporaryDirectory(prefix="foton plugin api ", dir="/tmp") as temporary:
            checkout = Path(temporary) / "fresh checkout"
            checkout.mkdir()
            export_tracked_checkout(checkout)

            generated = checkout / "foton-registry/src/generated"
            for filename in GENERATED_REGISTRY_INPUTS:
                self.assertFalse((generated / filename).exists())

            environment = os.environ.copy()
            environment["CARGO_TARGET_DIR"] = str(ROOT / "target")
            result = subprocess.run(
                ["bash", "dev/build-plugin-api.sh", "--check"],
                cwd=checkout,
                env=environment,
                capture_output=True,
                text=True,
                timeout=300,
            )

            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertTrue((checkout / "plugin-api/build/foton-plugin-api.jar").is_file())
            for filename in GENERATED_REGISTRY_INPUTS:
                self.assertTrue((generated / filename).is_file(), filename)

    def test_minecraft_source_dry_run_is_target_exact_and_network_free(self) -> None:
        with tempfile.TemporaryDirectory(prefix="foton minecraft source ", dir="/tmp") as temporary:
            checkout = Path(temporary) / "fresh checkout"
            checkout.mkdir()
            export_tracked_checkout(checkout)

            fake_bin = Path(temporary) / "fake-bin"
            fake_bin.mkdir()
            fake_git = fake_bin / "git"
            fake_git.write_text("#!/bin/sh\nexit 97\n", encoding="utf-8")
            fake_git.chmod(0o755)

            environment = os.environ.copy()
            environment["PATH"] = f"{fake_bin}:{environment['PATH']}"
            result = subprocess.run(
                ["bash", "update-minecraft-src.sh", "--dry-run"],
                cwd=checkout,
                env=environment,
                capture_output=True,
                text=True,
                timeout=10,
            )

            output = result.stdout + result.stderr
            self.assertEqual(result.returncode, 0, output)
            self.assertIn("61c79f013547b4782096c7a15c183b3f79548e60", output)
            self.assertIn("--only-version=26.2", output)
            self.assertIn("/tmp/", output)
            self.assertNotIn(".gitcraft-tmp", output)


if __name__ == "__main__":
    unittest.main()
