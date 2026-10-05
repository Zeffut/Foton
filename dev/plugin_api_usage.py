#!/usr/bin/env python3
"""What a corpus of real plugins actually asks a server for.

`org.bukkit` is around fifteen hundred public types before Paper adds its own,
and no useful amount of it can be implemented by working through it
alphabetically. This reads the jars instead: every class file carries a constant
pool naming exactly which methods and fields it references, so what a plugin
needs from a server is a fact that can be read rather than a thing to guess at.

Ranking is by how many distinct plugins reference a member, not by how many
times it appears. A plugin calling one method a thousand times is still one
plugin, and implementing for it would be implementing for an audience of one.

The corpus itself is not committed -- those are other people's artifacts, and a
few hundred megabytes of them. The ledger this produces is, so that everything
downstream reads a number instead of re-downloading the internet.

    python3 dev/plugin-api-usage.py <corpus-dir> [--write]

Standard library only, like the rest of `dev/`.
"""

import argparse
import collections
import json
import pathlib
import re
import struct
import sys
import zipfile
import zlib

REPO = pathlib.Path(__file__).resolve().parent.parent
LEDGER = REPO / "dev" / "plugin-api-usage.json"

# How many plugins have to reference a member before it reaches the committed
# ledger. A member exactly one plugin in the corpus calls is not evidence of
# anything -- it is that plugin's own taste, and the tail of those is most of
# the file. The count of everything is kept beside the list so the truncation
# is visible rather than implied.
SHARED_BY = 2

# Package prefixes worth counting, and what each one means for Foton.
#
# `api` is the public surface. `internal` is the versioned Mojang/CraftBukkit
# surface, which needs separate implementation and certification.
SURFACES = {
    "org/bukkit/": "api",
    "io/papermc/paper/": "api",
    "com/destroystokyo/paper/": "api",
    "org/spigotmc/": "api",
    "net/minecraft/": "internal",
    "org/bukkit/craftbukkit/": "internal",
}

# A specific implementation package must win over its public-package parent.
# `org/bukkit/craftbukkit` starts with `org/bukkit`, so insertion order would
# otherwise classify the implementation internals as API.
SURFACE_PREFIXES = sorted(SURFACES.items(), key=lambda entry: len(entry[0]), reverse=True)
CLASS_IN_DESCRIPTOR = re.compile(r"L([A-Za-z_$][\w$]*(?:/[A-Za-z_$][\w$]*)+);")

# Constant pool tags, from the JVM specification, table 4.4-B.
TAG_UTF8 = 1
TAG_CLASS = 7
TAG_FIELDREF = 9
TAG_METHODREF = 10
TAG_INTERFACE_METHODREF = 11
TAG_NAME_AND_TYPE = 12
TAG_METHOD_HANDLE = 15
TAG_METHOD_TYPE = 16
TAG_DYNAMIC = 17
TAG_INVOKE_DYNAMIC = 18
TAG_MODULE = 19
TAG_PACKAGE = 20

# Tag -> how many bytes follow it, for the tags whose payload is fixed width.
FIXED_WIDTH = {
    3: 4,   # Integer
    4: 4,   # Float
    5: 8,   # Long
    6: 8,   # Double
    TAG_CLASS: 2,
    8: 2,   # String
    TAG_FIELDREF: 4,
    TAG_METHODREF: 4,
    TAG_INTERFACE_METHODREF: 4,
    TAG_NAME_AND_TYPE: 4,
    TAG_METHOD_HANDLE: 3,
    TAG_METHOD_TYPE: 2,
    TAG_DYNAMIC: 4,
    TAG_INVOKE_DYNAMIC: 4,
    TAG_MODULE: 2,
    TAG_PACKAGE: 2,
}

# Long and Double take two constant pool slots. This is the JVM's own
# long-standing wart and it has to be honored or every later index is wrong.
DOUBLE_WIDTH_TAGS = {5, 6}


class NotAClassFile(Exception):
    """The bytes are not a class file this tool can read."""


CLASS_ENTRY_ERRORS = (
    NotAClassFile,
    struct.error,
    KeyError,
    IndexError,
    zipfile.BadZipFile,
    OSError,
    EOFError,
    RuntimeError,
    NotImplementedError,
    zlib.error,
)
ARCHIVE_ERRORS = (
    zipfile.BadZipFile,
    OSError,
    EOFError,
    RuntimeError,
    NotImplementedError,
    zlib.error,
)


def _error_reason(error):
    return str(error) or type(error).__name__


def _unreadable_class(entry, error):
    return {"entry": entry, "reason": _error_reason(error)}


def _empty_scan_result():
    found = {kind: set() for kind in set(SURFACES.values())}
    found["classes"] = set()
    found["handler_events"] = set()
    found["unreadable_classes"] = []
    return found


def constant_pool(data):
    """Returns the constant pool of one class file, as {index: (tag, payload)}.

    Only the pool is read. The rest of the class file -- fields, methods, code
    -- says how the references are used, and this tool only needs to know that
    they exist.
    """
    return _read_pool(data)[0]


def _read_pool(data):
    """The pool, and the offset just past it, which is where the class begins."""
    if len(data) < 10 or data[:4] != b"\xca\xfe\xba\xbe":
        raise NotAClassFile("bad magic")
    count = struct.unpack_from(">H", data, 8)[0]
    pool = {}
    offset = 10
    index = 1
    while index < count:
        tag = data[offset]
        offset += 1
        if tag == TAG_UTF8:
            length = struct.unpack_from(">H", data, offset)[0]
            offset += 2
            pool[index] = (tag, data[offset:offset + length])
            offset += length
        else:
            width = FIXED_WIDTH.get(tag)
            if width is None:
                raise NotAClassFile(f"unknown constant pool tag {tag}")
            pool[index] = (tag, data[offset:offset + width])
            offset += width
        index += 2 if tag in DOUBLE_WIDTH_TAGS else 1
    return pool, offset


def _utf8(pool, index):
    entry = pool.get(index)
    if not entry or entry[0] != TAG_UTF8:
        return None
    return entry[1].decode("utf-8", errors="replace")


def _class_name(pool, index):
    entry = pool.get(index)
    if not entry or entry[0] != TAG_CLASS:
        return None
    return _utf8(pool, struct.unpack(">H", entry[1])[0])


def _member_name(pool, index):
    entry = pool.get(index)
    if not entry or entry[0] not in (TAG_FIELDREF, TAG_METHODREF, TAG_INTERFACE_METHODREF):
        return None
    owner_index, name_type_index = struct.unpack(">HH", entry[1])
    owner = _class_name(pool, owner_index)
    name_type = pool.get(name_type_index)
    if not owner or not name_type or name_type[0] != TAG_NAME_AND_TYPE:
        return None
    name_index, descriptor_index = struct.unpack(">HH", name_type[1])
    name = _utf8(pool, name_index)
    descriptor = _utf8(pool, descriptor_index)
    return f"{owner}#{name}{descriptor}" if name and descriptor else None


def references(data):
    """Every member reference a class file makes into a watched package.

    Yields `(surface, "owner#member")`, where owner is a slash-separated class
    name so it reads the way the constant pool stores it.
    """
    pool = constant_pool(data)
    for tag, payload in pool.values():
        if tag not in (TAG_FIELDREF, TAG_METHODREF, TAG_INTERFACE_METHODREF):
            continue
        class_index, name_and_type_index = struct.unpack(">HH", payload)
        owner = _class_name(pool, class_index)
        if not owner:
            continue
        surface = next(
            (kind for prefix, kind in SURFACE_PREFIXES if owner.startswith(prefix)),
            None,
        )
        if surface is None:
            continue
        entry = pool.get(name_and_type_index)
        if not entry or entry[0] != TAG_NAME_AND_TYPE:
            continue
        name_index, descriptor_index = struct.unpack(">HH", entry[1])
        member = _utf8(pool, name_index)
        descriptor = _utf8(pool, descriptor_index)
        if member and descriptor:
            yield surface, f"{owner}#{member}{descriptor}"


