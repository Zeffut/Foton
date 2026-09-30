#!/usr/bin/env python3
"""Checks on the plugin API usage scanner.

The scanner reads a binary format by hand, so the tests compile real class
files with `javac` rather than asserting against bytes typed out here. A
fixture that was hand-written would only prove the reader agrees with whoever
wrote the fixture.
"""
import collections
import contextlib
import io
import json
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
import zipfile
from unittest import mock

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

REACHES_CRAFTBUKKIT = """
package example;

import org.bukkit.craftbukkit.entity.CraftPlayer;

public class Crafty {
    public Object handle(CraftPlayer player) {
        return player.getHandle();
    }
}
"""

CLASS_ONLY_SOURCE = """
package example;

import org.bukkit.World;

public class ClassOnly {
    public Class<?> worldType() {
        return World.class;
    }
}
"""

HANDLER_SOURCE = """
package example;

import org.bukkit.event.EventHandler;
import org.bukkit.event.world.WorldUnloadEvent;

public class WorldListener {
    @EventHandler
    public void onWorldUnload(WorldUnloadEvent event) { }
}
"""

JAVA_BRIDGE_WITH_ORPHAN = """
package foton;

import org.bukkit.event.world.WorldLoadEvent;
import org.bukkit.event.world.WorldUnloadEvent;

public final class EventBridge {
    public static void fireWorldLoad() {
        dispatch(new WorldLoadEvent());
    }

    public static void fireWorldUnload() {
        dispatch(new WorldUnloadEvent());
    }

    private static void dispatch(Object event) { }
}
"""

RUST_WITHOUT_CALLER = """
const BRIDGE: &str = "foton/EventBridge";
fn forward_world_load(env: &mut JNIEnv) {
    env.call_static_method(BRIDGE, "fireWorldLoad", "()V", &[]);
}
"""

RUST_WITH_WRONG_DESCRIPTOR = """
const BRIDGE: &str = "foton/EventBridge";
fn forward_world_load(env: &mut JNIEnv) {
    env.call_static_method(
        BRIDGE, "fireWorldLoad", "(Ljava/lang/String;)V", &[]);
}
"""

RUST_DYNAMIC_HELPER_CALL = """
const BRIDGE: &str = "foton/EventBridge";
fn world_call(env: &mut JNIEnv, method: &str) {
    env.call_static_method(BRIDGE, method, "()V", &[]);
}
fn forward_world_load(env: &mut JNIEnv) {
    world_call(env, "fireWorldLoad");
}
"""

JAVA_OVERLOADED_FIRE = """
package foton;

import org.bukkit.event.world.WorldLoadEvent;
import org.bukkit.event.world.WorldUnloadEvent;

public final class EventBridge {
    public static void fireOverloaded(String value) {
        new WorldLoadEvent();
    }

    public static void fireOverloaded(int value) {
        new WorldUnloadEvent();
    }
}
"""

RUST_OVERLOADED_FIRE = """
const BRIDGE: &str = "foton/EventBridge";
fn forward(env: &mut JNIEnv) {
    env.call_static_method(
        BRIDGE, "fireOverloaded", "(Ljava/lang/String;)V", &[]);
}
"""

JAVA_LAMBDA_EVENT = """
package foton;

import org.bukkit.event.server.PluginDisableEvent;

public final class PluginHost {
    public static void disableAll() {
        release(() -> new PluginDisableEvent());
    }

    private static void release(Runnable action) {
        action.run();
    }
}
"""

RUST_LAMBDA_EVENT = """
const HOST: &str = "foton/PluginHost";
fn shutdown(env: &mut JNIEnv) {
    env.call_static_method(HOST, "disableAll", "()V", &[]);
}
"""

JAVA_JNI_ENTRY_POINTS = {
    "foton/PluginHost.java": """
        package foton;
        import org.bukkit.event.server.PluginDisableEvent;
        import org.bukkit.event.server.PluginEnableEvent;
        public final class PluginHost {
            public static void enableAll() { enable(); }
            private static void enable() {
                FotonLifecycle.dispatchCommands();
                new PluginEnableEvent();
            }
            public static void disableAll() { disable(); }
            private static void disable() { new PluginDisableEvent(); }
        }
    """,
    "foton/FotonLifecycle.java": """
        package foton;
        import io.papermc.paper.plugin.lifecycle.event.registrar.ReloadableRegistrarEvent;
        public final class FotonLifecycle {
            public static void dispatchCommands() {
                ReloadableRegistrarEvent event = () -> null;
                consume(event);
            }
            private static void consume(Object event) { }
        }
    """,
    "foton/FotonMessenger.java": """
        package foton;
        import org.bukkit.event.player.PlayerRegisterChannelEvent;
        import org.bukkit.event.player.PlayerUnregisterChannelEvent;
        public final class FotonMessenger {
            public static void dispatchFromNetwork() {
                FotonMessenger messenger = new FotonMessenger();
                messenger.updateListening();
            }
            private void updateListening() {
                new PlayerRegisterChannelEvent();
                new PlayerUnregisterChannelEvent();
            }
        }
    """,
    "foton/UncalledHost.java": """
        package foton;
        import org.bukkit.event.world.WorldUnloadEvent;
        public final class UncalledHost {
            public static void enableAll() { new WorldUnloadEvent(); }
        }
    """,
}

