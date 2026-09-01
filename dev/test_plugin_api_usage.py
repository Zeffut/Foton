#!/usr/bin/env python3
"""Checks on the plugin API usage scanner.

The scanner reads a binary format by hand, so the tests compile real class
files with `javac` rather than asserting against bytes typed out here. A
fixture that was hand-written would only prove the reader agrees with whoever
wrote the fixture.
"""
import collections
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
        // Compiles to a reference on Player, whose class file declares no such
        // method: java.lang.Object does. The scanner has to know that.
        player.sendMessage(player.toString());
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
    "org/bukkit/Season.java": """
        package org.bukkit;
        public enum Season { SPRING, SUMMER }
    """,
    "org/bukkit/command/CommandSender.java": """
        package org.bukkit.command;
        public interface CommandSender {
            String getName();
        }
    """,
    "org/bukkit/entity/Player.java": """
        package org.bukkit.entity;
        public interface Player extends org.bukkit.command.CommandSender {
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


def build_jar(sources, into, drop=None):
    """Compiles sources and packs the class files into a jar. Returns its path.

    The stubs stand in for the API and are compiled but not packed when there
    are sources of their own: a real plugin jar carries its own classes and
    finds Bukkit's on the server. Packing them would make the plugin look like
    it referenced every member of the API it was merely compiled against.

    `drop` leaves one compiled class out, which is how a jar that cannot answer
    everything a plugin calls is built.
    """
    root = into / "src"
    for name, body in STUBS.items():
        path = root / "api" / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(body, encoding="utf-8")
    for name, body in sources.items():
        path = root / "own" / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(body, encoding="utf-8")

    stubs = into / "stubs"
    stubs.mkdir()
    subprocess.run(
        ["javac", "-nowarn", "-d", str(stubs),
         *[str(p) for p in (root / "api").rglob("*.java")]],
        check=True,
        capture_output=True,
    )

    packed = stubs
    if sources:
        classes = into / "classes"
        classes.mkdir()
        subprocess.run(
            ["javac", "-nowarn", "-d", str(classes), "-cp", str(stubs),
             *[str(p) for p in (root / "own").rglob("*.java")]],
            check=True,
            capture_output=True,
        )
        packed = classes

    jar = into / "example.jar"
    with zipfile.ZipFile(jar, "w") as archive:
        for compiled in packed.rglob("*.class"):
            entry = compiled.relative_to(packed).as_posix()
            if entry == drop:
                continue
            archive.write(compiled, entry)
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


@unittest.skipIf(shutil.which("javac") is None, "javac is needed to build the fixture")
class Gaps(unittest.TestCase):
    """What the built API jar can and cannot answer.

    This is the number the work is steered by, so a wrong answer here would
    send the next tranche of the API at the wrong members.
    """

    @classmethod
    def setUpClass(cls):
        cls._dir = tempfile.TemporaryDirectory()
        root = pathlib.Path(cls._dir.name)
        cls.plugin = build_jar({"example/ExamplePlugin.java": PLUGIN}, root / "plugin")
        # An API jar holding only what the stubs declare -- which is exactly
        # what the plugin calls, so nothing should be missing.
        cls.complete = build_jar({}, root / "complete")
        # The same, minus one method the plugin calls.
        cls.partial = build_jar({}, root / "partial", drop="org/bukkit/Bukkit.class")

    @classmethod
    def tearDownClass(cls):
        cls._dir.cleanup()

    def test_it_reads_what_a_class_declares(self):
        with zipfile.ZipFile(self.complete) as archive:
            name, supertypes, members = plugin_api_usage.declares(
                archive.read("org/bukkit/Bukkit.class"))

        self.assertEqual(name, "org/bukkit/Bukkit")
        self.assertIn("getServerName", members)
        self.assertIn("java/lang/Object", supertypes)

    def test_an_inherited_member_counts_as_provided(self):
        # A plugin calls `Child#method` and `Parent#method` interchangeably and
        # the JVM resolves both, so a jar that only listed declared members
        # would report a gap that does not exist.
        have = plugin_api_usage.provided(self.complete)

        self.assertIn("sendMessage", have["org/bukkit/entity/Player"])
        self.assertIn("getName", have["org/bukkit/entity/Player"],
                      "a member inherited from a supertype should still resolve")

    def test_what_java_lang_object_gives_every_class_is_not_a_gap(self):
        # `player.toString()` compiles to a reference on Player, and the Object
        # class file is not in the API jar. Counting that as missing would put
        # a phantom member near the top of the ranking.
        corpus = self.plugin.parent
        _, missing = plugin_api_usage.gaps(corpus, self.complete)

        self.assertNotIn("org/bukkit/entity/Player#toString", missing)

    def test_what_an_enum_gives_its_constants_is_not_a_gap(self):
        # `Material.name()` comes from java.lang.Enum, whose class file is not
        # in the API jar. Counting it missing put a member every plugin
        # "needs" at the top of the ranking, pointing the work at nothing.
        have = plugin_api_usage.provided(self.complete)

        self.assertIn("name", have["org/bukkit/Season"])
        self.assertIn("toString", have["org/bukkit/Season"])

    def test_a_jar_that_answers_everything_leaves_no_gap(self):
        corpus = self.plugin.parent
        per_plugin, missing = plugin_api_usage.gaps(corpus, self.complete)

        self.assertEqual(per_plugin[self.plugin.name][1], set())
        self.assertEqual(missing, collections.Counter())

    def test_a_missing_member_is_reported_against_the_plugins_that_call_it(self):
        corpus = self.plugin.parent
        _, missing = plugin_api_usage.gaps(corpus, self.partial)

        self.assertEqual(missing["org/bukkit/Bukkit#getServerName"], 1)


class ConstantPool(unittest.TestCase):
    def test_bytes_that_are_not_a_class_file_are_refused(self):
        with self.assertRaises(plugin_api_usage.NotAClassFile):
            plugin_api_usage.constant_pool(b"PK\\x03\\x04 this is a zip, not a class")


if __name__ == "__main__":
    unittest.main()
