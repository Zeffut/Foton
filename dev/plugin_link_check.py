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
    python3 dev/plugin_link_check.py Some.jar --java-version 21

Exits non-zero for detected mismatches in the known server-provided packages.
This is a conservative static triage tool, not JVM verification: it checks
direct bytecode access opcodes, but not reflection, service loading, or
optional dependency guards. References outside the known server packages are
listed separately
and require a real installed-dependency test before certification.

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
ACC_PUBLIC = 0x0001
ACC_PRIVATE = 0x0002
ACC_PROTECTED = 0x0004
ACC_INTERFACE = 0x0200
ACC_ABSTRACT = 0x0400
JAVA_VERSION = 25
MAX_JAR_ENTRIES = 50_000
MAX_JAR_BYTES = 256 * 1024 * 1024
MAX_CENTRAL_DIRECTORY_BYTES = 32 * 1024 * 1024
MAX_CLASS_COUNT = 50_000
MAX_CLASS_SIZE = 16 * 1024 * 1024
MAX_TOTAL_CLASS_BYTES = 512 * 1024 * 1024
MAX_COMPRESSION_RATIO = 200

ACCESS_OPCODES = {0xb2, 0xb3, 0xb4, 0xb5, 0xb6, 0xb7, 0xb8, 0xb9}
STATIC_OPCODES = {0xb2, 0xb3, 0xb8}
FIELD_OPCODES = {0xb2, 0xb3, 0xb4, 0xb5}
ONE_OPERAND = {0x10, 0x12, 0xa9, 0xbc, *range(0x15, 0x1a), *range(0x36, 0x3b)}
TWO_OPERANDS = {
    0x11, 0x13, 0x14, 0x84, *range(0x99, 0xa9), *range(0xb2, 0xb9),
    0xbb, 0xbd, 0xc0, 0xc1, 0xc6, 0xc7,
}
THREE_OPERANDS = {0xc5}
FOUR_OPERANDS = {0xb9, 0xba, 0xc8, 0xc9}
HANDLE_OPCODE = {
    1: 0xb4, 2: 0xb2, 3: 0xb5, 4: 0xb3,
    5: 0xb6, 6: 0xb8, 7: 0xb7, 8: 0xb7, 9: 0xb9,
}

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
    "io/netty/",
    "org/apache/maven/",
    "org/eclipse/aether/",
    "org/codehaus/plexus/",
    "org/jetbrains/",
    "javax/inject/",
    "net/md_5/bungee/",
)

# Never resolvable by any reimplementation, and counted apart for that reason.
INTERNAL = ("net/minecraft/", "org/bukkit/craftbukkit/")

# The JDK's own packages, which every runtime answers for.
JDK = ("java/", "jdk/", "sun/", "com/sun/", "org/w3c/", "org/xml/", "org/ietf/")


class ClassInfo:
    """One class file, read far enough to answer linkage questions."""

    __slots__ = ("name", "flags", "super", "interfaces", "members", "refs", "calls", "classes")

    def __init__(self, name, flags, super_name, interfaces, members, refs, calls, classes):
        self.name = name
        self.flags = flags
        self.super = super_name
        self.interfaces = interfaces
        # {name+descriptor: access flags}
        self.members = members
        # [(tag, owner, name, descriptor)]
        self.refs = refs
        # [(opcode, tag, owner, name, descriptor)] from actually used Code.
        self.calls = calls
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


def member_ref(pool, index):
    entry = pool.get(index)
    if entry is None or entry[0] not in (
        usage.TAG_FIELDREF, usage.TAG_METHODREF, usage.TAG_INTERFACE_METHODREF
    ):
        raise ValueError(f"invalid member reference {index}")
    owner_index, name_and_type_index = struct.unpack(">HH", entry[1])
    owner = usage._class_name(pool, owner_index)
    pair = pool.get(name_and_type_index)
    if not owner or pair is None or pair[0] != usage.TAG_NAME_AND_TYPE:
        raise ValueError(f"invalid member reference {index}")
    name_index, descriptor_index = struct.unpack(">HH", pair[1])
    name = usage._utf8(pool, name_index)
    descriptor = usage._utf8(pool, descriptor_index)
    if not name or not descriptor:
        raise ValueError(f"invalid member reference {index}")
    return entry[0], owner, name, descriptor


