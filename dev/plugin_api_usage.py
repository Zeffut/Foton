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
import struct
import sys
import zipfile

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
# `api` is the surface a reimplementation can serve. `internal` is the part
# that reaches for the Mojang server or a server implementation's guts, which
# no reimplementation can ever provide -- counting it separately is the only
# honest way to state the ceiling.
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


def gaps(corpus, api_jar):
    """What each plugin still calls that the jar cannot answer.

    Only the `api` surface. A plugin reaching into net.minecraft or CraftBukkit
    is out of reach by construction and saying otherwise would be a lie about
    the ceiling.
    """
    have = provided(api_jar)
    per_plugin = {}
    missing_audience = collections.Counter()
    for jar in sorted(corpus.glob("*.jar")):
        found, _ = scan(jar)
        if found["internal"]:
            continue
        wanted = found["api"]
        if not wanted:
            continue
        missing = set()
        for member in wanted:
            owner, signature = member.split("#", 1)
            if signature not in FROM_OBJECT and signature not in have.get(owner, ()):
                missing.add(member)
        per_plugin[jar.name] = (len(wanted), missing)
        for member in missing:
            missing_audience[member] += 1
    return per_plugin, missing_audience


def scan(jar):
    """What one plugin jar references, as {surface: {member, ...}}.

    A jar that cannot be read at all is reported rather than skipped silently:
    a corpus that quietly lost half its entries would produce a ranking that
    looks exactly as authoritative as a correct one.
    """
    found = {kind: set() for kind in set(SURFACES.values())}
    unreadable = 0
    with zipfile.ZipFile(jar) as archive:
        for entry in archive.namelist():
            if not entry.endswith(".class"):
                continue
            try:
                for surface, member in references(archive.read(entry)):
                    found[surface].add(member)
            except (NotAClassFile, struct.error, KeyError):
                unreadable += 1
    return found, unreadable


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


def event_reach(events):
    """Ranks event *types* rather than the accessors called on them."""
    best = {}
    for member, count in events:
        owner = member.split("#", 1)[0]
        best[owner] = max(best.get(owner, 0), count)
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

def report_gap(corpus, api_jar, top):
    """Prints how far the built jar gets, and what is next by audience."""
    per_plugin, missing_audience = gaps(corpus, api_jar)
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("corpus", type=pathlib.Path, help="directory of plugin jars")
    parser.add_argument("--write", action="store_true", help="update the committed ledger")
    parser.add_argument("--top", type=int, default=30, help="how many members to print")
    parser.add_argument(
        "--gap",
        type=pathlib.Path,
        help="measure a built API jar against the corpus instead of ranking it",
    )
    args = parser.parse_args()

    if args.gap:
        report_gap(args.corpus, args.gap, args.top)
        return

    jars = sorted(args.corpus.glob("*.jar"))
    if not jars:
        raise SystemExit(f"no jars in {args.corpus}")

    plugins_per_member = collections.Counter()
    surface_of = {}
    reaches_internal = []
    failed = []
    # What each plugin that could ever run needs, which is what the curve is
    # computed from. A plugin reaching internals is excluded: no amount of API
    # would make it work, and leaving it in would flatter every number.
    needs = {}

    for jar in jars:
        try:
            found, unreadable = scan(jar)
        except (zipfile.BadZipFile, OSError) as error:
            failed.append((jar.name, str(error)))
            continue
        if unreadable:
            failed.append((jar.name, f"{unreadable} unreadable class files"))
        if found["internal"]:
            reaches_internal.append(jar.name)
        elif found["api"]:
            needs[jar.stem] = found["api"]
        for kind, members in found.items():
            for member in members:
                plugins_per_member[member] += 1
                surface_of[member] = kind

    scanned = len(jars) - len([f for f in failed if "unreadable" not in f[1]])
    api = [(m, n) for m, n in plugins_per_member.items() if surface_of[m] == "api"]
    api.sort(key=lambda pair: (-pair[1], pair[0]))

    print(f"corpus: {scanned} plugins read, {len(jars)} jars found")
    print(f"distinct API members referenced: {len(api)}")
    print(
        f"plugins reaching past the public API: {len(reaches_internal)}"
        f" of {scanned}"
        f" ({100 * len(reaches_internal) // max(scanned, 1)}%) -- these can never run"
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

    events = [(m, n) for m, n in api if "/event/" in m]
    print("\ntop events, which is what an event system has to carry first:")
    for member, count in event_reach(events)[:18]:
        print(f"  {count:4}  {member}")

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
                    "plugins_scanned": scanned,
                    "plugins_reaching_internals": len(reaches_internal),
                    "api_members_referenced": len(api),
                    "api_members_kept_at_least": SHARED_BY,
                    "api_members": [
                        {"member": m, "plugins": n} for m, n in api if n >= SHARED_BY
                    ],
                    "packages": [
                        {"package": p, "plugins": n}
                        for p, n in reach_by(api, plugins_per_member, package_of)
                    ],
                    "coverage_curve": curve,
                    "events": [
                        {"event": e, "plugins": n}
                        for e, n in event_reach(
                            [(m, n) for m, n in api if "/event/" in m]
                        )
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