RUST_JNI_ENTRY_POINTS = """
const HOST_CLASS: &str = "foton/PluginHost";
const MESSENGER: &str = "foton/FotonMessenger";

fn lifecycle(env: &mut JNIEnv) {
    env.call_static_method(HOST_CLASS, "enableAll", "()V", &[]);
    env.call_static_method(HOST_CLASS, "disableAll", "()V", &[]);
    env.call_static_method(MESSENGER, "dispatchFromNetwork", "()V", &[]);
}
"""

INTERNAL_CLASS_ONLY_SOURCE = """
package example;

import net.minecraft.FakeInternals;

public class InternalClassOnly {
    public Class<?> internalType() {
        return FakeInternals.class;
    }
}
"""

OPTIONAL_ADAPTER_SOURCES = {
    "example/Main.java": """
        package example;
        import org.bukkit.World;
        public final class Main {
            public Class<?> worldType() { return World.class; }
        }
    """,
    "example/InternalAdapter.java": """
        package example;
        import net.minecraft.FakeInternals;
        public final class InternalAdapter {
            public void use() { FakeInternals.reachInside(); }
        }
    """,
}

LOAD_BEARING_INTERNAL_SOURCE = """
package example;
import net.minecraft.FakeInternals;
public final class Main {
    public void use() { FakeInternals.reachInside(); }
}
"""

UNREADABLE_ADAPTER_SOURCES = {
    "example/Main.java": """
        package example;
        public final class Main {
            public Adapter adapter() { return new Adapter(); }
        }
    """,
    "example/Adapter.java": """
        package example;
        import net.minecraft.FakeInternals;
        public final class Adapter {
            public void use() { FakeInternals.reachInside(); }
        }
    """,
}

LEDGER = {
    "schema_version": 2,
    "plugins_scanned": 3,
    "plugins_reaching_internals": 1,
    "api_members_referenced": 2,
    "api_members_kept_at_least": 2,
    "api_members": [
        {
            "member": "org/bukkit/World#getName()Ljava/lang/String;",
            "plugins": 2,
        },
        {"member": "org/bukkit/World#getSeed()J", "plugins": 1},
    ],
    "api_classes_referenced": 2,
    "api_classes": [
        {"class": "org/bukkit/World", "plugins": 2},
        {"class": "org/bukkit/Server", "plugins": 1},
    ],
    "plugin_incidence": [
        {
            "plugin": "Public.jar",
            "reachability_status": "complete",
            "reachability_reason": None,
            "archive_error": None,
            "entrypoints": ["example/Public"],
            "entrypoint_reachable_classes": ["example/Public"],
            "unreadable_class_count": 0,
            "unreadable_classes": [],
            "internal": {
                "load_bearing_classes": [],
                "load_bearing_members": [],
                "optional_adapter_classes": [],
                "optional_adapter_members": [],
            },
        },
        {
            "plugin": "OptionalAdapter.jar",
            "reachability_status": "complete",
            "reachability_reason": None,
            "archive_error": None,
            "entrypoints": ["example/OptionalAdapter"],
            "entrypoint_reachable_classes": ["example/OptionalAdapter"],
            "unreadable_class_count": 0,
            "unreadable_classes": [],
            "internal": {
                "load_bearing_classes": [],
                "load_bearing_members": [],
                "optional_adapter_classes": ["net/minecraft/Optional"],
                "optional_adapter_members": ["net/minecraft/Optional#use()V"],
            },
        },
        {
            "plugin": "LoadBearing.jar",
            "reachability_status": "complete",
            "reachability_reason": None,
            "archive_error": None,
            "entrypoints": ["example/LoadBearing"],
            "entrypoint_reachable_classes": ["example/LoadBearing"],
            "unreadable_class_count": 0,
            "unreadable_classes": [],
            "internal": {
                "load_bearing_classes": ["net/minecraft/Required"],
                "load_bearing_members": ["net/minecraft/Required#use()V"],
                "optional_adapter_classes": [],
                "optional_adapter_members": [],
            },
        },
    ],
}