EVENT_HANDLER_DESCRIPTOR = "Lorg/bukkit/event/EventHandler;"


def _descriptor_classes(descriptor):
    """Class names carried by a JVM field, method or generic descriptor."""
    return set(re.findall(r"L([^;<]+)", descriptor))


def _skip_annotation(data, offset):
    """Returns the offset after one JVM annotation structure."""
    _, pairs = struct.unpack_from(">HH", data, offset)
    offset += 4
    for _ in range(pairs):
        offset += 2  # element_name_index
        offset = _skip_element_value(data, offset)
    return offset


def _skip_element_value(data, offset):
    """Returns the offset after one JVM annotation element_value."""
    tag = chr(data[offset])
    offset += 1
    if tag in "BCDFIJSZsc":
        return offset + 2
    if tag == "e":
        return offset + 4
    if tag == "@":
        return _skip_annotation(data, offset)
    if tag == "[":
        count = struct.unpack_from(">H", data, offset)[0]
        offset += 2
        for _ in range(count):
            offset = _skip_element_value(data, offset)
        return offset
    raise NotAClassFile(f"unknown annotation element tag {tag!r}")


def _annotation_types(pool, data):
    """Annotation descriptors in a RuntimeVisible/InvisibleAnnotations body."""
    count = struct.unpack_from(">H", data, 0)[0]
    offset = 2
    result = set()
    for _ in range(count):
        type_index = struct.unpack_from(">H", data, offset)[0]
        descriptor = _utf8(pool, type_index)
        if descriptor:
            result.add(descriptor)
        offset = _skip_annotation(data, offset)
    return result


def _class_usage(data):
    """All class references and Bukkit event-handler parameter types."""
    pool, offset = _read_pool(data)
    classes = set()

    for tag, payload in pool.values():
        descriptor = None
        if tag == TAG_CLASS:
            name = _utf8(pool, struct.unpack(">H", payload)[0])
            if name:
                if name.startswith("["):
                    classes.update(_descriptor_classes(name))
                else:
                    classes.add(name)
        elif tag == TAG_NAME_AND_TYPE:
            _, descriptor_index = struct.unpack(">HH", payload)
            descriptor = _utf8(pool, descriptor_index)
        elif tag == TAG_METHOD_TYPE:
            descriptor = _utf8(pool, struct.unpack(">H", payload)[0])
        if descriptor:
            classes.update(_descriptor_classes(descriptor))

    offset += 6  # access_flags, this_class, super_class
    interface_count = struct.unpack_from(">H", data, offset)[0]
    offset += 2 + 2 * interface_count

    handler_events = set()
    for section in range(2):  # fields, then methods
        count = struct.unpack_from(">H", data, offset)[0]
        offset += 2
        for _ in range(count):
            _, _, descriptor_index, attribute_count = struct.unpack_from(
                ">HHHH", data, offset)
            offset += 8
            descriptor = _utf8(pool, descriptor_index) or ""
            descriptor_types = _descriptor_classes(descriptor)
            classes.update(descriptor_types)
            annotations = set()
            for _ in range(attribute_count):
                attribute_name_index, length = struct.unpack_from(">HI", data, offset)
                offset += 6
                attribute_name = _utf8(pool, attribute_name_index)
                attribute = data[offset:offset + length]
                offset += length
                if attribute_name in (
                    "RuntimeVisibleAnnotations",
                    "RuntimeInvisibleAnnotations",
                ):
                    annotations.update(_annotation_types(pool, attribute))
            if section == 1 and EVENT_HANDLER_DESCRIPTOR in annotations:
                handler_events.update(
                    name for name in descriptor_types
                    if "/event/" in name and name.endswith("Event")
                )

    return classes, handler_events


def class_usage(data):
    """Watched class references and Bukkit event-handler parameter types."""
    classes, handler_events = _class_usage(data)
    watched = {
        name for name in classes
        if any(name.startswith(prefix) for prefix, _ in SURFACE_PREFIXES)
    }
    return watched, handler_events


def _watched_class(name):
    return any(name.startswith(prefix) for prefix, _ in SURFACE_PREFIXES)


def class_details(data):
    """Class, descriptor, method-handle and reflection demands in one class file.

    A constant string is only a candidate reflection target. Constructed names
    cannot be inferred reliably from bytecode strings, so their call sites need
    manual review rather than being presented as a complete static inventory.
    """
    pool = constant_pool(data)
    classes = set()
    handles = set()
    indy = set()
    reflection_names = set()
    partial_names = set()
    strings = set()
    for tag, payload in pool.values():
        if tag == TAG_CLASS:
            name = _utf8(pool, struct.unpack(">H", payload)[0])
            if name:
                classes.update(candidate for candidate in CLASS_IN_DESCRIPTOR.findall(name)
                               if _watched_class(candidate))
                if _watched_class(name):
                    classes.add(name)
        elif tag == TAG_UTF8:
            value = payload.decode("utf-8", errors="replace")
            classes.update(candidate for candidate in CLASS_IN_DESCRIPTOR.findall(value)
                           if _watched_class(candidate))
        elif tag == 8:  # CONSTANT_String, unlike arbitrary UTF8, is loadable at runtime.
            value = _utf8(pool, struct.unpack(">H", payload)[0])
            if value:
                strings.add(value)
        elif tag == TAG_METHOD_HANDLE:
            kind, target = struct.unpack(">BH", payload)
            member = _member_name(pool, target)
            if member:
                handles.add(f"{kind}:{member}")
        elif tag in (TAG_DYNAMIC, TAG_INVOKE_DYNAMIC):
            _, name_and_type = struct.unpack(">HH", payload)
            entry = pool.get(name_and_type)
            if entry and entry[0] == TAG_NAME_AND_TYPE:
                name, descriptor = struct.unpack(">HH", entry[1])
                signature = f"{_utf8(pool, name)}{_utf8(pool, descriptor)}"
                (indy if tag == TAG_INVOKE_DYNAMIC else handles).add(signature)
    for value in strings:
        normalized = value.replace(".", "/")
        if _watched_class(normalized) and re.fullmatch(r"[\w$/]+", normalized):
            reflection_names.add(normalized)
        elif any(normalized.startswith(prefix) for prefix, _ in SURFACE_PREFIXES):
            partial_names.add(value)
    reflective_calls = False
    for tag, payload in pool.values():
        if tag not in (TAG_METHODREF, TAG_INTERFACE_METHODREF):
            continue
        owner_index, name_type_index = struct.unpack(">HH", payload)
        owner = _class_name(pool, owner_index)
        name_type = pool.get(name_type_index)
        if owner not in ("java/lang/Class", "java/lang/ClassLoader") or not name_type:
            continue
        name_index = struct.unpack_from(">H", name_type[1])[0]
        if _utf8(pool, name_index) in ("forName", "loadClass"):
            reflective_calls = True
    return classes, handles, indy, reflection_names, reflective_calls, partial_names


