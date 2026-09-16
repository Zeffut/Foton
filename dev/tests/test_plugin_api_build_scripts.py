#!/usr/bin/env python3
"""Regression tests for the plugin API and Minecraft source build scripts."""

from __future__ import annotations

import io
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
GENERATED_REGISTRY_INPUTS = (
    "vanilla_entities.rs",
    "vanilla_enchantments.rs",
    "vanilla_potions.rs",
)


def interpreter_environment() -> dict:
    environment = os.environ.copy()
    python_directory = str(Path(sys.executable).parent)
    environment["PATH"] = f"{python_directory}:{environment['PATH']}"
    return environment


def add_tar_file(archive: tarfile.TarFile, name: str, contents: bytes = b"payload") -> None:
    member = tarfile.TarInfo(name)
    member.size = len(contents)
    archive.addfile(member, io.BytesIO(contents))


def extract_archive_safely(archive: tarfile.TarFile, destination: Path) -> None:
    base = destination.resolve()
    members = archive.getmembers()
    for member in members:
        if not (member.isfile() or member.isdir()):
            raise ValueError(f"unsupported archive member: {member.name}")
        target = (base / member.name).resolve()
        try:
            target.relative_to(base)
        except ValueError as error:
            raise ValueError(f"archive member escapes destination: {member.name}") from error
    archive.extractall(destination, members=members)


def export_tracked_checkout(destination: Path) -> None:
    archive_path = destination.parent / "checkout.tar"
    with archive_path.open("wb") as archive:
        subprocess.run(
            ["git", "-C", str(ROOT), "archive", "HEAD"],
            check=True,
            stdout=archive,
        )
    with tarfile.open(archive_path) as archive:
        extract_archive_safely(archive, destination)

    # Exercise the scripts from the working tree, including the change under test.
    for relative in (
        "dev/build-plugin-api.sh",
        "dev/gen-attribute.py",
        "dev/gen-entity-type.py",
        "dev/gen-enchantment.py",
        "dev/gen-potion-type.py",
        "foton-registry/build_assets/attributes.json",
        "plugin-api/check/FotonPotionLookupRunner.java",
        "plugin-api/check/PaperAttributeConsumer.java",
        "plugin-api/src/org/bukkit/potion/PotionEffectType.java",
        "update-minecraft-src.sh",
    ):
        shutil.copy2(ROOT / relative, destination / relative)
    (destination / "plugin-api/src/org/bukkit/attribute/Attribute.java").unlink(missing_ok=True)


