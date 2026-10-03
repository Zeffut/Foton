"""Compile actual Java bytecode to exercise the plugin linkage diagnostic."""

import pathlib
import shutil
import subprocess
import sys
import tempfile
import unittest
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


if __name__ == "__main__":
    unittest.main()