def scan_details(jar):
    """Inventory static members and non-member linkage with review flags."""
    found = {kind: set() for kind in set(SURFACES.values())}
    details = dict(classes=set(), services=set(), method_handles=set(),
                   invokedynamic=set(), reflection_names=set(), manual_review=set(),
                   unreadable=0)
    with zipfile.ZipFile(jar) as archive:
        for entry in archive.namelist():
            if entry.startswith("META-INF/services/") and not entry.endswith("/"):
                service = entry.removeprefix("META-INF/services/").replace(".", "/")
                if _watched_class(service):
                    details["services"].add(service)
                continue
            if not entry.endswith(".class"):
                continue
            try:
                data = archive.read(entry)
                for surface, member in references(data):
                    found[surface].add(member)
                classes, handles, indy, names, reflective, _partial = class_details(data)
                details["classes"].update(classes)
                details["method_handles"].update(handles)
                details["invokedynamic"].update(indy)
                details["reflection_names"].update(names)
                # A constant-pool string does not identify which reflective
                # call site uses it. A class may also build a second name.
                if reflective:
                    details["manual_review"].add(entry)
            except (NotAClassFile, struct.error, KeyError, IndexError):
                details["unreadable"] += 1
    return found | details


# What a class answers because a JDK supertype does. Those class files are not
# in the API jar and never will be, but `player.toString()` and
# `material.name()` both compile to references on the API type -- so without
# this, every such call would be counted as a gap that does not exist and
# phantom members would sit near the top of the ranking.
FROM_OBJECT = frozenset({
    "toString()Ljava/lang/String;",
    "equals(Ljava/lang/Object;)Z",
    "hashCode()I",
    "getClass()Ljava/lang/Class;",
    "clone()Ljava/lang/Object;",
    "finalize()V",
    "notify()V",
    "notifyAll()V",
    "wait()V",
    "wait(J)V",
    "wait(JI)V",
})

# Bukkit's serialization wrappers inherit these public methods from the JDK.
# The wrapper classes are intentionally thin, so calls compiled against the
# Bukkit owner still resolve through the Java superclass at runtime.
INHERITED_API = {
    "org/bukkit/util/io/BukkitObjectInputStream": {
        "readObject()Ljava/lang/Object;",
    },
    "org/bukkit/util/io/BukkitObjectOutputStream": {
        "writeObject(Ljava/lang/Object;)V",
    },
}

FROM_JDK = {
    "java/lang/Object": FROM_OBJECT,
    "java/lang/Enum": FROM_OBJECT | {
        "name()Ljava/lang/String;",
        "ordinal()I",
        "compareTo(Ljava/lang/Enum;)I",
        "getDeclaringClass()Ljava/lang/Class;",
        "describeConstable()Ljava/util/Optional;",
    },
    "java/lang/Record": FROM_OBJECT,
    "java/lang/Throwable": FROM_OBJECT | {
        "getMessage()Ljava/lang/String;",
        "getLocalizedMessage()Ljava/lang/String;",
        "getCause()Ljava/lang/Throwable;",
        "printStackTrace()V",
        "printStackTrace(Ljava/io/PrintStream;)V",
        "printStackTrace(Ljava/io/PrintWriter;)V",
        "getStackTrace()[Ljava/lang/StackTraceElement;",
        "initCause(Ljava/lang/Throwable;)Ljava/lang/Throwable;",
        "addSuppressed(Ljava/lang/Throwable;)V",
        "getSuppressed()[Ljava/lang/Throwable;",
        "fillInStackTrace()Ljava/lang/Throwable;",
    },
}


def declares(data):
    """What one class file *provides*: its name, its supertypes, its members.

    The mirror of `references`. Together they answer the only question that
    matters for compatibility -- whether the thing a plugin calls is there --
    which counting classes written never could.
    """
    pool, offset = _read_pool(data)
    offset += 2  # access_flags
    this_index, super_index, interface_count = struct.unpack_from(">HHH", data, offset)
    offset += 6
    supertypes = []
    if super_index:
        supertypes.append(_class_name(pool, super_index))
    for _ in range(interface_count):
        supertypes.append(_class_name(pool, struct.unpack_from(">H", data, offset)[0]))
        offset += 2

    members = set()
    for _ in range(2):  # fields, then methods: the same shape twice
        count = struct.unpack_from(">H", data, offset)[0]
        offset += 2
        for _ in range(count):
            name = _utf8(pool, struct.unpack_from(">H", data, offset + 2)[0])
            descriptor = _utf8(pool, struct.unpack_from(">H", data, offset + 4)[0])
            if name and descriptor:
                members.add(f"{name}{descriptor}")
            offset += 6
            attributes = struct.unpack_from(">H", data, offset)[0]
            offset += 2
            for _ in range(attributes):
                length = struct.unpack_from(">I", data, offset + 2)[0]
                offset += 6 + length
    return _class_name(pool, this_index), [s for s in supertypes if s], members


def provided(api_jar):
    """Every member the built API jar can answer, per class.

    Returns {class: {member, ...}} with inherited members folded in, because a
    plugin calls `JavaPlugin#getServer` and `Plugin#getServer` interchangeably
    and both have to resolve.
    """
    own = {}
    parents = {}
    with zipfile.ZipFile(api_jar) as archive:
        for entry in archive.namelist():
            if not entry.endswith(".class"):
                continue
            try:
                name, supertypes, members = declares(archive.read(entry))
            except (NotAClassFile, struct.error, KeyError, IndexError):
                continue
            if name:
                own[name] = members
                parents[name] = supertypes

    resolved = {}

    def walk(name, seen):
        if name in resolved:
            return resolved[name]
        if name in seen:
            return set()
        if name not in own:
            # A supertype the jar does not hold. If the JDK provides it, what
            # it provides is still reachable; anything else is genuinely absent.
            return FROM_JDK.get(name, set())
        seen.add(name)
        members = set(own[name])
        for parent in parents.get(name, ()):
            members |= walk(parent, seen)
        resolved[name] = members
        return members

    for name in own:
        walk(name, set())
    return resolved


def gaps(corpus, api_jar, include_internal=False):
    """What each plugin still calls that the jar cannot answer.

    Only the `api` surface. Corpus ranking can exclude plugins that also need
    internals; a single-JAR gap report includes their public demands.
    """
    have = provided(api_jar)
    per_plugin = {}
    missing_audience = collections.Counter()
    jars = [corpus] if corpus.is_file() else sorted(corpus.glob("*.jar"))
    for jar in jars:
        found, _ = scan(jar)
        internal_classes = classes_on_surface(found["classes"], "internal")
        if (found["internal"] or internal_classes) and not include_internal:
            continue
        wanted_members = found["api"]
        wanted_classes = classes_on_surface(found["classes"], "api")
        if not wanted_members and not wanted_classes:
            continue
        missing = set()
        for member in wanted_members:
            owner, signature = member.split("#", 1)
            if signature not in FROM_OBJECT and signature not in INHERITED_API.get(owner, ()) and signature not in have.get(owner, ()):
                missing.add(member)
        missing.update(wanted_classes - have.keys())
        per_plugin[jar.name] = (len(wanted_members) + len(wanted_classes), missing)
        for member in missing:
            missing_audience[member] += 1
    return per_plugin, missing_audience


def scan(jar):
    """What one plugin jar references, including classes and handler events.

    A jar that cannot be read at all is reported rather than skipped silently:
    a corpus that quietly lost half its entries would produce a ranking that
    looks exactly as authoritative as a correct one.
    """
    found = _empty_scan_result()
    with zipfile.ZipFile(jar) as archive:
        for entry in archive.namelist():
            if not entry.endswith(".class"):
                continue
            try:
                data = archive.read(entry)
                for surface, member in references(data):
                    found[surface].add(member)
                classes, handler_events = class_usage(data)
                found["classes"].update(classes)
                found["handler_events"].update(handler_events)
            except CLASS_ENTRY_ERRORS as error:
                found["unreadable_classes"].append(
                    _unreadable_class(entry, error))
    found["unreadable_classes"].sort(key=lambda row: row["entry"])
    return found, len(found["unreadable_classes"])


