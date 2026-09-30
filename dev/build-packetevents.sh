#!/usr/bin/env bash
# Builds the `packetevents` plugin Foton ships: PacketEvents' own API, on
# Foton's packet tap.
#
# The upstream PacketEvents plugin cannot run on Foton. It finds Minecraft's
# Netty pipeline by reflecting into CraftBukkit and splices its handlers in,
# and Foton has neither. PacketEvents is built for several platforms, though,
# and what a platform supplies is small: where packets come from, where they
# go, and which channel belongs to which player. `plugin-api/packetevents/`
# is that platform for Foton. Everything else -- the event pipeline, every
# packet wrapper, the registries and mappings for each protocol -- is
# PacketEvents' own, unmodified, pinned below by digest.
#
# Unlike plugin-api/lib, these jars are fetched rather than committed: the
# plugin is optional, and five megabytes of GPL binaries in the repository
# would be paid for by every clone for the sake of a component most servers
# never load. A server that wants it builds it once; the jars are cached.
#
#     bash dev/build-packetevents.sh    # after dev/build-plugin-api.sh
#
# Output: plugin-api/build/bundled/packetevents.jar, which the plugin host
# loads before the plugins directory (see foton/PluginHost.java).
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC="$REPO/plugin-api/packetevents"
OUT="$REPO/plugin-api/build/packetevents"
CACHE="$OUT/libs"
API_JAR="$REPO/plugin-api/build/foton-plugin-api.jar"
BUNDLED="$REPO/plugin-api/build/bundled"
# Maven Central answers bursts with 429 through some proxies; Paper's
# repository mirrors it and hosts nothing that would shadow these coordinates.
MIRROR="${FOTON_MAVEN_MIRROR:-https://repo.papermc.io/repository/maven-public}"
CODEMC="https://repo.codemc.io/repository/maven-releases"

# repository path/in/maven artifact version sha256
#
# PacketEvents 2.13.0 is the release that knows protocol 776, Minecraft 26.2,
# which is what Foton speaks. Netty is the buffer implementation PacketEvents'
# wrappers read and write through; Foton has no Netty of its own, so this is
# the version Paper 26.2 build 129 carries (META-INF/libraries.list). Adventure's
# NBT module and Examination are what PacketEvents' bundled text serializers
# link against and a Paper server has on its class path; they go inside this
# plugin, not on every plugin's class path.
PINNED=$(cat <<'LIST'
codemc com/github/retrooper packetevents-api 2.13.0 c7feb88872d9065037d45190de2e96f69ebe039f2e9b28b1e74669595107ee8f
codemc com/github/retrooper packetevents-netty-common 2.13.0 89b44ffea051a1242aee4e0703cf5722c436f7354ad9d52148c8d6ef0d0ae843
mirror io/netty netty-buffer 4.2.15.Final 1361fd9c9ba85b9831cf54a1b2e45ddc3ce34a768931726c099d3f5ef0efe4a3
mirror io/netty netty-common 4.2.15.Final 78206aa7f6d197caa926291408c01889b6b910ca0f74017d3fcbdaccf9562959
mirror net/kyori adventure-nbt 5.2.0 834e94d6c883ac5dba43054632bc383eee6e39bd42ba1b87b6b6f47bcf84554b
mirror net/kyori examination-api 1.3.0 c9237ffecb05428f6eff86216246ac70ce0b47b04c08ea7ca35020fde57f8492
mirror net/kyori examination-string 1.3.0 7d01fc25a4bb3af0e1662685455f4541fbf4626216ea5846e455c1491e156b8c
LIST
)

if [ ! -f "$API_JAR" ]; then
  echo "no plugin API jar at ${API_JAR#"$REPO"/}; run dev/build-plugin-api.sh first" >&2
  exit 1
fi

digest() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1
  else shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