class PluginApiBuildScriptTests(unittest.TestCase):
    def test_generated_entity_classes_are_backed_by_api_sources_or_planned_interfaces(self) -> None:
        with tempfile.TemporaryDirectory(prefix="foton entity types ", dir="/tmp") as temporary:
            output = Path(temporary)
            result = subprocess.run(
                [sys.executable, "dev/gen-entity-type.py", str(output)],
                cwd=ROOT,
                capture_output=True,
                text=True,
                timeout=30,
            )

            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            generated = (output / "org/bukkit/entity/EntityType.java").read_text(
                encoding="utf-8"
            )
            emitted = set(
                re.findall(
                    r'case "(org\.bukkit\.entity\.[A-Za-z0-9_.]+)"', generated
                )
            )
            source_classes = {
                ".".join(source.relative_to(ROOT / "plugin-api/src").with_suffix("").parts)
                for source in (ROOT / "plugin-api/src/org/bukkit/entity").rglob("*.java")
            }
            planned_interfaces = {
                "org.bukkit.entity.Allay",
                "org.bukkit.entity.ItemDisplay",
                "org.bukkit.entity.TextDisplay",
            }
            self.assertEqual(emitted - source_classes - planned_interfaces, set())

    def test_archive_extraction_rejects_parent_traversal(self) -> None:
        with tempfile.TemporaryDirectory(prefix="foton archive traversal ", dir="/tmp") as temporary:
            root = Path(temporary)
            archive_path = root / "malicious.tar"
            destination = root / "checkout"
            outside = root / "outside.txt"
            with tarfile.open(archive_path, "w") as archive:
                add_tar_file(archive, "checkout/../../outside.txt")

            with tarfile.open(archive_path) as archive:
                with self.assertRaises(ValueError):
                    extract_archive_safely(archive, destination)
            self.assertFalse(outside.exists())

    def test_archive_extraction_rejects_symbolic_links(self) -> None:
        with tempfile.TemporaryDirectory(prefix="foton archive symlink ", dir="/tmp") as temporary:
            root = Path(temporary)
            archive_path = root / "malicious.tar"
            destination = root / "checkout"
            outside = root / "outside.txt"
            with tarfile.open(archive_path, "w") as archive:
                link = tarfile.TarInfo("link")
                link.type = tarfile.SYMTYPE
                link.linkname = "../outside.txt"
                archive.addfile(link)
                add_tar_file(archive, "link/payload.txt")

            with tarfile.open(archive_path) as archive:
                with self.assertRaises(ValueError):
                    extract_archive_safely(archive, destination)
            self.assertFalse(outside.exists())

    def test_fresh_and_cached_builds_regenerate_missing_sources_from_path_with_spaces(self) -> None:
        with tempfile.TemporaryDirectory(prefix="foton plugin api ", dir="/tmp") as temporary:
            checkout = Path(temporary) / "fresh checkout"
            checkout.mkdir()
            export_tracked_checkout(checkout)

            generated = checkout / "foton-registry/src/generated"
            for filename in GENERATED_REGISTRY_INPUTS:
                self.assertFalse((generated / filename).exists())

            environment = interpreter_environment()
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

            backup = Path(temporary) / "generated-backup"
            backup.mkdir()
            for filename in GENERATED_REGISTRY_INPUTS:
                shutil.copy2(generated / filename, backup / filename)
                (generated / filename).unlink()
            fake_bin = Path(temporary) / "fake-cargo-bin"
            fake_bin.mkdir()
            fake_cargo = fake_bin / "cargo"
            fake_cargo.write_text(
                """#!/bin/sh
if [ "$1" = "check" ]; then
  cp "$FOTON_TEST_REGISTRY_BACKUP"/*.rs "$FOTON_TEST_REGISTRY_OUTPUT/"
fi
exit 0
""",
                encoding="utf-8",
            )
            fake_cargo.chmod(0o755)
            cached_environment = environment.copy()
            cached_environment["PATH"] = f"{fake_bin}:{cached_environment['PATH']}"
            cached_environment["FOTON_TEST_REGISTRY_BACKUP"] = str(backup)
            cached_environment["FOTON_TEST_REGISTRY_OUTPUT"] = str(generated)
            cached_result = subprocess.run(
                ["bash", "dev/build-plugin-api.sh"],
                cwd=checkout,
                env=cached_environment,
                capture_output=True,
                text=True,
                timeout=300,
            )

            self.assertEqual(
                cached_result.returncode,
                0,
                cached_result.stdout + cached_result.stderr,
            )
            for filename in GENERATED_REGISTRY_INPUTS:
                self.assertTrue((generated / filename).is_file(), filename)

    def test_registry_generation_reports_every_missing_output(self) -> None:
        with tempfile.TemporaryDirectory(prefix="foton registry output ", dir="/tmp") as temporary:
            checkout = Path(temporary) / "fresh checkout"
            checkout.mkdir()
            export_tracked_checkout(checkout)

            fake_bin = Path(temporary) / "fake-bin"
            fake_bin.mkdir()
            fake_cargo = fake_bin / "cargo"
            fake_cargo.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
            fake_cargo.chmod(0o755)

            environment = interpreter_environment()
            environment["PATH"] = f"{fake_bin}:{environment['PATH']}"
            result = subprocess.run(
                ["bash", "dev/build-plugin-api.sh"],
                cwd=checkout,
                env=environment,
                capture_output=True,
                text=True,
                timeout=30,
            )

            output = result.stdout + result.stderr
            self.assertNotEqual(result.returncode, 0, output)
            self.assertIn("foton-registry build did not generate", output)
            for filename in GENERATED_REGISTRY_INPUTS:
                self.assertIn(filename, output)

    def test_existing_stale_entity_registry_is_refreshed_before_java_generation(self) -> None:
        with tempfile.TemporaryDirectory(prefix="foton stale entity registry ", dir="/tmp") as temporary:
            root = Path(temporary)
            checkout = root / "existing checkout"
            checkout.mkdir()
            export_tracked_checkout(checkout)

            generated = checkout / "foton-registry/src/generated"
            generated.mkdir(parents=True, exist_ok=True)
            backups = root / "registry-backup"
            backups.mkdir()
            for filename in GENERATED_REGISTRY_INPUTS:
                shutil.copy2(ROOT / "foton-registry/src/generated" / filename, backups / filename)
                shutil.copy2(backups / filename, generated / filename)
            stale_marker = "pub static STALE_CHECKOUT_ENTITY: u8 = 0;\n"
            (generated / "vanilla_entities.rs").write_text(stale_marker, encoding="utf-8")

            build_script_input = checkout / "foton-registry/build/build.rs"
            invalidation_barrier = root / "invalidation-barrier"
            os.utime(build_script_input, ns=(1_000_000_000, 1_000_000_000))
            invalidation_barrier.write_text("cargo may regenerate only after build.rs changes\n")
            os.utime(invalidation_barrier, ns=(2_000_000_000, 2_000_000_000))

            fake_bin = root / "fake-bin"
            fake_bin.mkdir()
            fake_cargo = fake_bin / "cargo"
            fake_cargo.write_text(
                """#!/bin/sh
if [ "$1" = "check" ]; then
  if [ ! "$FOTON_TEST_BUILD_SCRIPT" -nt "$FOTON_TEST_INVALIDATION_BARRIER" ]; then
    echo "registry build script was not invalidated before cargo" >&2
    exit 23
  fi
  : > "$FOTON_TEST_CARGO_INVALIDATED"
  cp "$FOTON_TEST_REGISTRY_BACKUP"/*.rs "$FOTON_TEST_REGISTRY_OUTPUT/"
fi
exit 0
""",
                encoding="utf-8",
            )
            fake_cargo.chmod(0o755)

            environment = interpreter_environment()
            environment["PATH"] = f"{fake_bin}:{environment['PATH']}"
            environment["FOTON_TEST_CARGO_INVALIDATED"] = str(root / "cargo-invalidated")
            environment["FOTON_TEST_BUILD_SCRIPT"] = str(build_script_input)
            environment["FOTON_TEST_INVALIDATION_BARRIER"] = str(invalidation_barrier)
            environment["FOTON_TEST_REGISTRY_BACKUP"] = str(backups)
            environment["FOTON_TEST_REGISTRY_OUTPUT"] = str(generated)
            result = subprocess.run(
                ["bash", "dev/build-plugin-api.sh"],
                cwd=checkout,
                env=environment,
                capture_output=True,
                text=True,
                timeout=300,
            )

            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertTrue(Path(environment["FOTON_TEST_CARGO_INVALIDATED"]).is_file())
            self.assertNotIn(
                "STALE_CHECKOUT_ENTITY",
                (generated / "vanilla_entities.rs").read_text(encoding="utf-8"),
            )

    def test_minecraft_source_preserves_spaced_target_as_one_gradle_argument(self) -> None:
        with tempfile.TemporaryDirectory(prefix="foton minecraft source ", dir="/tmp") as temporary:
            root = Path(temporary)
            checkout = root / "fresh checkout with spaces"
            checkout.mkdir()
            shutil.copy2(ROOT / "Cargo.toml", checkout / "Cargo.toml")
            shutil.copy2(ROOT / "update-minecraft-src.sh", checkout / "update-minecraft-src.sh")

            fake_bin = root / "fake-bin"
            fake_bin.mkdir()
            fake_gradlew = fake_bin / "fake-gradlew"
            fake_gradlew.write_text(
                """#!/usr/bin/env python3
import json
import os
import shlex
import sys

payload = next(argument.split("=", 1)[1] for argument in sys.argv[1:] if argument.startswith("--args="))
with open(os.environ["FOTON_TEST_GRADLE_ARGS"], "w", encoding="utf-8") as output:
    json.dump(shlex.split(payload), output)
""",
                encoding="utf-8",
            )
            fake_gradlew.chmod(0o755)
            fake_git = fake_bin / "git"
            fake_git.write_text(
                """#!/bin/sh
if [ "$1" = "clone" ]; then
  destination="$3"
  mkdir -p "$destination"
  printf '%s\n' 'org.gradle.jvmargs=-Xmx4G' > "$destination/build.gradle"
  cp "$FOTON_TEST_FAKE_GRADLEW" "$destination/gradlew"
  chmod +x "$destination/gradlew"
fi
exit 0
""",
                encoding="utf-8",
            )
            fake_git.chmod(0o755)

            observed_args = root / "observed-args.json"
            environment = interpreter_environment()
            environment["PATH"] = f"{fake_bin}:{environment['PATH']}"
            environment["FOTON_TEST_FAKE_GRADLEW"] = str(fake_gradlew)
            environment["FOTON_TEST_GRADLE_ARGS"] = str(observed_args)
            result = subprocess.run(
                ["bash", "update-minecraft-src.sh"],
                cwd=checkout,
                env=environment,
                capture_output=True,
                text=True,
                timeout=10,
            )

            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertEqual(
                json.loads(observed_args.read_text(encoding="utf-8")),
                [
                    f"--override-repo-target={checkout.resolve() / 'minecraft-src'}",
                    "--only-version=26.2",
                    "--only-unobfuscated",
                    "--mappings=identity_unmapped",
                    "--only-stable",
                ],
            )

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

            environment = interpreter_environment()
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
