#!/usr/bin/env python3
"""Would this plugin jar link against Foton, and if not, exactly where not.

`plugin_api_usage.py --gap` answers "is the member there". The JVM asks more
than that, and each thing it asks that the ledger does not is a way for a
plugin to compile against Paper, load on Foton, and die later:

- **The kind of the owner.** Paper's `Attribute` is an interface, so a plugin
  calls `Attribute.getKey()` with `invokeinterface`. If Foton declares it an
  enum, the member exists and the call still throws
  `IncompatibleClassChangeError` -- the gap ledger counts it as covered.
- **Types that are named but never called.** `instanceof ItemDisplay`, a
  `catch`, and above all an event type in a listener method's signature.
  Bukkit reflects over a listener's declared methods, so one missing event
  class costs the plugin *every* handler in that listener, including the ones
  for events that exist.
- **Adventure, Gson, Guava and the rest.** A Paper server provides them, so a
  plugin does not ship them; the ledger only watches `org/bukkit` and friends.
- **What the plugin implements.** A plugin class implementing an API interface
  that Foton widened with a new abstract method throws `AbstractMethodError`
  the first time the server calls it.

    python3 dev/plugin_link_check.py Some.jar [Other.jar ...]
    python3 dev/plugin_link_check.py Some.jar --provided-by packetevents.jar

Exits non-zero when anything a server must provide is missing. A reference into
another plugin (`--provided-by` not given) is listed apart and does not fail
the check: that is a dependency to install, not API to write.

Standard library only, like the rest of `dev/`.
"""

import argparse
import collections
import pathlib
import subprocess
import struct
import sys
import tempfile
import zipfile

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import plugin_api_usage as usage  # noqa: E402

REPO = pathlib.Path(__file__).resolve().parent.parent
DEFAULT_API = REPO / "plugin-api" / "build" / "foton-plugin-api.jar"
DEFAULT_LIBS = REPO / "plugin-api" / "lib"
JDK_CACHE = REPO / "plugin-api" / "build" / "jdk-classes.jar"

# The runtime's own classes, copied out of jrt:/ once. Without them a member a
# plugin reaches through a JDK supertype -- `ItemMeta.clone()` through
# Cloneable, `Registry.forEach` through Iterable -- cannot be judged, and a
# check that cannot judge must not accuse, so it passed. Missing members hid
# behind that until a stricter reading found them.
JDK_DUMP = """
import java.net.URI;
import java.nio.file.*;
import java.util.zip.*;

public class JdkDump {
    public static void main(String[] args) throws Exception {
        FileSystem jrt = FileSystems.getFileSystem(URI.create("jrt:/"));
        try (ZipOutputStream out = new ZipOutputStream(Files.newOutputStream(Path.of(args[0])))) {
            out.putNextEntry(new ZipEntry("META-INF/jdk-version"));
            out.write(System.getProperty("java.version").getBytes());
            for (String module : new String[] {"java.base", "java.logging", "java.sql", "java.desktop",
                                               "java.net.http", "java.management", "java.xml"}) {
                Path root = jrt.getPath("/modules", module);
                if (!Files.exists(root)) continue;
                try (var files = Files.walk(root)) {
                    for (Path file : (Iterable<Path>) files::iterator) {
                        String name = root.relativize(file).toString();
                        if (!name.endsWith(".class") || name.equals("module-info.class")) continue;
                        out.putNextEntry(new ZipEntry(name));
                        out.write(Files.readAllBytes(file));
                    }
                }
            }
        }
    }
}
"""