def code_accesses(code, pool):
    """Decode instruction boundaries; never mistake an operand for an opcode."""
    accesses = []
    offset = 0
    while offset < len(code):
        start = offset
        opcode = code[offset]
        offset += 1
        if opcode in (0xaa, 0xab):  # tableswitch / lookupswitch
            offset += (4 - offset % 4) % 4
            if offset + 8 > len(code):
                raise ValueError("truncated switch")
            if opcode == 0xaa:
                if offset + 12 > len(code):
                    raise ValueError("truncated tableswitch")
                low, high = struct.unpack_from(">ii", code, offset + 4)
                count = high - low + 1
                if count < 0 or count > (len(code) - offset - 12) // 4:
                    raise ValueError("invalid tableswitch bounds")
                offset += 12 + count * 4
            else:
                count = struct.unpack_from(">i", code, offset + 4)[0]
                if count < 0 or count > (len(code) - offset - 8) // 8:
                    raise ValueError("invalid lookupswitch bounds")
                offset += 8 + count * 8
            continue
        if opcode == 0xc4:  # wide
            if offset >= len(code):
                raise ValueError("truncated wide instruction")
            widened = code[offset]
            if widened == 0x84:
                offset += 5
            elif widened in {*range(0x15, 0x1a), *range(0x36, 0x3b), 0xa9}:
                offset += 3
            else:
                raise ValueError(f"invalid wide opcode {widened}")
        elif opcode in ONE_OPERAND:
            offset += 1
        elif opcode in TWO_OPERANDS:
            offset += 2
        elif opcode in THREE_OPERANDS:
            offset += 3
        elif opcode in FOUR_OPERANDS:
            offset += 4
        elif opcode > 0xc9:
            raise ValueError(f"reserved opcode {opcode}")
        if offset > len(code):
            raise ValueError(f"truncated instruction at {start}")
        if opcode in ACCESS_OPCODES:
            index = struct.unpack_from(">H", code, start + 1)[0]
            tag, owner, name, descriptor = member_ref(pool, index)
            if (opcode in FIELD_OPCODES) != (tag == usage.TAG_FIELDREF):
                raise ValueError(f"wrong constant-pool kind at {start}")
            accesses.append((opcode, tag, owner, name, descriptor))
    return accesses


def read_class(data, scan_code=True):
    pool, offset = usage._read_pool(data)
    flags, this_index, super_index, interface_count = struct.unpack_from(">HHHH", data, offset)
    offset += 8
    interfaces = []
    for _ in range(interface_count):
        interfaces.append(usage._class_name(pool, struct.unpack_from(">H", data, offset)[0]))
        offset += 2

    members = {}
    calls = []
    classes = set()
    for section in range(2):
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
                attribute_name = usage._utf8(pool, struct.unpack_from(">H", data, offset)[0])
                length = struct.unpack_from(">I", data, offset + 2)[0]
                if offset + 6 + length > len(data):
                    raise ValueError("truncated attribute")
                if scan_code and section == 1 and attribute_name == "Code":
                    if length < 8:
                        raise ValueError("truncated Code attribute")
                    code_length = struct.unpack_from(">I", data, offset + 10)[0]
                    if code_length > length - 8:
                        raise ValueError("truncated Code instructions")
                    calls.extend(code_accesses(data[offset + 14:offset + 14 + code_length], pool))
                offset += 6 + length

    refs = []
    for index, (tag, payload) in pool.items():
        if tag == usage.TAG_METHOD_HANDLE:
            kind, target_index = struct.unpack(">BH", payload)
            opcode = HANDLE_OPCODE.get(kind)
            if opcode is None:
                raise ValueError(f"invalid method-handle reference kind {kind}")
            ref_tag, owner, name, descriptor = member_ref(pool, target_index)
            calls.append((opcode, ref_tag, owner, name, descriptor))
            continue
        if tag == usage.TAG_CLASS:
            name = usage._utf8(pool, struct.unpack(">H", payload)[0])
            element = element_class(name) if name else None
            if element:
                classes.add(element)
            continue
        if tag not in (usage.TAG_FIELDREF, usage.TAG_METHODREF, usage.TAG_INTERFACE_METHODREF):
            continue
        _, owner, name, descriptor = member_ref(pool, index)
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
        calls,
        classes,
    )