DESCRIPTOR_ENTRYPOINT = re.compile(
    r"^\s*(?:main|bootstrapper)\s*:\s*['\"]?([^\s'#\"]+)", re.MULTILINE)


def plugin_incidence(jar):
    """Per-plugin internal incidence split by descriptor-entrypoint reachability."""
    class_dependencies = {}
    internal_members = {}
    internal_classes = {}
    entrypoints = set()
    unreadable_classes = []
    with zipfile.ZipFile(jar) as archive:
        for descriptor in ("plugin.yml", "paper-plugin.yml"):
            if descriptor not in archive.namelist():
                continue
            source = archive.read(descriptor).decode("utf-8", errors="replace")
            entrypoints.update(
                name.replace(".", "/")
                for name in DESCRIPTOR_ENTRYPOINT.findall(source)
            )

        for entry in archive.namelist():
            if not entry.endswith(".class"):
                continue
            try:
                data = archive.read(entry)
                owner, _, _ = declares(data)
                classes, _ = _class_usage(data)
                members = {
                    member for surface, member in references(data)
                    if surface == "internal"
                }
            except CLASS_ENTRY_ERRORS as error:
                unreadable_classes.append(_unreadable_class(entry, error))
                continue
            if not owner:
                continue
            class_dependencies[owner] = classes
            internal_members[owner] = members
            internal_classes[owner] = classes_on_surface(classes, "internal")

    own_classes = set(class_dependencies)
    complete = (
        not unreadable_classes
        and bool(entrypoints)
        and entrypoints <= own_classes
    )
    reachable = set()
    if complete:
        pending = list(entrypoints)
        while pending:
            owner = pending.pop()
            if owner in reachable:
                continue
            reachable.add(owner)
            pending.extend(
                (class_dependencies.get(owner, set()) & own_classes) - reachable)

    load_bearing_owners = reachable if complete else set()
    optional_owners = own_classes - load_bearing_owners if complete else set()
    if unreadable_classes:
        reachability_reason = "one or more plugin classes were unreadable"
    elif not entrypoints:
        reachability_reason = "no descriptor entrypoint was found"
    elif not entrypoints <= own_classes:
        reachability_reason = "one or more descriptor entrypoints were unreadable or absent"
    else:
        reachability_reason = None
    return {
        "plugin": pathlib.Path(jar).name,
        "reachability_status": "complete" if complete else "unknown",
        "reachability_reason": reachability_reason,
        "archive_error": None,
        "entrypoints": sorted(entrypoints),
        "entrypoint_reachable_classes": sorted(reachable) if complete else None,
        "unreadable_class_count": len(unreadable_classes),
        "unreadable_classes": sorted(
            unreadable_classes, key=lambda row: row["entry"]),
        "internal": {
            "load_bearing_classes": sorted(set().union(
                *(internal_classes.get(owner, set()) for owner in load_bearing_owners)
            )),
            "load_bearing_members": sorted(set().union(
                *(internal_members.get(owner, set()) for owner in load_bearing_owners)
            )),
            "optional_adapter_classes": sorted(set().union(
                *(internal_classes.get(owner, set()) for owner in optional_owners)
            )),
            "optional_adapter_members": sorted(set().union(
                *(internal_members.get(owner, set()) for owner in optional_owners)
            )),
        },
    }


def unreadable_archive_incidence(jar, error):
    """An unknown row for a corpus item whose ZIP directory cannot be read."""
    reason = _error_reason(error)
    return {
        "plugin": pathlib.Path(jar).name,
        "reachability_status": "unknown",
        "reachability_reason": "plugin archive was unreadable",
        "archive_error": reason,
        "entrypoints": [],
        "entrypoint_reachable_classes": None,
        "unreadable_class_count": None,
        "unreadable_classes": [],
        "internal": {
            "load_bearing_classes": [],
            "load_bearing_members": [],
            "optional_adapter_classes": [],
            "optional_adapter_members": [],
        },
    }


def surface_of_class(name):
    """The compatibility surface containing a slash-separated class name."""
    return next(
        (kind for prefix, kind in SURFACE_PREFIXES if name.startswith(prefix)),
        None,
    )


def classes_on_surface(classes, surface):
    """Class references belonging to one compatibility surface."""
    return {name for name in classes if surface_of_class(name) == surface}


def package_of(member):
    """The package a member's owner lives in, which is the unit worth ranking.

    Whether a plugin calls `getX` or `getY` on the same class says little; that
    it needs `org.bukkit.inventory` at all says what has to be built.
    """
    owner = member.split("#", 1)[0]
    return owner.rsplit("/", 1)[0]


def reach_by(api, plugins_per_member, group):
    """Ranks groups by how many *distinct plugins* reach into them.

    Summing member counts would let one plugin that calls forty methods in a
    package outweigh ten plugins that each call one, which is the opposite of
    what the ranking is for. So a group is scored by the largest number of
    plugins any single one of its members has -- a floor on the audience the
    group actually has.
    """
    best = {}
    for member, count in api:
        key = group(member)
        best[key] = max(best.get(key, 0), count)
    return sorted(best.items(), key=lambda pair: (-pair[1], pair[0]))