def jdk_classes():
    """The JDK's classes as a layer, extracted once and cached beside the API jar."""
    if not JDK_CACHE.is_file():
        JDK_CACHE.parent.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory() as work:
            source = pathlib.Path(work) / "JdkDump.java"
            source.write_text(JDK_DUMP)
            partial = JDK_CACHE.with_suffix(".part")
            subprocess.run(["java", str(source), str(partial)], check=True,
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            partial.replace(JDK_CACHE)
    return read_jar(JDK_CACHE)

ACC_STATIC = 0x0008
ACC_INTERFACE = 0x0200
ACC_ABSTRACT = 0x0400

# What a Paper server puts on a plugin's class path without the plugin asking.
# A reference into one of these that nothing resolves is a hole in Foton; a
# reference anywhere else is somebody else's jar.
SERVER_PROVIDED = (
    "org/bukkit/",
    "io/papermc/paper/",
    "com/destroystokyo/paper/",
    "org/spigotmc/",
    "net/kyori/",
    "com/mojang/brigadier/",
    "com/google/gson/",
    "com/google/common/",
    "org/yaml/snakeyaml/",
    "org/joml/",
    "org/slf4j/",
    "net/md_5/bungee/",
)

# Never resolvable by any reimplementation, and counted apart for that reason.
INTERNAL = ("net/minecraft/", "org/bukkit/craftbukkit/")

# The JDK's own packages, which every runtime answers for.
JDK = ("java/", "javax/", "jdk/", "sun/", "com/sun/", "org/w3c/", "org/xml/", "org/ietf/")


class ClassInfo:
    """One class file, read far enough to answer linkage questions."""

    __slots__ = ("name", "flags", "super", "interfaces", "members", "refs", "classes")

    def __init__(self, name, flags, super_name, interfaces, members, refs, classes):
        self.name = name
        self.flags = flags
        self.super = super_name
        self.interfaces = interfaces
        # {name+descriptor: access flags}
        self.members = members
        # [(tag, owner, name, descriptor)]
        self.refs = refs
        # every class name the constant pool or a declared signature mentions
        self.classes = classes

    @property
    def is_interface(self):
        return bool(self.flags & ACC_INTERFACE)


def descriptor_classes(descriptor):
    """The class names inside a field or method descriptor."""
    names = []
    index = 0
    while index < len(descriptor):
        if descriptor[index] == "L":
            end = descriptor.index(";", index)
            names.append(descriptor[index + 1:end])
            index = end + 1
        else:
            index += 1
    return names


def element_class(name):
    """`[[Lorg/bukkit/Foo;` is still a reference to `org/bukkit/Foo`."""
    if not name.startswith("["):
        return name
    stripped = name.lstrip("[")
    if stripped.startswith("L") and stripped.endswith(";"):
        return stripped[1:-1]
    return None


def read_class(data):
    pool, offset = usage._read_pool(data)
    flags, this_index, super_index, interface_count = struct.unpack_from(">HHHH", data, offset)
    offset += 8
    interfaces = []
    for _ in range(interface_count):
        interfaces.append(usage._class_name(pool, struct.unpack_from(">H", data, offset)[0]))
        offset += 2

    members = {}
    classes = set()
    for _ in range(2):
        count = struct.unpack_from(">H", data, offset)[0]
        offset += 2
        for _ in range(count):
            access, name_index, descriptor_index = struct.unpack_from(">HHH", data, offset)
            name = usage._utf8(pool, name_index)
            descriptor = usage._utf8(pool, descriptor_index)
            if name and descriptor:
                members[f"{name}{descriptor}"] = access
                classes.update(descriptor_classes(descriptor))
            offset += 6
            attributes = struct.unpack_from(">H", data, offset)[0]
            offset += 2
            for _ in range(attributes):
                length = struct.unpack_from(">I", data, offset + 2)[0]
                offset += 6 + length

    refs = []
    for tag, payload in pool.values():
        if tag == usage.TAG_CLASS:
            name = usage._utf8(pool, struct.unpack(">H", payload)[0])
            element = element_class(name) if name else None
            if element:
                classes.add(element)
            continue
        if tag not in (usage.TAG_FIELDREF, usage.TAG_METHODREF, usage.TAG_INTERFACE_METHODREF):
            continue
        class_index, name_and_type_index = struct.unpack(">HH", payload)
        owner = usage._class_name(pool, class_index)
        entry = pool.get(name_and_type_index)
        if not owner or not entry:
            continue
        name_index, descriptor_index = struct.unpack(">HH", entry[1])
        name = usage._utf8(pool, name_index)
        descriptor = usage._utf8(pool, descriptor_index)
        if not name or not descriptor:
            continue
        owner = element_class(owner)
        if owner is None:
            continue
        refs.append((tag, owner, name, descriptor))
        classes.update(descriptor_classes(descriptor))

    super_name = usage._class_name(pool, super_index) if super_index else None
    for supertype in [super_name, *interfaces]:
        if supertype:
            classes.add(supertype)
    return ClassInfo(
        usage._class_name(pool, this_index),
        flags,
        super_name,
        [i for i in interfaces if i],
        members,
        refs,
        classes,
    )


def read_jar(path):
    classes = {}
    with zipfile.ZipFile(path) as archive:
        for entry in archive.namelist():
            if not entry.endswith(".class") or entry.startswith("META-INF/"):
                continue
            try:
                info = read_class(archive.read(entry))
            except (usage.NotAClassFile, struct.error, KeyError, IndexError, ValueError):
                continue
            if info.name:
                classes.setdefault(info.name, info)
    return classes


class World:
    """Every class the JVM could find, and the rules it finds members by."""

    def __init__(self, layers):
        self.classes = {}
        for layer in layers:
            for name, info in layer.items():
                self.classes.setdefault(name, info)

    def find(self, name):
        return self.classes.get(name)

    def _jdk_has(self, name, signature):
        """A JDK class missing from the extracted set; only the common members are known."""
        known = usage.FROM_JDK.get(name)
        if known is None:
            return None  # unknown: cannot say, so do not accuse
        return signature in known

    def field(self, owner, signature, seen=None):
        seen = seen if seen is not None else set()
        if owner in seen:
            return False
        seen.add(owner)
        info = self.find(owner)
        if info is None:
            # No JDK supertype of an API class declares a public field a
            # plugin would read, so a field not found by here is absent.
            return False
        if signature in info.members:
            return True
        return any(
            self.field(parent, signature, seen)
            for parent in [*info.interfaces, info.super]
            if parent
        )

    def method(self, owner, signature, seen=None):
        seen = seen if seen is not None else set()
        if owner in seen:
            return False
        seen.add(owner)
        info = self.find(owner)
        if info is None:
            if owner.startswith("java/"):
                return self._jdk_has(owner, signature)
            return False
        if signature in info.members:
            return True
        parents = [info.super, *info.interfaces] if info.super else list(info.interfaces)
        if info.is_interface and "java/lang/Object" not in parents:
            parents.append("java/lang/Object")
        unknown = False
        for parent in parents:
            found = self.method(parent, signature, seen)
            if found:
                return True
            if found is None:
                unknown = True
        return None if unknown else False

    def abstract_methods(self, name, seen=None):
        """Abstract methods an implementor of `name` must provide."""
        seen = seen if seen is not None else set()
        if name in seen:
            return {}
        seen.add(name)
        info = self.find(name)
        if info is None:
            return {}
        out = {}
        for parent in [*info.interfaces, info.super]:
            if parent:
                out.update(self.abstract_methods(parent, seen))
        for signature, access in info.members.items():
            if "(" not in signature or signature.startswith("<"):
                continue
            if access & ACC_STATIC:
                continue
            if access & ACC_ABSTRACT:
                out.setdefault(signature, name)
            else:
                out.pop(signature, None)
        return out

    def concrete(self, name, signature, seen=None):
        """Whether `name` or an ancestor supplies a body for `signature`."""
        seen = seen if seen is not None else set()
        if name in seen:
            return False
        seen.add(name)
        info = self.find(name)
        if info is None:
            return name.startswith("java/")  # the JDK is taken on trust
        access = info.members.get(signature)
        if access is not None and not access & ACC_ABSTRACT:
            return True
        for parent in [info.super, *info.interfaces]:
            if parent and self.concrete(parent, signature, seen):
                return True
        return False


def served(name):
    return name.startswith(SERVER_PROVIDED) and not name.startswith(INTERNAL)


def check(plugin_path, world, plugin_classes):
    problems = collections.defaultdict(set)
    external = collections.Counter()
    internal = set()
    for info in plugin_classes.values():
        for name in info.classes:
            if name.startswith(INTERNAL):
                internal.add(name)
                continue
            if world.find(name) or name.startswith(JDK):
                continue
            if served(name):
                problems["missing class"].add(f"{name}  (named by {info.name})")
            else:
                external[name.rsplit("/", 1)[0]] += 1

        for tag, owner, member, descriptor in info.refs:
            if owner in plugin_classes or owner.startswith(INTERNAL) or not served(owner):
                continue
            target = world.find(owner)
            if target is None:
                continue  # already reported as a missing class
            signature = f"{member}{descriptor}"
            if tag == usage.TAG_INTERFACE_METHODREF and not target.is_interface:
                problems["called as an interface, declared a class"].add(f"{owner}#{signature}")
            if tag == usage.TAG_METHODREF and target.is_interface:
                problems["called as a class, declared an interface"].add(f"{owner}#{signature}")
            if tag == usage.TAG_FIELDREF:
                found = world.field(owner, signature)
            else:
                found = world.method(owner, signature)
            if found is False:
                problems["missing member"].add(f"{owner}#{signature}")

        # An abstract class, an interface, or an enum whose constants each
        # supply the body (javac marks that enum abstract) owes nothing here.
        implementors = [] if info.flags & (ACC_ABSTRACT | ACC_INTERFACE) else [info.super, *info.interfaces]
        for parent in implementors:
            if not parent or not served(parent):
                continue
            target = world.find(parent)
            if target is None:
                continue
            for signature, declarer in world.abstract_methods(parent).items():
                if not world.concrete(info.name, signature):
                    problems["abstract method the plugin does not implement"].add(
                        f"{info.name} <- {declarer}#{signature}"
                    )
    return problems, external, internal


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("plugins", nargs="+", type=pathlib.Path)
    parser.add_argument("--api", type=pathlib.Path, default=DEFAULT_API)
    parser.add_argument("--libs", type=pathlib.Path, default=DEFAULT_LIBS)
    parser.add_argument(
        "--provided-by",
        type=pathlib.Path,
        action="append",
        default=[],
        help="another plugin jar installed beside this one",
    )
    args = parser.parse_args()

    if not args.api.is_file():
        raise SystemExit(f"no API jar at {args.api}; run dev/build-plugin-api.sh")
    layers = [read_jar(args.api), jdk_classes()]
    layers += [read_jar(jar) for jar in sorted(args.libs.glob("*.jar"))]
    layers += [read_jar(jar) for jar in args.provided_by]

    failed = False
    for plugin in args.plugins:
        plugin_classes = read_jar(plugin)
        # The plugin's own classes come first: a shaded Gson answers for itself.
        world = World([plugin_classes, *layers])
        problems, external, internal = check(plugin, world, plugin_classes)
        total = sum(len(v) for v in problems.values())
        print(f"{plugin.name}: {total} linkage problem(s)")
        for kind in sorted(problems):
            print(f"  {kind} ({len(problems[kind])})")
            for line in sorted(problems[kind]):
                print(f"    {line}")
        if internal:
            print(f"  reaches server internals ({len(internal)}), never resolvable:")
            for name in sorted(internal):
                print(f"    {name}")
        if external:
            print("  other plugins or libraries it expects, by package:")
            for package, count in sorted(external.items()):
                print(f"    {package} ({count})")
        failed |= total > 0
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