def check_entry_limits(entry, total):
    if entry.file_size > MAX_CLASS_SIZE:
        raise ValueError(f"class exceeds {MAX_CLASS_SIZE} bytes: {entry.filename}")
    if entry.file_size and (
        entry.compress_size == 0 or entry.file_size > entry.compress_size * MAX_COMPRESSION_RATIO
    ):
        raise ValueError(f"class compression ratio exceeds {MAX_COMPRESSION_RATIO}: {entry.filename}")
    if total + entry.file_size > MAX_TOTAL_CLASS_BYTES:
        raise ValueError("total decompressed class size exceeds limit")
    return total + entry.file_size


def check_jar_entry_count(count):
    if count > MAX_JAR_ENTRIES:
        raise ValueError(f"JAR exceeds {MAX_JAR_ENTRIES} entries")


def preflight_archive(path):
    """Bound ZIP metadata before zipfile allocates its central-directory map."""
    size = path.stat().st_size
    if size > MAX_JAR_BYTES:
        raise ValueError(f"JAR exceeds {MAX_JAR_BYTES} bytes: {path}")
    with path.open("rb") as source:
        source.seek(max(0, size - (65_535 + 22)))
        tail = source.read()
    position = tail.rfind(b"PK\x05\x06")
    while position >= 0:
        if position + 22 <= len(tail):
            fields = struct.unpack_from("<4sHHHHIIH", tail, position)
            _, disk, directory_disk, disk_entries, count, directory_size, _, comment = fields
            if position + 22 + comment == len(tail):
                if disk or directory_disk or disk_entries != count or count == 0xffff:
                    raise ValueError(f"multi-disk or ZIP64 JAR is unsupported: {path}")
                check_jar_entry_count(count)
                if directory_size > MAX_CENTRAL_DIRECTORY_BYTES:
                    raise ValueError(f"JAR central directory exceeds limit: {path}")
                return
        position = tail.rfind(b"PK\x05\x06", 0, position)
    raise ValueError(f"JAR has no valid end-of-central-directory record: {path}")


def read_bounded(archive, entry, limit):
    data = bytearray()
    with archive.open(entry) as source:
        while chunk := source.read(65_536):
            if len(data) + len(chunk) > limit:
                raise ValueError(f"entry exceeds decompression limit: {entry.filename}")
            data.extend(chunk)
    return bytes(data)


def multi_release_enabled(archive, entries):
    manifest = entries.get("META-INF/MANIFEST.MF")
    if manifest is None:
        return False
    if manifest.file_size > 65_536:
        raise ValueError("JAR manifest exceeds limit")
    text = read_bounded(archive, manifest, 65_536).decode("utf-8", errors="replace")
    unfolded = text.replace("\r\n", "\n").replace("\n ", "")
    main_section = unfolded.split("\n\n", 1)[0]
    return any(line.lower().strip() == "multi-release: true" for line in main_section.splitlines())


