#!/usr/bin/env python3
"""Checks on the plugin API usage scanner.

The scanner reads a binary format by hand, so the tests compile real class
files with `javac` rather than asserting against bytes typed out here. A
fixture that was hand-written would only prove the reader agrees with whoever
wrote the fixture.
"""
import pathlib
import shutil
import subprocess
import sys
import tempfile
import unittest
import zipfile

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import plugin_api_usage


PLUGIN = """
package example;

import org.bukkit.Bukkit;
import org.bukkit.entity.Player;

public class ExamplePlugin {
    public void greet(Player player) {
        player.sendMessage(Bukkit.getServerName());
    }
}
"""

REACHES_INTERNALS = """
package example;

import net.minecraft.FakeInternals;

public class Sneaky {
    public void poke() {
        FakeInternals.reachInside();
    }
}
"""

STUBS = {
    "org/bukkit/Bukkit.java": """
        package org.bukkit;
        public final class Bukkit {
            public static String getServerName() { return ""; }
        }
    """,
    "org/bukkit/entity/Player.java": """
        package org.bukkit.entity;
        public interface Player {
            void sendMessage(String message);
        }
    """,
    "net/minecraft/FakeInternals.java": """
        package net.minecraft;
        public final class FakeInternals {
            public static void reachInside() { }
        }
    """,
}


def build_jar(sources, into):
    """Compiles sources and packs the class files into a jar. Returns its path."""
    root = into / "src"
    for name, body in {**STUBS, **sources}.items():
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(body, encoding="utf-8")

    classes = into / "classes"
    classes.mkdir()
    subprocess.run(
        ["javac", "-nowarn", "-d", str(classes), *[str(p) for p in root.rglob("*.java")]],
        check=True,
        capture_output=True,
    )

    jar = into / "example.jar"
    with zipfile.ZipFile(jar, "w") as archive:
        for compiled in classes.rglob("*.class"):
            archive.write(compiled, compiled.relative_to(classes).as_posix())
    return jar


@unittest.skipIf(shutil.which("javac") is None, "javac is needed to build the fixture")
class Scanning(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls._dir = tempfile.TemporaryDirectory()
        root = pathlib.Path(cls._dir.name)
        cls.plain = build_jar({"example/ExamplePlugin.java": PLUGIN}, root / "plain")
        cls.sneaky = build_jar({"example/Sneaky.java": REACHES_INTERNALS}, root / "sneaky")

    @classmethod
    def tearDownClass(cls):
        cls._dir.cleanup()

    def test_it_finds_the_api_members_a_plugin_calls(self):
        found, _ = plugin_api_usage.scan(self.plain)

        self.assertIn("org/bukkit/Bukkit#getServerName", found["api"])
        self.assertIn("org/bukkit/entity/Player#sendMessage", found["api"])

    def test_a_plugin_that_stays_on_the_api_is_not_counted_as_reaching_inside(self):
        # The ceiling on how much of the ecosystem can ever run is this number,
        # so a plugin landing on the wrong side of it matters more than most
        # miscounts would.
        found, _ = plugin_api_usage.scan(self.plain)

        self.assertEqual(found["internal"], set())

    def test_reaching_for_the_mojang_server_is_counted_separately(self):
        found, _ = plugin_api_usage.scan(self.sneaky)

        self.assertIn("net/minecraft/FakeInternals#reachInside", found["internal"])
        self.assertEqual(found["api"], set())

    def test_every_class_in_the_jar_is_read(self):
        # A jar whose classes silently failed to parse would produce a ranking
        # that looks exactly as authoritative as a correct one, which is the
        # failure this whole tool would be worst at surviving.
        _, unreadable = plugin_api_usage.scan(self.plain)

        self.assertEqual(unreadable, 0)


class ConstantPool(unittest.TestCase):
    def test_bytes_that_are_not_a_class_file_are_refused(self):
        with self.assertRaises(plugin_api_usage.NotAClassFile):
            plugin_api_usage.constant_pool(b"PK\\x03\\x04 this is a zip, not a class")


if __name__ == "__main__":
    unittest.main()