mkdir -p "$CACHE"
LIBS=""
while read -r repo group artifact version sha; do
  [ -n "$artifact" ] || continue
  jar="$CACHE/$artifact-$version.jar"
  if [ ! -f "$jar" ] || [ "$(digest "$jar")" != "$sha" ]; then
    base="$MIRROR"; [ "$repo" = "codemc" ] && base="$CODEMC"
    url="$base/$group/$artifact/$version/$artifact-$version.jar"
    echo "fetching $artifact $version"
    fetched=0
    for delay in 0 2 4 8 16; do
      sleep "$delay"
      if curl -sSfL "$url" -o "$jar.part"; then fetched=1; break; fi
    done
    if [ "$fetched" != 1 ]; then
      echo "could not fetch $url" >&2
      exit 1
    fi
    mv "$jar.part" "$jar"
  fi
  actual="$(digest "$jar")"
  if [ "$actual" != "$sha" ]; then
    echo "$artifact-$version.jar has digest $actual, expected $sha" >&2
    rm -f "$jar"
    exit 1
  fi
  LIBS="$LIBS:$jar"
done <<< "$PINNED"

CP="$API_JAR"
for jar in "$REPO/plugin-api/lib/"*.jar; do CP="$CP:$jar"; done
CP="$CP$LIBS"

rm -rf "$OUT/classes"
mkdir -p "$OUT/classes" "$BUNDLED"
javac --release 21 -Xlint:all -cp "$CP" -d "$OUT/classes" $(find "$SRC/src" -name '*.java' | sort)
cp "$SRC/plugin.yml" "$SRC/config.yml" "$OUT/classes/"
mkdir -p "$OUT/classes/META-INF"
cp "$SRC/NOTICE" "$OUT/classes/META-INF/NOTICE-packetevents"

# One jar, as the upstream plugin is: a plugin's own class loader is the only
# place these classes should be visible from. Module descriptors, signatures
# and multi-release overlays are dropped -- the merged jar is none of those
# things, and a stale signature would make the JVM refuse it outright.
python3 - "$OUT/classes" "$BUNDLED/packetevents.jar" ${LIBS//:/ } <<'PY'
import pathlib, sys, zipfile

classes, target, *libraries = sys.argv[1:]
skip = ("module-info.class", "META-INF/versions/", "META-INF/MANIFEST.MF")
signature = (".SF", ".RSA", ".DSA", ".EC")
seen = set()
with zipfile.ZipFile(target, "w", zipfile.ZIP_DEFLATED) as out:
    out.writestr("META-INF/MANIFEST.MF", "Manifest-Version: 1.0\r\n\r\n")
    seen.add("META-INF/MANIFEST.MF")
    for path in sorted(pathlib.Path(classes).rglob("*")):
        if path.is_file():
            name = path.relative_to(classes).as_posix()
            out.write(path, name)
            seen.add(name)
    for library in libraries:
        if not library:
            continue
        with zipfile.ZipFile(library) as jar:
            for entry in jar.infolist():
                name = entry.filename
                if entry.is_dir() or name in seen:
                    continue
                if name.endswith("module-info.class") or name.startswith(skip):
                    continue
                if name.startswith("META-INF/") and name.upper().endswith(signature):
                    continue
                out.writestr(entry, jar.read(name))
                seen.add(name)
PY

echo "wrote ${BUNDLED#"$REPO"/}/packetevents.jar ($(du -h "$BUNDLED/packetevents.jar" | cut -f1))"

# PacketEvents reads packets by its own id tables; Foton writes them by the
# ids extracted from vanilla. If the two disagree, every listener acts on the
# wrong packet and nothing says so -- so the build refuses the jar instead.
CHECK="$OUT/check"
rm -rf "$CHECK"
mkdir -p "$CHECK"
RUN_CP="$BUNDLED/packetevents.jar:$API_JAR"
for jar in "$REPO/plugin-api/lib/"*.jar; do RUN_CP="$RUN_CP:$jar"; done
javac -nowarn -d "$CHECK" -cp "$RUN_CP" "$SRC/check/PacketIdCheck.java"
if ! java -cp "$CHECK:$RUN_CP" PacketIdCheck "$REPO/foton-registry/build_assets/packets.json"; then
  rm -f "$BUNDLED/packetevents.jar"
  echo "packetevents.jar removed: its packet ids do not match Foton's" >&2
  exit 1
fi