def read_jar(path, scan_code=True, java_version=JAVA_VERSION):
    if not 9 <= java_version <= JAVA_VERSION:
        raise ValueError(f"unsupported Java target {java_version}; supported range is 9..{JAVA_VERSION}")
    preflight_archive(path)
    classes = {}
    with zipfile.ZipFile(path) as archive:
        infos = archive.infolist()
        check_jar_entry_count(len(infos))
        entries = {}
        for entry in infos:
            if entry.filename in entries:
                raise ValueError(f"duplicate archive entry {entry.filename} in {path}")
            entries[entry.filename] = entry
        multi_release = multi_release_enabled(archive, entries)
        selected = {}
        for name, entry in entries.items():
            if not name.endswith(".class"):
                continue
            version = 0
            logical = name
            if name.startswith("META-INF/versions/"):
                parts = name.split("/", 3)
                if len(parts) != 4 or not parts[2].isdigit() or int(parts[2]) < 9:
                    raise ValueError(f"malformed multi-release class entry {name} in {path}")
                if not multi_release:
                    continue  # Java ignores versioned classes without the manifest flag.
                version = int(parts[2])
                logical = parts[3]
                if version > java_version:
                    continue
            elif name.startswith("META-INF/"):
                continue
            if logical == "module-info.class":
                continue  # Classpath loading does not define the module descriptor.
            previous = selected.get(logical)
            if previous is None or previous[0] < version:
                selected[logical] = (version, entry)
        if len(selected) > MAX_CLASS_COUNT:
            raise ValueError(f"JAR exceeds {MAX_CLASS_COUNT} selected classes: {path}")
        total = 0
        for logical, (_, entry) in selected.items():
            total = check_entry_limits(entry, total)
            try:
                info = read_class(read_bounded(archive, entry, MAX_CLASS_SIZE), scan_code)
            except (usage.NotAClassFile, struct.error, KeyError, IndexError, ValueError,
                    zipfile.BadZipFile, EOFError) as error:
                raise ValueError(f"unreadable class {entry.filename} in {path}: {error}") from error
            if info.name != logical[:-6]:
                raise ValueError(f"class name {info.name!r} does not match entry {entry.filename} in {path}")
            classes[info.name] = info
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

    def resolve(self, owner, signature, field, seen=None):
        """Return the declaring type/flags, not merely a Boolean hit."""
        seen = seen if seen is not None else set()
        if owner in seen:
            return None
        seen.add(owner)
        info = self.find(owner)
        if info is None:
            return None
        access = info.members.get(signature)
        if access is not None:
            return owner, access
        parents = [*info.interfaces, info.super] if field else [info.super, *info.interfaces]
        for parent in parents:
            if parent:
                found = self.resolve(parent, signature, field, seen)
                if found is not None:
                    return found
        return None

    def is_subclass(self, name, ancestor, seen=None):
        if name == ancestor:
            return True
        seen = seen if seen is not None else set()
        if name in seen:
            return False
        seen.add(name)
        info = self.find(name)
        return info is not None and any(
            self.is_subclass(parent, ancestor, seen)
            for parent in [info.super, *info.interfaces] if parent
        )

    def _jdk_has(self, name, signature):
        """A JDK class this tool does not model; only the common members are known."""
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
            if access & (ACC_STATIC | ACC_PRIVATE):
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
            # Object does not implement every unknown interface method. Only
            # signatures known to exist in the JDK can satisfy a contract.
            return name.startswith("java/") and self._jdk_has(name, signature) is True
        access = info.members.get(signature)
        if access is not None:
            # An abstract redeclaration masks inherited bodies; a static or
            # private same-named method cannot implement the contract.
            return not access & (ACC_ABSTRACT | ACC_STATIC | ACC_PRIVATE)
        for parent in [info.super, *info.interfaces]:
            if parent and self.concrete(parent, signature, seen):
                return True
        return False


def served(name):
    return name.startswith(SERVER_PROVIDED) and not name.startswith(INTERNAL)


def is_jdk(name):
    # javax.inject is an external JAR in Paper's runtime; javax.xml, sql,
    # net.ssl and the rest are Java platform modules.
    return name.startswith(JDK) or (name.startswith("javax/") and not name.startswith("javax/inject/"))


def package_of(name):
    return name.rpartition("/")[0]