def coverage_curve(reachable, ranked):
    """What implementing the top `k` members buys, at several values of `k`.

    Two bounds, because either alone would mislead.

    The low one counts plugins whose *every* referenced member exists. It is
    pessimistic: the JVM resolves lazily, so a missing method breaks the line
    that calls it rather than the plugin that ships it, and much of what a
    plugin references sits on paths a given server never runs.

    The high one is the share of the median plugin's references that exist. It
    is optimistic for the mirror reason: covering most of a plugin is not the
    same as it working.

    The truth is between them and nobody can place it without running the
    plugins. The gap is the useful part -- it says whether effort buys breadth
    or depth.
    """
    if not reachable:
        return []
    rows = []
    for k in (100, 250, 500, 1000, 1500, 2000, 2500, 3000, 3500, 4000, len(ranked)):
        if k > len(ranked):
            continue
        have = set(ranked[:k])
        whole = sum(1 for members in reachable.values() if members <= have)
        shares = sorted(len(members & have) / len(members) for members in reachable.values())
        rows.append(
            {
                "members": k,
                "plugins_fully_covered": whole,
                "median_plugin_covered_percent": round(100 * shares[len(shares) // 2]),
            }
        )
    return rows


RUST_STRING_CONSTANT = re.compile(
    r'\bconst\s+([A-Z][A-Z0-9_]*)\s*:\s*&str\s*=\s*"([^"]+)"'
)
RUST_STATIC_CALL = re.compile(
    r'\bcall_static_method\s*\(\s*([A-Z][A-Z0-9_]*|"[^"]+")\s*,\s*'
    r'"([A-Za-z_$][A-Za-z0-9_$]*)"\s*,\s*"([^"]+)"',
    re.DOTALL,
)
RUST_DYNAMIC_JNI_CALL = re.compile(
    r'\bcall_static_method\s*\(\s*([A-Z][A-Z0-9_]*|"[^"]+")\s*,\s*'
    r'method\s*,\s*"([^"]+)"',
    re.DOTALL,
)
# Each entry names a reviewed helper whose method argument is dynamic. A new
# helper is invisible to compatibility evidence until it is explicitly added
# here and its concrete call sites still pass compiled-signature validation.
RUST_DYNAMIC_JNI_HELPERS = frozenset({"world_call", "string_call", "block_call"})
RUST_REVIEWED_DYNAMIC_CALLS = {
    "spawn_location_call_named": {
        "firePlayerRespawn": (
            "foton/EventBridge",
            "(Ljava/lang/String;Ljava/lang/String;Z)Ljava/lang/String;",
        ),
        "firePlayerSpawnLocation": (
            "foton/EventBridge",
            "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
        ),
    },
}
RUST_FUNCTION = re.compile(
    r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)\s*\([^)]*\)\s*(?:->\s*[^\{]+)?\{",
    re.DOTALL,
)


def _method_body(source, opening_brace):
    """Returns a Java method body using balanced braces."""
    depth = 1
    offset = opening_brace + 1
    while offset < len(source) and depth:
        if source[offset] == "{":
            depth += 1
        elif source[offset] == "}":
            depth -= 1
        offset += 1
    if depth:
        return source[opening_brace + 1:]
    return source[opening_brace + 1:offset - 1]


def _compiled_method_signatures(api_jar):
    """Exact owner, method and descriptor triples declared in a compiled jar."""
    signatures = set()
    with zipfile.ZipFile(api_jar) as archive:
        for entry in archive.namelist():
            if not entry.endswith(".class"):
                continue
            try:
                owner, _, members = declares(archive.read(entry))
            except (NotAClassFile, struct.error, KeyError, IndexError):
                continue
            if not owner:
                continue
            for member in members:
                opening = member.find("(")
                if opening > 0:
                    signatures.add((owner, member[:opening], member[opening:]))
    return signatures


def _member_reference(pool, index):
    """Resolve one method-reference constant to an exact JVM method triple."""
    entry = pool.get(index)
    if not entry or entry[0] not in (TAG_METHODREF, TAG_INTERFACE_METHODREF):
        return None
    class_index, name_and_type_index = struct.unpack(">HH", entry[1])
    owner = _class_name(pool, class_index)
    name_and_type = pool.get(name_and_type_index)
    if not owner or not name_and_type or name_and_type[0] != TAG_NAME_AND_TYPE:
        return None
    name_index, descriptor_index = struct.unpack(">HH", name_and_type[1])
    name = _utf8(pool, name_index)
    descriptor = _utf8(pool, descriptor_index)
    if not name or not descriptor:
        return None
    return owner, name, descriptor


def _event_type(name):
    return "/event/" in name and name.endswith("Event")


def _code_edges(pool, code, bootstrap_calls):
    """Exact invoked methods and constructed event types in one Code body."""
    calls = set()
    events = set()
    offset = 0
    one_byte = {0x10, 0x12, *range(0x15, 0x1a), *range(0x36, 0x3b), 0xa9, 0xbc}
    two_bytes = {
        0x11, 0x13, 0x14, 0x84,
        *range(0x99, 0xa9),
        *range(0xb2, 0xb9),
        0xbb, 0xbd, 0xc0, 0xc1, 0xc6, 0xc7,
    }
    while offset < len(code):
        opcode = code[offset]
        offset += 1
        if opcode in (0xb6, 0xb7, 0xb8, 0xb9):
            index = struct.unpack_from(">H", code, offset)[0]
            reference = _member_reference(pool, index)
            if reference:
                calls.add(reference)
        elif opcode == 0xbb:
            index = struct.unpack_from(">H", code, offset)[0]
            name = _class_name(pool, index)
            if name and _event_type(name):
                events.add(name)
        elif opcode == 0xba:
            index = struct.unpack_from(">H", code, offset)[0]
            entry = pool.get(index)
            if entry and entry[0] == TAG_INVOKE_DYNAMIC:
                bootstrap_index, name_and_type_index = struct.unpack(">HH", entry[1])
                calls.update(bootstrap_calls.get(bootstrap_index, ()))
                name_and_type = pool.get(name_and_type_index)
                if name_and_type and name_and_type[0] == TAG_NAME_AND_TYPE:
                    _, descriptor_index = struct.unpack(">HH", name_and_type[1])
                    descriptor = _utf8(pool, descriptor_index) or ""
                    events.update(
                        name for name in _descriptor_classes(descriptor)
                        if _event_type(name)
                    )

        if opcode == 0xaa:  # tableswitch, padded to a four-byte boundary
            offset += (-offset) % 4
            _, low, high = struct.unpack_from(">iii", code, offset)
            offset += 12 + 4 * (high - low + 1)
        elif opcode == 0xab:  # lookupswitch
            offset += (-offset) % 4
            _, pairs = struct.unpack_from(">ii", code, offset)
            offset += 8 + 8 * pairs
        elif opcode == 0xc4:  # wide
            modified = code[offset]
            offset += 5 if modified == 0x84 else 3
        elif opcode in (0xb9, 0xba):
            offset += 4
        elif opcode in (0xc5,):
            offset += 3
        elif opcode in (0xc8, 0xc9):
            offset += 4
        elif opcode in one_byte:
            offset += 1
        elif opcode in two_bytes:
            offset += 2
    return events, calls


def _compiled_method_graph(api_jar):
    """Compiled Java methods indexed by exact owner, name and descriptor."""
    methods = {}
    with zipfile.ZipFile(api_jar) as archive:
        for entry in archive.namelist():
            if not entry.endswith(".class"):
                continue
            try:
                data = archive.read(entry)
                pool, offset = _read_pool(data)
                offset += 2  # access_flags
                this_index, _, interface_count = struct.unpack_from(">HHH", data, offset)
                offset += 6 + 2 * interface_count
                owner = _class_name(pool, this_index)
                field_count = struct.unpack_from(">H", data, offset)[0]
                offset += 2
                for _ in range(field_count):
                    attribute_count = struct.unpack_from(">H", data, offset + 6)[0]
                    offset += 8
                    for _ in range(attribute_count):
                        length = struct.unpack_from(">I", data, offset + 2)[0]
                        offset += 6 + length

                method_count = struct.unpack_from(">H", data, offset)[0]
                offset += 2
                method_code = []
                for _ in range(method_count):
                    _, name_index, descriptor_index, attribute_count = struct.unpack_from(
                        ">HHHH", data, offset)
                    offset += 8
                    name = _utf8(pool, name_index)
                    descriptor = _utf8(pool, descriptor_index)
                    code = b""
                    for _ in range(attribute_count):
                        attribute_name_index, length = struct.unpack_from(">HI", data, offset)
                        offset += 6
                        attribute_name = _utf8(pool, attribute_name_index)
                        if attribute_name == "Code":
                            code_length = struct.unpack_from(">I", data, offset + 4)[0]
                            code = data[offset + 8:offset + 8 + code_length]
                        offset += length
                    if owner and name and descriptor:
                        method_code.append(((owner, name, descriptor), code))

                bootstrap_calls = collections.defaultdict(set)
                class_attribute_count = struct.unpack_from(">H", data, offset)[0]
                offset += 2
                for _ in range(class_attribute_count):
                    attribute_name_index, length = struct.unpack_from(">HI", data, offset)
                    offset += 6
                    attribute_name = _utf8(pool, attribute_name_index)
                    if attribute_name == "BootstrapMethods":
                        bootstrap_count = struct.unpack_from(">H", data, offset)[0]
                        bootstrap_offset = offset + 2
                        for bootstrap_index in range(bootstrap_count):
                            _, argument_count = struct.unpack_from(
                                ">HH", data, bootstrap_offset)
                            bootstrap_offset += 4
                            for _ in range(argument_count):
                                argument_index = struct.unpack_from(
                                    ">H", data, bootstrap_offset)[0]
                                bootstrap_offset += 2
                                argument = pool.get(argument_index)
                                if argument and argument[0] == TAG_METHOD_HANDLE:
                                    _, reference_index = struct.unpack(">BH", argument[1])
                                    reference = _member_reference(pool, reference_index)
                                    if reference:
                                        bootstrap_calls[bootstrap_index].add(reference)
                    offset += length

                for node, code in method_code:
                    methods[node] = _code_edges(pool, code, bootstrap_calls)
            except (NotAClassFile, struct.error, KeyError, IndexError):
                continue
    return methods


def _rust_jni_roots(rust_source_root):
    """Exact static JNI targets, including reviewed dynamic helper call sites."""
    roots = set()
    for path in pathlib.Path(rust_source_root).rglob("*.rs"):
        source = path.read_text(encoding="utf-8", errors="replace")
        constants = dict(RUST_STRING_CONSTANT.findall(source))
        for owner_expression, method, descriptor in RUST_STATIC_CALL.findall(source):
            if owner_expression.startswith('"'):
                owner = owner_expression[1:-1]
            else:
                owner = constants.get(owner_expression)
            if owner:
                roots.add((owner, method, descriptor))

        helpers = {}
        for match in RUST_FUNCTION.finditer(source):
            helper = match.group(1)
            if helper not in RUST_DYNAMIC_JNI_HELPERS:
                continue
            body = _method_body(source, match.end() - 1)
            calls = RUST_DYNAMIC_JNI_CALL.findall(body)
            if len(calls) != 1:
                continue
            owner_expression, descriptor = calls[0]
            owner = (
                owner_expression[1:-1]
                if owner_expression.startswith('"')
                else constants.get(owner_expression)
            )
            if owner:
                helpers[helper] = (owner, descriptor)

        for helper, (owner, descriptor) in helpers.items():
            call = re.compile(
                rf"\b{re.escape(helper)}\s*\(\s*[^,\n]+,\s*"
                r'"([A-Za-z_$][A-Za-z0-9_$]*)"'
            )
            for method in call.findall(source):
                roots.add((owner, method, descriptor))
        for helper, reviewed in RUST_REVIEWED_DYNAMIC_CALLS.items():
            call = re.compile(
                rf"\b{re.escape(helper)}\s*\(\s*[^,\n]+,\s*"
                r'"([A-Za-z_$][A-Za-z0-9_$]*)"'
            )
            for method in call.findall(source):
                target = reviewed.get(method)
                if target:
                    owner, descriptor = target
                    roots.add((owner, method, descriptor))
    return roots


def emitted_events(
    java_source_root: pathlib.Path,
    rust_source_root: pathlib.Path,
    compiled_api_jar: pathlib.Path,
) -> set[str]:
    """Events reached by exact, compiled-signature-valid Rust JNI calls."""
    del java_source_root  # The compiled jar is the authoritative exact graph.
    methods = _compiled_method_graph(compiled_api_jar)
    called_from_rust = _rust_jni_roots(rust_source_root) & methods.keys()

    emitted = set()
    pending = list(called_from_rust)
    reached = set()
    while pending:
        node = pending.pop()
        if node in reached:
            continue
        reached.add(node)
        evidence = methods.get(node)
        if evidence:
            events, calls = evidence
            emitted.update(events)
            pending.extend(calls - reached)
    return emitted


def event_parents(api_jar):
    """Direct event supertypes declared by the built API jar."""
    parents = {}
    with zipfile.ZipFile(api_jar) as archive:
        for entry in archive.namelist():
            if not entry.endswith(".class"):
                continue
            try:
                name, supertypes, _ = declares(archive.read(entry))
            except (NotAClassFile, struct.error, KeyError, IndexError):
                continue
            if name:
                parents[name] = set(supertypes)
    return parents


def expand_event_hierarchy(events, parents):
    """Events plus every supertype that receives their dispatched instances."""
    reachable = set()
    pending = list(events)
    while pending:
        current = pending.pop()
        if current in reachable:
            continue
        reachable.add(current)
        pending.extend(set(parents.get(current, ())) - reachable)
    return reachable


def _member_resolution(rows, provided_members):
    missing = []
    for row in rows:
        member = row["member"]
        owner, signature = member.split("#", 1)
        if (
            signature not in FROM_OBJECT
            and signature not in INHERITED_API.get(owner, ())
            and signature not in provided_members.get(owner, ())
        ):
            missing.append(member)
    return {
        "referenced": len(rows),
        "resolved": len(rows) - len(missing),
        "missing": len(missing),
        "missing_members": sorted(missing),
    }


def _class_resolution(rows, provided_members):
    missing = [row["class"] for row in rows if row["class"] not in provided_members]
    return {
        "referenced": len(rows),
        "resolved": len(rows) - len(missing),
        "missing": len(missing),
        "missing_classes": sorted(name.replace("/", ".") for name in missing),
    }


def compatibility_summary(
    ledger,
    provided_members,
    wanted_events,
    emitted,
    parents=None,
):
    """Return JSON-serializable binary, ceiling and event evidence."""
    rows = list(ledger.get("api_members", ()))
    class_rows = list(ledger.get("api_classes", ()))
    threshold = ledger.get("api_members_kept_at_least", SHARED_BY)
    shared = [row for row in rows if row.get("plugins", 0) >= threshold]
    shared_classes = [
        row for row in class_rows if row.get("plugins", 0) >= threshold
    ]
    all_members_recorded = len(rows) == ledger.get("api_members_referenced", len(rows))
    classes_recorded = "api_classes" in ledger
    all_classes_recorded = classes_recorded and len(class_rows) == ledger.get(
        "api_classes_referenced", len(class_rows))

    all_evidence = _member_resolution(rows, provided_members)
    if not all_members_recorded:
        all_evidence = {
            "referenced": ledger.get("api_members_referenced", len(rows)),
            "resolved": None,
            "missing": None,
            "missing_members": None,
        }

    all_class_evidence = _class_resolution(class_rows, provided_members)
    if not all_classes_recorded:
        all_class_evidence = {
            "referenced": ledger.get("api_classes_referenced"),
            "resolved": None,
            "missing": None,
            "missing_classes": None,
        }
    shared_class_evidence = _class_resolution(shared_classes, provided_members)
    if not classes_recorded:
        shared_class_evidence = {
            "referenced": None,
            "resolved": None,
            "missing": None,
            "missing_classes": None,
        }

    wanted = {
        row["event"] if isinstance(row, dict) else row
        for row in wanted_events
    }
    emitted_wanted = wanted & expand_event_hierarchy(set(emitted), parents or {})
    missing_events = wanted - emitted_wanted
    scanned = ledger.get("plugins_scanned", 0)
    historical_reaches = ledger.get("plugins_reaching_internals", 0)
    incidence = ledger.get("plugin_incidence")
    def complete_incidence(row):
        internal = row.get("internal")
        return (
            row.get("reachability_status") == "complete"
            and "archive_error" in row
            and row["archive_error"] is None
            and row.get("unreadable_class_count") == 0
            and row.get("unreadable_classes") == []
            and isinstance(row.get("entrypoints"), list)
            and bool(row["entrypoints"])
            and isinstance(row.get("entrypoint_reachable_classes"), list)
            and isinstance(internal, dict)
            and all(
                isinstance(internal.get(key), list)
                for key in (
                    "load_bearing_classes",
                    "load_bearing_members",
                    "optional_adapter_classes",
                    "optional_adapter_members",
                )
            )
        )

    current_ceiling = (
        ledger.get("schema_version") == 2
        and isinstance(incidence, list)
        and len(incidence) == scanned
        and all(complete_incidence(row) for row in incidence)
    )
    if current_ceiling:
        reaches_internals = sum(
            1 for row in incidence
            if row["internal"]["load_bearing_members"]
            or row["internal"]["load_bearing_classes"]
        )
        optional_adapters = sum(
            1 for row in incidence
            if row["internal"]["optional_adapter_members"]
            or row["internal"]["optional_adapter_classes"]
        )
        ceiling = {
            "status": "current",
            "plugins_on_public_api": scanned - reaches_internals,
            "plugins_reaching_internals": reaches_internals,
            "plugins_scanned": scanned,
            "plugins_with_optional_adapters": optional_adapters,
        }
    else:
        ceiling = {
            "status": "unknown",
            "reason": (
                "ledger lacks complete per-plugin class incidence and "
                "entrypoint reachability evidence"
            ),
            "plugins_on_public_api": None,
            "plugins_reaching_internals": None,
            "plugins_scanned": scanned,
            "plugins_with_optional_adapters": None,
            "historical": {
                "plugins_on_public_api": max(scanned - historical_reaches, 0),
                "plugins_reaching_internals": historical_reaches,
                "plugins_scanned": scanned,
            },
        }
    return {
        "api": {
            "all": all_evidence,
            "classes": {
                "all": all_class_evidence,
                "shared": shared_class_evidence,
            },
            "shared": _member_resolution(shared, provided_members),
        },
        "ceiling": ceiling,
        "events": {
            "emitted": len(emitted_wanted),
            "emitted_types": sorted(name.replace("/", ".") for name in emitted_wanted),
            "listened": len(wanted),
            "missing": len(missing_events),
            "missing_types": sorted(name.replace("/", ".") for name in missing_events),
        },
    }


def write_summary_json(path, summary):
    """Writes a deterministic machine-readable compatibility summary."""
    pathlib.Path(path).write_text(
        json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")

def report_gap(corpus, api_jar, top):
    """Prints how far the built jar gets, and what is next by audience."""
    per_plugin, missing_audience = gaps(corpus, api_jar, include_internal=corpus.is_file())
    if not per_plugin:
        raise SystemExit(f"no jars in {corpus}")

    served = [name for name, (_, missing) in per_plugin.items() if not missing]
    total = len(per_plugin)
    print(f"corpus: {total} plugins referencing the servable surface")
    print(f"fully served: {len(served)}")
    for name in sorted(served):
        print(f"    {name}")

    close = sorted(
        ((len(missing), name, wanted) for name, (wanted, missing) in per_plugin.items() if missing),
        key=lambda row: row[0],
    )
    print()
    print("nearest, by how many members are still missing:")
    for count, name, wanted in close[:top]:
        print(f"    {count:4d} missing of {wanted:4d}  {name}")

    print()
    print(f"next by audience -- plugins that would gain, top {top}:")
    for member, plugins in missing_audience.most_common(top):
        print(f"    {plugins:3d}  {member}")


def report_covered(api_jar, top):
    """How much of the committed ledger the built API jar can already answer.

    The corpus is not committed, so `--gap` needs a few hundred megabytes of
    other people's jars to say anything. This asks the same question of the
    ledger instead, which is committed: every member at least two of the scanned
    plugins referenced, crossed against what the jar declares. It is the number
    to quote and the number to move, and it needs nothing but the repository.
    """
    ledger = json.loads(LEDGER.read_text(encoding="utf-8"))
    api = provided(api_jar)
    shared = [
        row for row in ledger["api_members"]
        if row["plugins"] >= ledger["api_members_kept_at_least"]
    ]
    missing = []
    for row in shared:
        owner, _, member = row["member"].partition("#")
        if member not in api.get(owner, ()):
            missing.append(row)

    total = len(shared)
    covered = total - len(missing)
    print(f"ledger: {total} members referenced by at least "
          f"{ledger['api_members_kept_at_least']} of {ledger['plugins_scanned']} plugins")
    print(f"covered by the built API: {covered} ({100 * covered // max(total, 1)}%)")
    print(f"missing: {len(missing)}")
    if missing:
        print()
        print(f"by audience -- plugins that reference each, top {top}:")
        for row in sorted(missing, key=lambda row: -row["plugins"])[:top]:
            print(f"  {row['plugins']:3d}  {row['member']}")

    class_rows = [
        row for row in ledger.get("api_classes", ())
        if row["plugins"] >= ledger["api_members_kept_at_least"]
    ]
    if class_rows:
        missing_classes = [
            row for row in class_rows if row["class"] not in api
        ]
        covered_classes = len(class_rows) - len(missing_classes)
        print()
        print(f"class references in the same shared corpus: {len(class_rows)}")
        print(f"classes present in the built API: {covered_classes}")
        print(f"missing classes: {len(missing_classes)}")
        for row in sorted(
            missing_classes, key=lambda row: (-row["plugins"], row["class"])
        )[:top]:
            print(f"  {row['plugins']:3d}  {row['class']}")


def report_events(source_root, api_jar, top):
    """Which events plugins listen for that no listener could ever receive.

    A member that resolves is not a member that works, and for events the gap is
    total: a listener registers fine against an event class the server never
    constructs, and then simply never runs. `--covered` cannot see this -- the
    class is present, so it counts as covered.

    Subclasses count. `EventBridge.dispatch` selects handlers with
    `isAssignableFrom`, so a listener registered for `EntityDamageEvent`
    receives every `EntityDamageByEntityEvent` too, and calling the base class
    unfired would be measuring a gap that is not there. The hierarchy comes from
    the built jar, because that is what the dispatcher will actually walk.

    A Java declaration is only evidence when a Rust forwarding call reaches
    it. This avoids crediting orphaned `fireX` methods that gameplay never
    invokes.
    """
    ledger = json.loads(LEDGER.read_text(encoding="utf-8"))
    wanted = [row for row in ledger["events"] if row["event"].endswith("Event")]
    constructed = emitted_events(
        source_root, REPO / "foton-plugin" / "src", api_jar)

    # Every ancestor of every constructed event, so a base class counts as
    # reachable when something builds one of its descendants.
    reachable = expand_event_hierarchy(constructed, event_parents(api_jar))

    silent = []
    for row in wanted:
        event = row["event"]
        if event not in reachable:
            silent.append((row["plugins"], event))
    silent.sort(reverse=True)

    fired = len(wanted) - len(silent)
    print(f"event types plugins listen for: {len(wanted)}")
    print(f"a listener could receive: {fired} "
          f"({100 * fired // max(len(wanted), 1)}%)")
    print(f"nothing constructs, directly or as a subclass: {len(silent)}")
    if not silent:
        return
    print()
    print(f"by audience -- plugins that listen and would never be called, top {top}:")
    for plugins, event in silent[:top]:
        print(f"  {plugins:3d}  {event.replace('/', '.')}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "corpus",
        type=pathlib.Path,
        nargs="?",
        help="directory of plugin jars; not needed with --covered",
    )
    parser.add_argument("--write", action="store_true", help="update the committed ledger")
    parser.add_argument("--top", type=int, default=30, help="how many members to print")
    parser.add_argument(
        "--gap",
        type=pathlib.Path,
        help="measure a built API jar against the corpus instead of ranking it",
    )
    parser.add_argument(
        "--events",
        type=pathlib.Path,
        help="which listened-for events nothing in this source tree fires",
    )
    parser.add_argument(
        "--covered",
        type=pathlib.Path,
        help="measure a built API jar against the committed ledger, no corpus needed",
    )
    parser.add_argument(
        "--summary-json",
        type=pathlib.Path,
        help="write machine-readable compatibility evidence (requires --covered)",
    )
    parser.add_argument(
        "--details", action="store_true",
        help="show class, handle, service and reflection evidence for one JAR",
    )
    args = parser.parse_args()

    if args.summary_json and not args.covered:
        parser.error("--summary-json needs --covered <jar>")

    def write_requested_summary():
        if not args.summary_json:
            return
        ledger = json.loads(LEDGER.read_text(encoding="utf-8"))
        wanted = [
            row for row in ledger["events"] if row["event"].endswith("Event")
        ]
        java_source = args.events or REPO / "plugin-api" / "src"
        emitted = emitted_events(
            java_source, REPO / "foton-plugin" / "src", args.covered)
        summary = compatibility_summary(
            ledger,
            provided(args.covered),
            wanted,
            emitted,
            event_parents(args.covered),
        )
        write_summary_json(args.summary_json, summary)
        print(f"wrote {args.summary_json}")

    if args.details:
        if args.corpus is None or not args.corpus.is_file():
            parser.error("--details needs a single JAR path")
        details = scan_details(args.corpus)
        print(json.dumps({key: sorted(value) if isinstance(value, set) else value
                          for key, value in details.items()}, indent=2))
        return

    if args.events:
        if not args.covered:
            parser.error("--events needs --covered <jar> for the class hierarchy")
        report_events(args.events, args.covered, args.top)
        write_requested_summary()
        return

    if args.covered:
        report_covered(args.covered, args.top)
        write_requested_summary()
        return

    if args.corpus is None:
        parser.error("a corpus directory is required unless --covered is given")

    if args.gap:
        report_gap(args.corpus, args.gap, args.top)
        return

    jars = sorted(args.corpus.glob("*.jar"))
    if not jars:
        raise SystemExit(f"no jars in {args.corpus}")

    plugins_per_member = collections.Counter()
    plugins_per_class = collections.Counter()
    plugins_per_internal_class = collections.Counter()
    plugins_per_event = collections.Counter()
    surface_of = {}
    reaches_internal = []
    reachability_unknown = []
    failed = []
    # What each plugin that could ever run needs, which is what the curve is
    # computed from. A plugin reaching internals is excluded: no amount of API
    # would make it work, and leaving it in would flatter every number.
    needs = {}
    incidence_rows = []

    for jar in jars:
        try:
            found, unreadable = scan(jar)
            incidence = plugin_incidence(jar)
        except ARCHIVE_ERRORS as error:
            found = _empty_scan_result()
            unreadable = 0
            incidence = unreadable_archive_incidence(jar, error)
            failed.append((jar.name, str(error)))
        incidence_rows.append(incidence)
        if unreadable:
            details = "; ".join(
                f'{row["entry"]}: {row["reason"]}'
                for row in incidence["unreadable_classes"]
            )
            failed.append(
                (jar.name, f"{unreadable} unreadable class files ({details})"))
        api_classes = classes_on_surface(found["classes"], "api")
        internal_classes = classes_on_surface(found["classes"], "internal")
        internal = incidence["internal"]
        load_bearing_internal = (
            internal["load_bearing_members"] or internal["load_bearing_classes"]
        )
        if incidence["reachability_status"] != "complete":
            reachability_unknown.append(jar.name)
        elif load_bearing_internal:
            reaches_internal.append(jar.name)
        elif found["api"] or api_classes:
            needs[jar.stem] = found["api"] | api_classes
        for kind in ("api", "internal"):
            members = found[kind]
            for member in members:
                plugins_per_member[member] += 1
                surface_of[member] = kind
        for name in found["classes"]:
            surface = surface_of_class(name)
            if surface == "api":
                plugins_per_class[name] += 1
            elif surface == "internal":
                plugins_per_internal_class[name] += 1
        for event in found["handler_events"]:
            plugins_per_event[event] += 1

    scanned = len(jars)
    api = [(m, n) for m, n in plugins_per_member.items() if surface_of[m] == "api"]
    api.sort(key=lambda pair: (-pair[1], pair[0]))

    print(f"corpus: {scanned} plugins read, {len(jars)} jars found")
    print(f"distinct API members referenced: {len(api)}")
    print(
        f"plugins known to reach past the public API: {len(reaches_internal)}"
        f" of {scanned}"
        f" ({100 * len(reaches_internal) // max(scanned, 1)}%) -- these can never run"
    )
    print(
        f"plugins with unknown entrypoint reachability: {len(reachability_unknown)}"
        f" of {scanned}"
    )
    if failed:
        print(f"problems: {len(failed)}")
        for name, why in failed[:5]:
            print(f"  {name}: {why}")

    print(f"\ntop {args.top} members, by how many plugins reference them:")
    for member, count in api[:args.top]:
        print(f"  {count:4}  {member}")

    print("\ntop packages, by how many plugins reach into them:")
    for package, count in reach_by(api, plugins_per_member, package_of)[:18]:
        print(f"  {count:4}  {package}")

    events = sorted(plugins_per_event.items(), key=lambda pair: (-pair[1], pair[0]))
    print("\ntop events, which is what an event system has to carry first:")
    for event, count in events[:18]:
        print(f"  {count:4}  {event.replace('/', '.')}")

    # Ranked among the plugins that could ever run. A member only the
    # internals-reaching ones want is work that serves nobody, and letting it
    # weigh here would raise it up the queue and flatten the curve.
    among_reachable = collections.Counter()
    for members in needs.values():
        for member in members:
            among_reachable[member] += 1
    ranked = [member for member, _ in among_reachable.most_common()]
    curve = coverage_curve(needs, ranked)
    print("\nwhat implementing the top members buys:")
    print("  members   plugins fully covered   median plugin covered")
    for row in curve:
        share = 100 * row["plugins_fully_covered"] // max(scanned, 1)
        print(
            f'  {row["members"]:>6}   {row["plugins_fully_covered"]:>3} / {scanned}'
            f' ({share:>2}%)            {row["median_plugin_covered_percent"]:>3}%'
        )

    if args.write:
        LEDGER.write_text(
            json.dumps(
                {
                    "schema_version": 2,
                    "plugins_scanned": scanned,
                    "plugins_reaching_internals": len(reaches_internal),
                    "plugins_reachability_unknown": len(reachability_unknown),
                    "api_members_referenced": len(api),
                    "api_members_kept_at_least": SHARED_BY,
                    "api_members": [
                        {"member": m, "plugins": n} for m, n in api
                    ],
                    "api_classes_referenced": len(plugins_per_class),
                    "api_classes": [
                        {"class": name, "plugins": count}
                        for name, count in sorted(
                            plugins_per_class.items(),
                            key=lambda pair: (-pair[1], pair[0]),
                        )
                    ],
                    "internal_classes_referenced": len(plugins_per_internal_class),
                    "internal_classes": [
                        {"class": name, "plugins": count}
                        for name, count in sorted(
                            plugins_per_internal_class.items(),
                            key=lambda pair: (-pair[1], pair[0]),
                        )
                    ],
                    "plugin_incidence": sorted(
                        incidence_rows, key=lambda row: row["plugin"]),
                    "packages": [
                        {"package": p, "plugins": n}
                        for p, n in reach_by(api, plugins_per_member, package_of)
                    ],
                    "coverage_curve": curve,
                    "events": [
                        {"event": event, "plugins": count}
                        for event, count in events
                    ],
                },
                indent=1,
            )
            + "\n",
            encoding="utf-8",
        )
        print(f"\nwrote {LEDGER.relative_to(REPO)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