PROVIDED = {"org/bukkit/World": {"getName()Ljava/lang/String;"}}
WANTED = {
    "org/bukkit/event/world/WorldLoadEvent",
    "org/bukkit/event/world/WorldUnloadEvent",
}
EMITTED = {"org/bukkit/event/world/WorldLoadEvent"}
EVENT_PARENTS = {
    "org/bukkit/event/world/WorldUnloadEvent": {
        "org/bukkit/event/world/WorldEvent",
    },
    "org/bukkit/event/world/WorldEvent": {"org/bukkit/event/Event"},
}

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
    "org/bukkit/craftbukkit/entity/CraftPlayer.java": """
        package org.bukkit.craftbukkit.entity;
        public class CraftPlayer {
            public Object getHandle() { return null; }
        }
    """,
    "org/bukkit/World.java": """
        package org.bukkit;
        public interface World { }
    """,
    "org/bukkit/event/EventHandler.java": """
        package org.bukkit.event;
        import java.lang.annotation.ElementType;
        import java.lang.annotation.Retention;
        import java.lang.annotation.RetentionPolicy;
        import java.lang.annotation.Target;
        @Retention(RetentionPolicy.RUNTIME)
        @Target(ElementType.METHOD)
        public @interface EventHandler { }
    """,
    "org/bukkit/event/world/WorldUnloadEvent.java": """
        package org.bukkit.event.world;
        public class WorldUnloadEvent extends WorldEvent { }
    """,
    "org/bukkit/event/world/WorldLoadEvent.java": """
        package org.bukkit.event.world;
        public class WorldLoadEvent extends WorldEvent { }
    """,
    "org/bukkit/event/server/PluginEnableEvent.java": """
        package org.bukkit.event.server;
        public class PluginEnableEvent extends org.bukkit.event.Event { }
    """,
    "org/bukkit/event/server/PluginDisableEvent.java": """
        package org.bukkit.event.server;
        public class PluginDisableEvent extends org.bukkit.event.Event { }
    """,
    "org/bukkit/event/player/PlayerRegisterChannelEvent.java": """
        package org.bukkit.event.player;
        public class PlayerRegisterChannelEvent extends org.bukkit.event.Event { }
    """,
    "org/bukkit/event/player/PlayerUnregisterChannelEvent.java": """
        package org.bukkit.event.player;
        public class PlayerUnregisterChannelEvent extends org.bukkit.event.Event { }
    """,
    "io/papermc/paper/plugin/lifecycle/event/registrar/ReloadableRegistrarEvent.java": """
        package io.papermc.paper.plugin.lifecycle.event.registrar;
        public interface ReloadableRegistrarEvent {
            Object registrar();
        }
    """,
    "org/bukkit/event/world/WorldEvent.java": """
        package org.bukkit.event.world;
        public class WorldEvent extends org.bukkit.event.Event { }
    """,
    "org/bukkit/event/Event.java": """
        package org.bukkit.event;
        public class Event { }
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


def write_sources(root, sources):
    for name, body in sources.items():
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(body, encoding="utf-8")


def merge_jars(output, *jars):
    """Builds one test API jar from independently compiled real class files."""
    with zipfile.ZipFile(output, "w") as merged:
        written = set()
        for jar in jars:
            with zipfile.ZipFile(jar) as source:
                for entry in source.namelist():
                    if entry in written:
                        continue
                    merged.writestr(entry, source.read(entry))
                    written.add(entry)
    return output


def damage_zip_entry_crc(jar, entry_name):
    """Make one real ZIP entry fail its CRC check without damaging the archive."""
    data = bytearray(pathlib.Path(jar).read_bytes())
    offset = 0
    while True:
        offset = data.find(b"PK\x01\x02", offset)
        if offset < 0:
            raise AssertionError(f"central directory entry not found: {entry_name}")
        name_length = int.from_bytes(data[offset + 28:offset + 30], "little")
        extra_length = int.from_bytes(data[offset + 30:offset + 32], "little")
        comment_length = int.from_bytes(data[offset + 32:offset + 34], "little")
        name = bytes(data[offset + 46:offset + 46 + name_length]).decode()
        if name == entry_name:
            crc_offset = offset + 16
            crc = int.from_bytes(data[crc_offset:crc_offset + 4], "little")
            data[crc_offset:crc_offset + 4] = (crc ^ 1).to_bytes(4, "little")
            pathlib.Path(jar).write_bytes(data)
            return
        offset += 46 + name_length + extra_length + comment_length


def run_main(args, repo, ledger):
    output = io.StringIO()
    with (
        mock.patch.object(plugin_api_usage, "REPO", repo),
        mock.patch.object(plugin_api_usage, "LEDGER", ledger),
        mock.patch.object(sys, "argv", ["plugin_api_usage.py", *map(str, args)]),
        contextlib.redirect_stdout(output),
    ):
        result = plugin_api_usage.main()
    return result, output.getvalue()


@unittest.skipIf(shutil.which("javac") is None, "javac is needed to build the fixture")
class Scanning(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls._dir = tempfile.TemporaryDirectory()
        root = pathlib.Path(cls._dir.name)
        cls.plain = build_jar({"example/ExamplePlugin.java": PLUGIN}, root / "plain")
        cls.sneaky = build_jar({"example/Sneaky.java": REACHES_INTERNALS}, root / "sneaky")
        cls.crafty = build_jar({"example/Crafty.java": REACHES_CRAFTBUKKIT}, root / "crafty")
        cls.class_only = build_jar(
            {"example/ClassOnly.java": CLASS_ONLY_SOURCE}, root / "class-only")
        cls.internal_class_only = build_jar(
            {"example/InternalClassOnly.java": INTERNAL_CLASS_ONLY_SOURCE},
            root / "internal-class-only",
        )
        cls.handler = build_jar(
            {"example/WorldListener.java": HANDLER_SOURCE}, root / "handler")
        cls.api = build_jar({}, root / "api")
        cls.api_without_world = build_jar(
            {}, root / "api-without-world", drop="org/bukkit/World.class")

    @classmethod
    def tearDownClass(cls):
        cls._dir.cleanup()

    def test_it_finds_the_api_members_a_plugin_calls(self):
        found, _ = plugin_api_usage.scan(self.plain)

        self.assertIn(
            "org/bukkit/Bukkit#getServerName()Ljava/lang/String;", found["api"])
        self.assertIn(
            "org/bukkit/entity/Player#sendMessage(Ljava/lang/String;)V", found["api"])

    def test_a_plugin_that_stays_on_the_api_is_not_counted_as_reaching_inside(self):
        # The ceiling on how much of the ecosystem can ever run is this number,
        # so a plugin landing on the wrong side of it matters more than most
        # miscounts would.
        found, _ = plugin_api_usage.scan(self.plain)

        self.assertEqual(found["internal"], set())

    def test_reaching_for_the_mojang_server_is_counted_separately(self):
        found, _ = plugin_api_usage.scan(self.sneaky)

        self.assertIn("net/minecraft/FakeInternals#reachInside()V", found["internal"])
        self.assertEqual(found["api"], set())

    def test_craftbukkit_is_internal_even_though_its_name_starts_with_bukkit(self):
        found, _ = plugin_api_usage.scan(self.crafty)

        self.assertIn(
            "org/bukkit/craftbukkit/entity/CraftPlayer#getHandle()Ljava/lang/Object;",
            found["internal"])
        self.assertEqual(found["api"], set())

    def test_an_internals_reaching_plugin_is_not_offered_as_api_work(self):
        per_plugin, missing = plugin_api_usage.gaps(self.sneaky.parent, self.api)

        self.assertNotIn(self.sneaky.name, per_plugin)
        self.assertEqual(missing, collections.Counter())

    def test_every_class_in_the_jar_is_read(self):
        # A jar whose classes silently failed to parse would produce a ranking
        # that looks exactly as authoritative as a correct one, which is the
        # failure this whole tool would be worst at surviving.
        _, unreadable = plugin_api_usage.scan(self.plain)

        self.assertEqual(unreadable, 0)

    def test_scan_keeps_class_only_references(self):
        found, unreadable = plugin_api_usage.scan(self.class_only)

        self.assertEqual(unreadable, 0)
        self.assertIn("org/bukkit/World", found["classes"])

    def test_missing_class_only_reference_is_a_binary_gap(self):
        per_plugin, missing = plugin_api_usage.gaps(
            self.class_only.parent, self.api_without_world)

        self.assertEqual(per_plugin[self.class_only.name][0], 1)
        self.assertEqual(missing["org/bukkit/World"], 1)

    def test_handler_descriptor_records_fully_qualified_event(self):
        found, _ = plugin_api_usage.scan(self.handler)

        self.assertIn(
            "org/bukkit/event/world/WorldUnloadEvent", found["handler_events"])

    def test_corpus_write_counts_class_only_internals_in_the_ceiling(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            corpus = root / "corpus"
            corpus.mkdir()
            shutil.copy(self.plain, corpus / "plain.jar")
            shutil.copy(self.internal_class_only, corpus / "internal.jar")
            for jar, name, main in (
                (corpus / "plain.jar", "Plain", "example.ExamplePlugin"),
                (corpus / "internal.jar", "Internal", "example.InternalClassOnly"),
            ):
                with zipfile.ZipFile(jar, "a") as archive:
                    archive.writestr(
                        "plugin.yml", f"name: {name}\nmain: {main}\nversion: 1\n")
            repo = root / "repo"
            ledger = repo / "dev" / "plugin-api-usage.json"
            ledger.parent.mkdir(parents=True)

            result, output = run_main([corpus, "--write"], repo, ledger)
            written = json.loads(ledger.read_text(encoding="utf-8"))

        self.assertEqual(result, 0)
        self.assertIn("plugins known to reach past the public API: 1 of 2", output)
        self.assertIn("plugins with unknown entrypoint reachability: 0 of 2", output)
        self.assertEqual(written["plugins_reaching_internals"], 1)
        self.assertEqual(
            written["internal_classes"],
            [{"class": "net/minecraft/FakeInternals", "plugins": 1}],
        )

    def test_corpus_write_versions_plugin_incidence_and_separates_optional_adapters(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            corpus = root / "corpus"
            corpus.mkdir()
            optional = build_jar(
                OPTIONAL_ADAPTER_SOURCES, root / "optional-adapter")
            load_bearing = build_jar(
                {"example/Main.java": LOAD_BEARING_INTERNAL_SOURCE},
                root / "load-bearing",
            )
            with zipfile.ZipFile(optional, "a") as archive:
                archive.writestr(
                    "plugin.yml", "name: OptionalAdapter\nmain: example.Main\nversion: 1\n")
            with zipfile.ZipFile(load_bearing, "a") as archive:
                archive.writestr(
                    "plugin.yml", "name: LoadBearing\nmain: example.Main\nversion: 1\n")
            shutil.copy(optional, corpus / "optional.jar")
            shutil.copy(load_bearing, corpus / "load-bearing.jar")
            repo = root / "repo"
            ledger = repo / "dev" / "plugin-api-usage.json"
            ledger.parent.mkdir(parents=True)

            result, _ = run_main([corpus, "--write"], repo, ledger)
            written = json.loads(ledger.read_text(encoding="utf-8"))

        self.assertEqual(result, 0)
        self.assertEqual(written["schema_version"], 2)
        incidence = {row["plugin"]: row for row in written["plugin_incidence"]}
        self.assertEqual(
            incidence["optional.jar"]["internal"]["load_bearing_members"], [])
        self.assertEqual(
            incidence["optional.jar"]["internal"]["optional_adapter_members"],
            ["net/minecraft/FakeInternals#reachInside()V"],
        )
        self.assertEqual(
            incidence["load-bearing.jar"]["internal"]["load_bearing_members"],
            ["net/minecraft/FakeInternals#reachInside()V"],
        )

    def test_unreadable_referenced_adapter_makes_only_its_plugin_ceiling_unknown(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            corpus = root / "corpus"
            corpus.mkdir()
            source = build_jar(
                UNREADABLE_ADAPTER_SOURCES, root / "unreadable-adapter")
            unreadable = corpus / "unreadable.jar"
            with (
                zipfile.ZipFile(source) as original,
                zipfile.ZipFile(unreadable, "w") as rewritten,
            ):
                for entry in original.infolist():
                    data = original.read(entry)
                    if entry.filename == "example/Adapter.class":
                        data = b"not a class file"
                    rewritten.writestr(entry, data)
                rewritten.writestr(
                    "plugin.yml",
                    "name: UnreadableAdapter\nmain: example.Main\nversion: 1\n",
                )
            repo = root / "repo"
            ledger = repo / "dev" / "plugin-api-usage.json"
            ledger.parent.mkdir(parents=True)

            result, _ = run_main([corpus, "--write"], repo, ledger)
            written = json.loads(ledger.read_text(encoding="utf-8"))
            summary = plugin_api_usage.compatibility_summary(
                written, PROVIDED, WANTED, EMITTED)

        self.assertEqual(result, 0)
        self.assertEqual(written["plugins_reachability_unknown"], 1)
        row = written["plugin_incidence"][0]
        self.assertEqual(row["reachability_status"], "unknown")
        self.assertEqual(row["unreadable_class_count"], 1)
        self.assertEqual(
            row["unreadable_classes"],
            [{"entry": "example/Adapter.class", "reason": "bad magic"}],
        )
        self.assertEqual(summary["ceiling"]["status"], "unknown")
        self.assertIsNone(summary["ceiling"]["plugins_on_public_api"])

    def test_bad_crc_class_retains_entry_reason_and_corpus_denominator(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            corpus = root / "corpus"
            corpus.mkdir()
            jar = build_jar(UNREADABLE_ADAPTER_SOURCES, root / "bad-crc")
            with zipfile.ZipFile(jar, "a") as archive:
                archive.writestr(
                    "plugin.yml",
                    "name: BadCrc\nmain: example.Main\nversion: 1\n",
                )
            damage_zip_entry_crc(jar, "example/Adapter.class")
            shutil.copy(jar, corpus / "bad-crc.jar")
            repo = root / "repo"
            ledger = repo / "dev" / "plugin-api-usage.json"
            ledger.parent.mkdir(parents=True)

            result, _ = run_main([corpus, "--write"], repo, ledger)
            written = json.loads(ledger.read_text(encoding="utf-8"))
            summary = plugin_api_usage.compatibility_summary(
                written, PROVIDED, WANTED, EMITTED)

        self.assertEqual(result, 0)
        self.assertEqual(written["plugins_scanned"], 1)
        self.assertEqual(written["plugins_reachability_unknown"], 1)
        row = written["plugin_incidence"][0]
        self.assertEqual(row["reachability_status"], "unknown")
        self.assertEqual(
            row["unreadable_classes"],
            [{
                "entry": "example/Adapter.class",
                "reason": "Bad CRC-32 for file 'example/Adapter.class'",
            }],
        )
        self.assertEqual(summary["ceiling"]["status"], "unknown")
        self.assertIsNone(summary["ceiling"]["plugins_on_public_api"])

    def test_wholly_corrupt_jar_remains_an_unknown_corpus_item(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            corpus = root / "corpus"
            corpus.mkdir()
            valid = build_jar(
                {"example/Main.java": "package example; public final class Main {}"},
                root / "valid",
            )
            with zipfile.ZipFile(valid, "a") as archive:
                archive.writestr(
                    "plugin.yml", "name: Valid\nmain: example.Main\nversion: 1\n")
            shutil.copy(valid, corpus / "valid.jar")
            (corpus / "corrupt.jar").write_bytes(b"not a ZIP archive")
            repo = root / "repo"
            ledger = repo / "dev" / "plugin-api-usage.json"
            ledger.parent.mkdir(parents=True)

            result, _ = run_main([corpus, "--write"], repo, ledger)
            written = json.loads(ledger.read_text(encoding="utf-8"))
            summary = plugin_api_usage.compatibility_summary(
                written, PROVIDED, WANTED, EMITTED)

        self.assertEqual(result, 0)
        self.assertEqual(written["plugins_scanned"], 2)
        self.assertEqual(written["plugins_reachability_unknown"], 1)
        incidence = {row["plugin"]: row for row in written["plugin_incidence"]}
        self.assertEqual(incidence["valid.jar"]["reachability_status"], "complete")
        self.assertEqual(incidence["corrupt.jar"]["reachability_status"], "unknown")
        self.assertEqual(
            incidence["corrupt.jar"]["archive_error"], "File is not a zip file")
        self.assertEqual(summary["ceiling"]["status"], "unknown")
        self.assertIsNone(summary["ceiling"]["plugins_on_public_api"])


class CompatibilityEvidence(unittest.TestCase):
    @unittest.skipUnless(
        (plugin_api_usage.REPO / "plugin-api/build/foton-plugin-api.jar").is_file(),
        "the repository API jar is built by dev/build-plugin-api.sh",
    )
    def test_repository_jni_roots_match_real_compiled_signatures_exactly(self):
        api = plugin_api_usage.REPO / "plugin-api/build/foton-plugin-api.jar"
        roots = plugin_api_usage._rust_jni_roots(
            plugin_api_usage.REPO / "foton-plugin/src")
        compiled = plugin_api_usage._compiled_method_signatures(api)

        for name, signature in (
            ("fireJoin", "(Ljava/lang/String;ILjava/lang/String;)Ljava/lang/String;"),
            ("firePreCreatureSpawn", "(Ljava/lang/String;DDDLjava/lang/String;Ljava/lang/String;)Z"),
            ("fireCreatureSpawn", "(Ljava/lang/String;Ljava/lang/String;DDDLjava/lang/String;)Z"),
        ):
            exact = ("foton/EventBridge", name, signature)
            self.assertIn(exact, roots)
            self.assertIn(exact, compiled)
        # The compatibility overload still exists, but the host must pass the login attempt.
        old_join = ("foton/EventBridge", "fireJoin",
                    "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;")
        self.assertIn(old_join, compiled)
        self.assertNotIn(old_join, roots)
        for name, signature in (
            ("firePreCreatureSpawn", "(Ljava/lang/String;DDDDLjava/lang/String;Ljava/lang/String;)Z"),
            ("fireCreatureSpawn", "(Ljava/lang/String;Ljava/lang/String;DDDDLjava/lang/String;)Z"),
        ):
            obsolete = ("foton/EventBridge", name, signature)
            self.assertNotIn(obsolete, roots)
            self.assertNotIn(obsolete, compiled)
        for mismatch in {
            (
                "foton/EventBridge",
                "fireBlockExp",
                "(Ljava/lang/String;IIILjava/lang/String;)Ljava/lang/String;",
            ),
            (
                "foton/EventBridge",
                "fireEntityPortal",
                "(Ljava/lang/String;Ljava/lang/String;DDDDLjava/lang/String;"
                "DDDDLjava/lang/String;)Ljava/lang/String;",
            ),
        }:
            self.assertIn(mismatch, roots)
            self.assertNotIn(mismatch, compiled)

    @unittest.skipUnless(
        (plugin_api_usage.REPO / "plugin-api/build/foton-plugin-api.jar").is_file(),
        "the repository API jar is built by dev/build-plugin-api.sh",
    )
    def test_repository_spawn_location_helper_has_two_exact_compiled_roots(self):
        api = plugin_api_usage.REPO / "plugin-api/build/foton-plugin-api.jar"
        roots = plugin_api_usage._rust_jni_roots(
            plugin_api_usage.REPO / "foton-plugin/src")
        compiled = plugin_api_usage._compiled_method_signatures(api)
        expected = {
            (
                "foton/EventBridge",
                "firePlayerRespawn",
                "(Ljava/lang/String;Ljava/lang/String;Z)Ljava/lang/String;",
            ),
            (
                "foton/EventBridge",
                "firePlayerSpawnLocation",
                "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
            ),
        }

        self.assertLessEqual(expected, roots)
        self.assertLessEqual(expected, compiled)

    def test_unlinked_java_fire_method_is_not_emitted(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            java = root / "java"
            rust = root / "rust"
            java.mkdir()
            rust.mkdir()
            (java / "EventBridge.java").write_text(
                JAVA_BRIDGE_WITH_ORPHAN, encoding="utf-8")
            (rust / "forward.rs").write_text(RUST_WITHOUT_CALLER, encoding="utf-8")

            compiled = build_jar(
                {"foton/EventBridge.java": JAVA_BRIDGE_WITH_ORPHAN},
                root / "compiled",
            )

            emitted = plugin_api_usage.emitted_events(java, rust, compiled)

        self.assertIn("org/bukkit/event/world/WorldLoadEvent", emitted)
        self.assertNotIn("org/bukkit/event/world/WorldUnloadEvent", emitted)

    def test_same_name_with_wrong_jni_descriptor_is_not_emitted(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            java = root / "java"
            rust = root / "rust"
            java.mkdir()
            rust.mkdir()
            (java / "EventBridge.java").write_text(
                JAVA_BRIDGE_WITH_ORPHAN, encoding="utf-8")
            (rust / "forward.rs").write_text(
                RUST_WITH_WRONG_DESCRIPTOR, encoding="utf-8")
            compiled = build_jar(
                {"foton/EventBridge.java": JAVA_BRIDGE_WITH_ORPHAN},
                root / "compiled",
            )

            emitted = plugin_api_usage.emitted_events(java, rust, compiled)

        self.assertNotIn("org/bukkit/event/world/WorldLoadEvent", emitted)

    def test_exact_jni_owner_name_and_descriptor_is_emitted(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            java = root / "java"
            rust = root / "rust"
            java.mkdir()
            rust.mkdir()
            (java / "EventBridge.java").write_text(
                JAVA_BRIDGE_WITH_ORPHAN, encoding="utf-8")
            (rust / "forward.rs").write_text(RUST_WITHOUT_CALLER, encoding="utf-8")
            compiled = build_jar(
                {"foton/EventBridge.java": JAVA_BRIDGE_WITH_ORPHAN},
                root / "compiled",
            )

            emitted = plugin_api_usage.emitted_events(java, rust, compiled)

        self.assertIn("org/bukkit/event/world/WorldLoadEvent", emitted)

    def test_explicit_dynamic_helper_mapping_is_descriptor_validated(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            java = root / "java"
            rust = root / "rust"
            java.mkdir()
            rust.mkdir()
            (java / "EventBridge.java").write_text(
                JAVA_BRIDGE_WITH_ORPHAN, encoding="utf-8")
            (rust / "forward.rs").write_text(
                RUST_DYNAMIC_HELPER_CALL, encoding="utf-8")
            compiled = build_jar(
                {"foton/EventBridge.java": JAVA_BRIDGE_WITH_ORPHAN},
                root / "compiled",
            )

            emitted = plugin_api_usage.emitted_events(java, rust, compiled)

        self.assertIn("org/bukkit/event/world/WorldLoadEvent", emitted)

    def test_exact_java_overload_does_not_traverse_same_named_body(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            java = root / "java"
            rust = root / "rust"
            java.mkdir()
            rust.mkdir()
            (java / "EventBridge.java").write_text(
                JAVA_OVERLOADED_FIRE, encoding="utf-8")
            (rust / "forward.rs").write_text(
                RUST_OVERLOADED_FIRE, encoding="utf-8")
            compiled = build_jar(
                {"foton/EventBridge.java": JAVA_OVERLOADED_FIRE},
                root / "compiled",
            )

            emitted = plugin_api_usage.emitted_events(java, rust, compiled)

        self.assertEqual(
            emitted, {"org/bukkit/event/world/WorldLoadEvent"})

    def test_exact_java_graph_follows_lambda_implementation_handle(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            java = root / "java"
            rust = root / "rust"
            java.mkdir()
            rust.mkdir()
            (java / "PluginHost.java").write_text(
                JAVA_LAMBDA_EVENT, encoding="utf-8")
            (rust / "forward.rs").write_text(
                RUST_LAMBDA_EVENT, encoding="utf-8")
            compiled = build_jar(
                {"foton/PluginHost.java": JAVA_LAMBDA_EVENT},
                root / "compiled",
            )

            emitted = plugin_api_usage.emitted_events(java, rust, compiled)

        self.assertEqual(
            emitted, {"org/bukkit/event/server/PluginDisableEvent"})

    def test_jni_entry_points_follow_static_and_instance_java_paths(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            java = root / "java"
            rust = root / "rust"
            write_sources(java, JAVA_JNI_ENTRY_POINTS)
            write_sources(rust, {"bridge.rs": RUST_JNI_ENTRY_POINTS})

            compiled = build_jar(JAVA_JNI_ENTRY_POINTS, root / "compiled")

            emitted = plugin_api_usage.emitted_events(java, rust, compiled)

        self.assertEqual(
            emitted,
            {
                "org/bukkit/event/player/PlayerRegisterChannelEvent",
                "org/bukkit/event/player/PlayerUnregisterChannelEvent",
                "org/bukkit/event/server/PluginDisableEvent",
                "org/bukkit/event/server/PluginEnableEvent",
                "io/papermc/paper/plugin/lifecycle/event/registrar/ReloadableRegistrarEvent",
            },
        )

    def test_summary_keeps_shared_and_long_tail_counts_separate(self):
        summary = plugin_api_usage.compatibility_summary(
            LEDGER, PROVIDED, WANTED, EMITTED)

        self.assertEqual(summary["api"]["shared"]["resolved"], 1)
        self.assertEqual(summary["api"]["all"]["missing"], 1)
        self.assertEqual(summary["api"]["classes"]["all"]["missing"], 1)
        self.assertEqual(
            summary["api"]["classes"]["all"]["missing_classes"],
            ["org.bukkit.Server"],
        )

    def test_summary_expands_emitted_event_supertypes(self):
        wanted = {
            "org/bukkit/event/Event",
            "org/bukkit/event/world/WorldEvent",
            "org/bukkit/event/world/WorldUnloadEvent",
        }

        summary = plugin_api_usage.compatibility_summary(
            LEDGER,
            PROVIDED,
            wanted,
            {"org/bukkit/event/world/WorldUnloadEvent"},
            EVENT_PARENTS,
        )

        self.assertEqual(summary["events"]["emitted"], 3)
        self.assertEqual(summary["events"]["missing"], 0)

    def test_summary_marks_unrecorded_class_evidence_unknown(self):
        legacy = {
            key: value for key, value in LEDGER.items()
            if key not in {"api_classes", "api_classes_referenced"}
        }

        summary = plugin_api_usage.compatibility_summary(
            legacy, PROVIDED, WANTED, EMITTED)

        self.assertIsNone(summary["api"]["classes"]["all"]["referenced"])
        self.assertIsNone(summary["api"]["classes"]["shared"]["resolved"])

    def test_legacy_ledger_ceiling_is_historical_not_current(self):
        legacy = {
            key: value for key, value in LEDGER.items()
            if key not in {"schema_version", "plugin_incidence"}
        }

        summary = plugin_api_usage.compatibility_summary(
            legacy, PROVIDED, WANTED, EMITTED)

        self.assertEqual(summary["ceiling"]["status"], "unknown")
        self.assertIsNone(summary["ceiling"]["plugins_on_public_api"])
        self.assertIsNone(summary["ceiling"]["plugins_reaching_internals"])
        self.assertEqual(
            summary["ceiling"]["historical"],
            {
                "plugins_on_public_api": 2,
                "plugins_reaching_internals": 1,
                "plugins_scanned": 3,
            },
        )

    def test_optional_adapter_does_not_lower_current_ceiling(self):
        summary = plugin_api_usage.compatibility_summary(
            LEDGER, PROVIDED, WANTED, EMITTED)

        self.assertEqual(summary["ceiling"]["status"], "current")
        self.assertEqual(summary["ceiling"]["plugins_on_public_api"], 2)
        self.assertEqual(summary["ceiling"]["plugins_reaching_internals"], 1)
        self.assertEqual(summary["ceiling"]["plugins_with_optional_adapters"], 1)

    def test_v2_rows_without_archive_read_evidence_keep_ceiling_unknown(self):
        incomplete = json.loads(json.dumps(LEDGER))
        for row in incomplete["plugin_incidence"]:
            row.pop("archive_error", None)

        summary = plugin_api_usage.compatibility_summary(
            incomplete, PROVIDED, WANTED, EMITTED)

        self.assertEqual(summary["ceiling"]["status"], "unknown")
        self.assertIsNone(summary["ceiling"]["plugins_on_public_api"])

    def test_summary_json_is_stable_and_newline_terminated(self):
        with tempfile.TemporaryDirectory() as directory:
            output = pathlib.Path(directory) / "summary.json"
            summary = plugin_api_usage.compatibility_summary(
                LEDGER, PROVIDED, WANTED, EMITTED)

            plugin_api_usage.write_summary_json(output, summary)

            text = output.read_text(encoding="utf-8")
        self.assertEqual(
            text, json.dumps(summary, indent=2, sort_keys=True) + "\n")

    @unittest.skipIf(shutil.which("javac") is None, "javac is needed to build the fixture")
    def test_summary_json_cli_matches_human_event_coverage(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            repo = root / "repo"
            java = repo / "plugin-api" / "src"
            rust = repo / "foton-plugin" / "src"
            write_sources(
                java,
                {
                    "foton/EventBridge.java": """
                        package foton;
                        import org.bukkit.event.world.WorldUnloadEvent;
                        public final class EventBridge {
                            public static void fireWorldUnload() {
                                new WorldUnloadEvent();
                            }
                        }
                    """,
                },
            )
            write_sources(
                rust,
                {
                    "forward.rs": """
                        const BRIDGE: &str = "foton/EventBridge";
                        fn forward(env: &mut JNIEnv) {
                            env.call_static_method(
                                BRIDGE, "fireWorldUnload", "()V", &[]);
                        }
                    """,
                },
            )
            ledger = repo / "dev" / "plugin-api-usage.json"
            ledger.parent.mkdir(parents=True)
            ledger.write_text(
                json.dumps(
                    {
                        "plugins_scanned": 1,
                        "plugins_reaching_internals": 0,
                        "api_members_referenced": 0,
                        "api_members_kept_at_least": 2,
                        "api_members": [],
                        "api_classes_referenced": 2,
                        "api_classes": [
                            {"class": "org/bukkit/World", "plugins": 2},
                            {"class": "org/bukkit/Server", "plugins": 1},
                        ],
                        "events": [
                            {"event": "org/bukkit/event/Event", "plugins": 1},
                            {
                                "event": "org/bukkit/event/world/WorldEvent",
                                "plugins": 1,
                            },
                            {
                                "event": "org/bukkit/event/world/WorldUnloadEvent",
                                "plugins": 1,
                            },
                        ],
                    }
                ),
                encoding="utf-8",
            )
            stubs = build_jar({}, root / "api-stubs")
            bridge = build_jar(
                {
                    "foton/EventBridge.java": """
                        package foton;
                        import org.bukkit.event.world.WorldUnloadEvent;
                        public final class EventBridge {
                            public static void fireWorldUnload() {
                                new WorldUnloadEvent();
                            }
                        }
                    """,
                },
                root / "api-bridge",
            )
            api = merge_jars(root / "api.jar", stubs, bridge)
            summary_path = root / "summary.json"

            result, output = run_main(
                [
                    "--covered", api,
                    "--events", java,
                    "--summary-json", summary_path,
                ],
                repo,
                ledger,
            )
            summary = json.loads(summary_path.read_text(encoding="utf-8"))

        human_emitted = int(
            re.search(r"a listener could receive: (\d+)", output).group(1))
        self.assertIsNone(result)
        self.assertEqual(summary["events"]["emitted"], human_emitted)
        self.assertEqual(summary["events"]["emitted"], 3)
        self.assertEqual(summary["api"]["classes"]["all"]["missing"], 1)


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
        self.assertIn("getServerName()Ljava/lang/String;", members)
        self.assertIn("java/lang/Object", supertypes)

    def test_an_inherited_member_counts_as_provided(self):
        # A plugin calls `Child#method` and `Parent#method` interchangeably and
        # the JVM resolves both, so a jar that only listed declared members
        # would report a gap that does not exist.
        have = plugin_api_usage.provided(self.complete)

        self.assertIn(
            "sendMessage(Ljava/lang/String;)V", have["org/bukkit/entity/Player"])
        self.assertIn("getName()Ljava/lang/String;", have["org/bukkit/entity/Player"],
                      "a member inherited from a supertype should still resolve")

    def test_what_java_lang_object_gives_every_class_is_not_a_gap(self):
        # `player.toString()` compiles to a reference on Player, and the Object
        # class file is not in the API jar. Counting that as missing would put
        # a phantom member near the top of the ranking.
        corpus = self.plugin.parent
        _, missing = plugin_api_usage.gaps(corpus, self.complete)

        self.assertNotIn(
            "org/bukkit/entity/Player#toString()Ljava/lang/String;", missing)

    def test_what_an_enum_gives_its_constants_is_not_a_gap(self):
        # `Material.name()` comes from java.lang.Enum, whose class file is not
        # in the API jar. Counting it missing put a member every plugin
        # "needs" at the top of the ranking, pointing the work at nothing.
        have = plugin_api_usage.provided(self.complete)

        self.assertIn("name()Ljava/lang/String;", have["org/bukkit/Season"])
        self.assertIn("toString()Ljava/lang/String;", have["org/bukkit/Season"])

    def test_a_jar_that_answers_everything_leaves_no_gap(self):
        corpus = self.plugin.parent
        per_plugin, missing = plugin_api_usage.gaps(corpus, self.complete)

        self.assertEqual(per_plugin[self.plugin.name][1], set())
        self.assertEqual(missing, collections.Counter())

    def test_a_missing_member_is_reported_against_the_plugins_that_call_it(self):
        corpus = self.plugin.parent
        _, missing = plugin_api_usage.gaps(corpus, self.partial)

        self.assertEqual(
            missing["org/bukkit/Bukkit#getServerName()Ljava/lang/String;"], 1)


class ConstantPool(unittest.TestCase):
    def test_bytes_that_are_not_a_class_file_are_refused(self):
        with self.assertRaises(plugin_api_usage.NotAClassFile):
            plugin_api_usage.constant_pool(b"PK\\x03\\x04 this is a zip, not a class")


if __name__ == "__main__":
    unittest.main()