def access_problem(world, caller, owner, declarer, flags, opcode):
    """Return a failure category, or None for an access proven legal."""
    for class_name in (owner, declarer):
        info = world.find(class_name)
        if info is not None and not info.flags & ACC_PUBLIC and package_of(caller) != package_of(class_name):
            return "inaccessible class"
    if flags & ACC_PUBLIC:
        return None
    if flags & ACC_PRIVATE:
        return None if caller == declarer else "inaccessible member"
    if package_of(caller) == package_of(declarer):
        return None
    if not flags & ACC_PROTECTED or not world.is_subclass(caller, declarer):
        return "inaccessible member"
    if opcode in STATIC_OPCODES or opcode == 0xb7:
        return None
    # Java's cross-package protected instance rule also constrains the
    # receiver type. This static pass has not modeled the operand stack.
    return "protected receiver needs runtime verification"


def inherited_server_contracts(world, class_name):
    """Find API contracts through every known plugin-owned ancestor."""
    contracts = set()
    pending = [class_name]
    seen = set()
    while pending:
        current = pending.pop()
        if current in seen:
            continue
        seen.add(current)
        info = world.find(current)
        if info is None:
            continue
        for parent in [info.super, *info.interfaces]:
            if not parent:
                continue
            if served(parent):
                contracts.add(parent)
            else:
                pending.append(parent)
    return contracts
def check(plugin_path, world, plugin_classes):
    problems = collections.defaultdict(set)
    external = collections.Counter()
    internal = set()
    for info in plugin_classes.values():
        for name in info.classes:
            if name.startswith(INTERNAL):
                internal.add(name)
                continue
            target_class = world.find(name)
            if target_class is not None:
                if (served(name) and not target_class.flags & ACC_PUBLIC
                        and package_of(info.name) != package_of(name)):
                    problems["inaccessible class"].add(
                        f"{name}  (named by {info.name})"
                    )
                continue
            if is_jdk(name):
                continue
            if served(name):
                problems["missing class"].add(f"{name}  (named by {info.name})")
            else:
                external[name.rsplit("/", 1)[0]] += 1

        for tag, owner, member, descriptor in info.refs:
            if owner.startswith(INTERNAL) or not (served(owner) or owner in plugin_classes):
                continue
            target = world.find(owner)
            if target is None:
                continue  # already reported as a missing class
            signature = f"{member}{descriptor}"
            resolved = world.resolve(owner, signature, tag == usage.TAG_FIELDREF)
            if owner in plugin_classes and (resolved is None or not served(resolved[0])):
                continue
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

        for opcode, tag, owner, member, descriptor in info.calls:
            if owner.startswith(INTERNAL) or not (served(owner) or owner in plugin_classes):
                continue
            signature = f"{member}{descriptor}"
            resolved = world.resolve(owner, signature, opcode in FIELD_OPCODES)
            if resolved is None:
                continue  # missing member above, or an unmodeled JDK ancestor.
            declarer, flags = resolved
            if not served(declarer):
                continue
            if bool(opcode in STATIC_OPCODES) != bool(flags & ACC_STATIC):
                problems["wrong static kind"].add(f"{owner}#{signature}")
            access = access_problem(world, info.name, owner, declarer, flags, opcode)
            if access:
                problems[access].add(f"{owner}#{signature}  (called by {info.name})")

        if info.flags & (ACC_ABSTRACT | ACC_INTERFACE):
            continue
        for parent in inherited_server_contracts(world, info.name):
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
    parser.add_argument("--java-version", type=int, default=JAVA_VERSION)
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
    layers = [read_jar(args.api, scan_code=False, java_version=args.java_version), jdk_classes()]
    layers += [read_jar(jar, scan_code=False, java_version=args.java_version)
               for jar in sorted(args.libs.glob("*.jar"))]
    layers += [read_jar(jar, scan_code=False, java_version=args.java_version)
               for jar in args.provided_by]

    failed = False
    for plugin in args.plugins:
        plugin_classes = read_jar(plugin, java_version=args.java_version)
        if not plugin_classes:
            raise SystemExit(f"no readable plugin classes in {plugin}")
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
