"""Compile actual Java bytecode to exercise the plugin linkage diagnostic."""

import pathlib
import os
import shutil
import struct
import subprocess
import sys
import tempfile
import unittest
import warnings
import zipfile

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import plugin_link_check as linkage


def compile_jar(root, label, sources, classpath=None):
    source_root = root / label / "src"
    classes = root / label / "classes"
    classes.mkdir(parents=True)
    paths = []
    for name, body in sources.items():
        path = source_root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(body, encoding="utf-8")
        paths.append(str(path))
    command = ["javac", "-nowarn", "-d", str(classes)]
    if classpath:
        command += ["-cp", str(classpath)]
    subprocess.run(command + paths, check=True, capture_output=True)
    jar = root / f"{label}.jar"
    with zipfile.ZipFile(jar, "w") as archive:
        for path in classes.rglob("*.class"):
            archive.write(path, path.relative_to(classes).as_posix())
    return jar, classes


@unittest.skipIf(shutil.which("javac") is None, "javac is required")
class LinkageCheck(unittest.TestCase):
    def test_private_replacement_and_static_kind_are_linkage_errors(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            _, paper_classes = compile_jar(root, "paper", {
                "org/bukkit/Thing.java": (
                    "package org.bukkit; public class Thing { "
                    "public void run() {} public static void start() {} }"
                ),
            })
            plugin, _ = compile_jar(root, "plugin", {
                "example/Plugin.java": (
                    "package example; public class Plugin { "
                    "public void callPrivate(org.bukkit.Thing thing) { thing.run(); } "
                    "public void callStatic() { org.bukkit.Thing.start(); } "
                    "public static void main(String[] args) { "
                    "Plugin plugin = new Plugin(); "
                    "try { plugin.callPrivate(new org.bukkit.Thing()); "
                    "throw new AssertionError(\"private call unexpectedly linked\"); "
                    "} catch (IllegalAccessError expected) {} "
                    "try { plugin.callStatic(); "
                    "throw new AssertionError(\"static call unexpectedly linked\"); "
                    "} catch (IncompatibleClassChangeError expected) {} "
                    "} }"
                ),
            }, paper_classes)
            foton, _ = compile_jar(root, "foton", {
                "org/bukkit/Thing.java": (
                    "package org.bukkit; public class Thing { "
                    "private void run() {} public void start() {} }"
                ),
            })
            classes = linkage.read_jar(plugin)
            problems, _, _ = linkage.check(
                plugin, linkage.World([classes, linkage.read_jar(foton)]), classes,
            )
            self.assertIn(
                "org/bukkit/Thing#run()V  (called by example/Plugin)",
                problems["inaccessible member"],
            )
            self.assertIn("org/bukkit/Thing#start()V", problems["wrong static kind"])
            if shutil.which("java"):
                subprocess.run(
                    ["java", "-cp", os.pathsep.join((str(root / "foton.jar"), str(plugin))),
                     "example.Plugin"],
                    check=True, capture_output=True,
                )

    def test_method_handle_static_kind_is_checked(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            _, paper_classes = compile_jar(root, "paper", {
                "org/bukkit/Thing.java": "package org.bukkit; public class Thing { public static void start() {} }",
            })
            plugin, _ = compile_jar(root, "plugin", {
                "example/Plugin.java": (
                    "package example; public class Plugin { "
                    "public Runnable callback() { return org.bukkit.Thing::start; } }"
                ),
            }, paper_classes)
            foton, _ = compile_jar(root, "foton", {
                "org/bukkit/Thing.java": "package org.bukkit; public class Thing { public void start() {} }",
            })
            own = linkage.read_jar(plugin)
            problems, _, _ = linkage.check(plugin, linkage.World([own, linkage.read_jar(foton)]), own)
            self.assertIn("org/bukkit/Thing#start()V", problems["wrong static kind"])

    def test_field_getstatic_and_getfield_visibility(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            _, paper_classes = compile_jar(root, "paper", {
                "org/bukkit/Thing.java": (
                    "package org.bukkit; public class Thing { "
                    "public static int number = 1; public int value = 2; }"
                ),
            })
            plugin, _ = compile_jar(root, "plugin", {
                "example/Plugin.java": (
                    "package example; public class Plugin { "
                    "public int read(org.bukkit.Thing thing) { "
                    "return org.bukkit.Thing.number + thing.value; } }"
                ),
            }, paper_classes)
            foton, _ = compile_jar(root, "foton", {
                "org/bukkit/Thing.java": (
                    "package org.bukkit; public class Thing { "
                    "public int number = 1; private int value = 2; }"
                ),
            })
            own = linkage.read_jar(plugin)
            problems, _, _ = linkage.check(plugin, linkage.World([own, linkage.read_jar(foton)]), own)
            self.assertIn("org/bukkit/Thing#numberI", problems["wrong static kind"])
            self.assertTrue(any("Thing#valueI" in value for value in problems["inaccessible member"]))

    def test_class_package_and_protected_visibility(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            _, paper_classes = compile_jar(root, "paper", {
                "org/bukkit/Thing.java": (
                    "package org.bukkit; public class Thing { "
                    "public void packageCall() {} public void protectedCall() {} }"
                ),
                "org/bukkit/Visible.java": "package org.bukkit; public class Visible { public void ping() {} }",
            })
            plugin, _ = compile_jar(root, "plugin", {
                "example/Plugin.java": (
                    "package example; public class Plugin { public void call("
                    "org.bukkit.Thing thing, org.bukkit.Visible visible) { "
                    "thing.packageCall(); thing.protectedCall(); visible.ping(); } }"
                ),
            }, paper_classes)
            foton, _ = compile_jar(root, "foton", {
                "org/bukkit/Thing.java": (
                    "package org.bukkit; public class Thing { "
                    "void packageCall() {} protected void protectedCall() {} }"
                ),
                "org/bukkit/Visible.java": "package org.bukkit; class Visible { public void ping() {} }",
            })
            own = linkage.read_jar(plugin)
            problems, _, _ = linkage.check(plugin, linkage.World([own, linkage.read_jar(foton)]), own)
            self.assertTrue(any("packageCall" in value for value in problems["inaccessible member"]))
            self.assertTrue(any("protectedCall" in value for value in problems["inaccessible member"]))
            self.assertTrue(any("Visible#ping" in value for value in problems["inaccessible class"]))

    def test_switch_operands_do_not_hide_or_fabricate_calls(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            _, paper_classes = compile_jar(root, "paper", {
                "org/bukkit/Thing.java": "package org.bukkit; public class Thing { public static void start() {} }",
            })
            plugin, _ = compile_jar(root, "plugin", {
                "example/Plugin.java": (
                    "package example; public class Plugin { public void call(int value) { "
                    "switch (value) { case 1: case 2: org.bukkit.Thing.start(); break; "
                    "default: break; } } }"
                ),
            }, paper_classes)
            foton, _ = compile_jar(root, "foton", {
                "org/bukkit/Thing.java": "package org.bukkit; public class Thing { public void start() {} }",
            })
            own = linkage.read_jar(plugin)
            problems, _, _ = linkage.check(plugin, linkage.World([own, linkage.read_jar(foton)]), own)
            self.assertIn("org/bukkit/Thing#start()V", problems["wrong static kind"])

    def test_concrete_descendant_but_abstract_plugin_base_is_valid(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            api, classes = compile_jar(root, "api", {
                "org/bukkit/Contract.java": "package org.bukkit; public interface Contract { void run(); }",
            })
            plugin, _ = compile_jar(root, "plugin", {
                "example/Base.java": "package example; public abstract class Base implements org.bukkit.Contract {}",
                "example/Impl.java": "package example; public final class Impl extends Base { public void run() {} }",
            }, classes)
            own = linkage.read_jar(plugin)
            problems, _, _ = linkage.check(plugin, linkage.World([own, linkage.read_jar(api)]), own)
            self.assertFalse(problems["abstract method the plugin does not implement"])

    def test_multi_release_variant_for_java_25_overrides_clean_base(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            base_jar, base_api = compile_jar(root, "base_api", {
                "org/bukkit/Thing.java": "package org.bukkit; public class Thing { public void run() {} }",
            })
            _, variant_api = compile_jar(root, "variant_api", {
                "org/bukkit/Thing.java": "package org.bukkit; public class Thing { public void run() {} public void newCall() {} }",
            })
            base, _ = compile_jar(root, "base", {
                "example/Plugin.java": "package example; public class Plugin { public void call(org.bukkit.Thing x) { x.run(); } }",
            }, base_api)
            variant, _ = compile_jar(root, "variant", {
                "example/Plugin.java": "package example; public class Plugin { public void call(org.bukkit.Thing x) { x.newCall(); } }",
            }, variant_api)
            multi = root / "multi.jar"
            with zipfile.ZipFile(base) as b, zipfile.ZipFile(variant) as v, zipfile.ZipFile(multi, "w") as out:
                out.writestr("META-INF/MANIFEST.MF", "Manifest-Version: 1.0\r\nMulti-Release: true\r\n\r\n")
                out.writestr("example/Plugin.class", b.read("example/Plugin.class"))
                out.writestr("META-INF/versions/25/example/Plugin.class", v.read("example/Plugin.class"))
            own = linkage.read_jar(multi, java_version=25)
            problems, _, _ = linkage.check(multi, linkage.World([own, linkage.read_jar(base_jar)]), own)
            self.assertIn("org/bukkit/Thing#newCall()V", problems["missing member"])
            java_21 = linkage.read_jar(multi, java_version=21)
            problems_21, _, _ = linkage.check(
                multi, linkage.World([java_21, linkage.read_jar(base_jar)]), java_21,
            )
            self.assertNotIn("org/bukkit/Thing#newCall()V", problems_21["missing member"])

    def test_oversized_class_metadata_rejected_before_decompression(self):
        with self.assertRaisesRegex(ValueError, "JAR exceeds"):
            linkage.check_jar_entry_count(linkage.MAX_JAR_ENTRIES + 1)
        synthetic = zipfile.ZipInfo("example/Bomb.class")
        synthetic.file_size = linkage.MAX_CLASS_SIZE + 1
        synthetic.compress_size = 1
        with self.assertRaisesRegex(ValueError, "class exceeds"):
            linkage.check_entry_limits(synthetic, 0)
        synthetic.file_size = 201
        with self.assertRaisesRegex(ValueError, "compression ratio"):
            linkage.check_entry_limits(synthetic, 0)
        synthetic.file_size = 2
        synthetic.compress_size = 2
        with self.assertRaisesRegex(ValueError, "total decompressed"):
            linkage.check_entry_limits(synthetic, linkage.MAX_TOTAL_CLASS_BYTES - 1)

    def test_javax_inject_is_server_provided_but_javax_sql_is_jdk(self):
        self.assertTrue(linkage.served("javax/inject/Provider"))
        self.assertFalse(linkage.is_jdk("javax/inject/Provider"))
        self.assertTrue(linkage.is_jdk("javax/sql/DataSource"))

    def test_missing_types_members_and_interface_kind(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            paper, classes = compile_jar(root, "paper", {
                "org/bukkit/Thing.java": "package org.bukkit; public interface Thing { void run(); }",
                "org/bukkit/Absent.java": "package org.bukkit; public class Absent {}",
            })
            plugin, _ = compile_jar(root, "plugin", {
                "example/Plugin.java": (
                    "package example; import org.bukkit.*; "
                    "public class Plugin { public Absent declared(Thing thing) { "
                    "thing.run(); return null; } }"
                ),
            }, classes)
            foton, _ = compile_jar(root, "foton", {
                "org/bukkit/Thing.java": "package org.bukkit; public class Thing {}",
            })
            problems, external, internal = linkage.check(
                plugin, linkage.World([linkage.read_jar(plugin), linkage.read_jar(foton)]),
                linkage.read_jar(plugin),
            )
            self.assertIn("org/bukkit/Absent  (named by example/Plugin)", problems["missing class"])
            self.assertIn("org/bukkit/Thing#run()V", problems["missing member"])
            self.assertIn("org/bukkit/Thing#run()V", problems["called as an interface, declared a class"])
            self.assertFalse(external)
            self.assertFalse(internal)
            self.assertTrue(paper.is_file())

    def test_abstract_method_and_external_dependency_are_separate(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            _, paper_classes = compile_jar(root, "paper", {
                "org/bukkit/Contract.java": "package org.bukkit; public interface Contract { void oldMethod(); }",
                "other/Optional.java": "package other; public interface Optional {}",
            })
            plugin, _ = compile_jar(root, "plugin", {
                "example/Plugin.java": (
                    "package example; public class Plugin implements org.bukkit.Contract { "
                    "public void oldMethod() {} public other.Optional optional() { return null; } }"
                ),
            }, paper_classes)
            foton, _ = compile_jar(root, "foton", {
                "org/bukkit/Contract.java": (
                    "package org.bukkit; public interface Contract { "
                    "void oldMethod(); void addedMethod(); }"
                ),
            })
            classes = linkage.read_jar(plugin)
            problems, external, internal = linkage.check(
                plugin, linkage.World([classes, linkage.read_jar(foton)]), classes,
            )
            self.assertIn(
                "example/Plugin <- org/bukkit/Contract#addedMethod()V",
                problems["abstract method the plugin does not implement"],
            )
            self.assertEqual(external["other"], 1)
            self.assertFalse(internal)
            provider, _ = compile_jar(root, "provider", {
                "other/Optional.java": "package other; public interface Optional {}",
            })
            _, with_provider, _ = linkage.check(
                plugin,
                linkage.World([classes, linkage.read_jar(foton), linkage.read_jar(provider)]),
                classes,
            )
            self.assertFalse(with_provider)

    def test_corrupt_class_fails_closed(self):
        with tempfile.TemporaryDirectory() as temporary:
            jar = pathlib.Path(temporary) / "broken.jar"
            with zipfile.ZipFile(jar, "w") as archive:
                archive.writestr("org/bukkit/Broken.class", b"not a class")
            with self.assertRaisesRegex(ValueError, "unreadable class org/bukkit/Broken.class"):
                linkage.read_jar(jar)

    def test_class_name_does_not_match_archive_entry(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            jar, _ = compile_jar(root, "valid", {
                "org/bukkit/Valid.java": "package org.bukkit; public class Valid {}",
            })
            renamed = root / "renamed.jar"
            with zipfile.ZipFile(jar) as source, zipfile.ZipFile(renamed, "w") as target:
                target.writestr("org/bukkit/Other.class", source.read("org/bukkit/Valid.class"))
            with self.assertRaisesRegex(ValueError, "does not match entry"):
                linkage.read_jar(renamed)

    def test_duplicate_entries_and_malformed_version_paths_fail_closed(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            duplicate = root / "duplicate.jar"
            with warnings.catch_warnings():
                warnings.simplefilter("ignore", UserWarning)
                with zipfile.ZipFile(duplicate, "w") as archive:
                    archive.writestr("example/Plugin.class", b"first")
                    archive.writestr("example/Plugin.class", b"second")
            with self.assertRaisesRegex(ValueError, "duplicate archive entry"):
                linkage.read_jar(duplicate)
            malformed = root / "malformed.jar"
            with zipfile.ZipFile(malformed, "w") as archive:
                archive.writestr("META-INF/MANIFEST.MF", "Manifest-Version: 1.0\r\nMulti-Release: true\r\n\r\n")
                archive.writestr("META-INF/versions/nope/example/Plugin.class", b"not a class")
            with self.assertRaisesRegex(ValueError, "malformed multi-release"):
                linkage.read_jar(malformed)

    def test_zip_entry_count_is_rejected_before_central_directory_parse(self):
        with tempfile.TemporaryDirectory() as temporary:
            jar = pathlib.Path(temporary) / "many.jar"
            with zipfile.ZipFile(jar, "w"):
                pass
            data = bytearray(jar.read_bytes())
            struct.pack_into("<H", data, 8, linkage.MAX_JAR_ENTRIES + 1)
            struct.pack_into("<H", data, 10, linkage.MAX_JAR_ENTRIES + 1)
            jar.write_bytes(data)
            with self.assertRaisesRegex(ValueError, "JAR exceeds"):
                linkage.preflight_archive(jar)


if __name__ == "__main__":
    unittest.main()
