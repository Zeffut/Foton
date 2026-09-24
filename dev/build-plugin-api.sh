#!/usr/bin/env bash
# Builds the Bukkit-compatible API a plugin is loaded against.
#
# Java, not Rust, because a Bukkit plugin is a JVM artifact compiled against
# JVM types: the classes it extends and the interfaces it implements have to
# exist as real classes before it can even be loaded. What those classes *do*
# is Foton's business and lives on the other side of JNI; what they *are* is
# fixed by twelve years of other people's compiled code.
#
# Which members exist is not a matter of taste. `dev/plugin-api-usage.json`
# ranks what a corpus of real plugins actually calls, and this grows in that
# order.
#
#     bash dev/build-plugin-api.sh          # build the jar
#     bash dev/build-plugin-api.sh --check   # build, then boot a real plugin
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC="$REPO/plugin-api/src"
OUT="$REPO/plugin-api/build"
JAR="$OUT/foton-plugin-api.jar"
mkdir -p "$OUT"

# Every invocation owns all of its generated sources, classes and checks.
# Keeping these under the system temporary directory prevents two CI jobs (or
# a developer and an E2E test) from deleting files while the other javac is
# still reading them. Only the completed jar is staged beside its destination
# for the final same-filesystem atomic rename.
BUILD="$(mktemp -d "${TMPDIR:-/tmp}/foton-plugin-api.XXXXXX")"
PUBLISH=""
FIXTURE_WORK=""
cleanup() {
  if [ -n "$FIXTURE_WORK" ]; then
    rm -rf -- "$FIXTURE_WORK"
  fi
  if [ -n "$PUBLISH" ]; then
    rm -f -- "$PUBLISH"
  fi
  rm -rf -- "$BUILD"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

CLASSES="$BUILD/classes"
GENERATED="$BUILD/generated"
BUILD_JAR="$BUILD/foton-plugin-api.jar"
mkdir -p "$CLASSES" "$GENERATED"

if ! command -v javac >/dev/null 2>&1; then
  echo "javac is missing; the plugin API cannot be built" >&2
  echo "install a JDK 21 or newer, or see dev/doctor.sh" >&2
  exit 1
fi

# Material is sixteen hundred constants over every block and item. It is
# generated from the same registry files the server itself is built from, so
# the enum cannot name a block Foton does not have -- and so there is no
# hand-written second copy to drift.
python3 "$REPO/dev/gen-material.py" "$GENERATED"
python3 "$REPO/dev/gen-entity-type.py" "$GENERATED"
python3 "$REPO/dev/gen-enchantment.py" "$GENERATED"
python3 "$REPO/dev/gen-potion-type.py" "$GENERATED"

# The API compiles against Adventure, Brigadier, Guava and the rest, which are
# committed in plugin-api/lib. Check them before use rather than trusting the
# directory: the set once held two incompatible Adventures and compiled anyway.
if ! bash "$REPO/dev/fetch-plugin-api-libs.sh" --check; then
  echo "run dev/fetch-plugin-api-libs.sh to restore them" >&2
  exit 1
fi
LIBS=""
for library in "$REPO"/plugin-api/lib/*.jar; do
  [ -f "$library" ] && [ ! -L "$library" ] || continue
  LIBS="${LIBS:+$LIBS:}$library"
done
API_CP="$BUILD_JAR${LIBS:+:$LIBS}"

# javac reads a file of sources with @, which avoids both mapfile (bash 4+,
# and macOS ships bash 3.2) and an argument list long enough to overflow exec.
SOURCES="$BUILD/sources.txt"
find "$SRC" "$GENERATED" -type f -name '*.java' | sort | while IFS= read -r source; do
  # Javac argfiles split on whitespace unless a path is quoted. Escape the two
  # characters that are significant inside a quoted argfile token.
  source=${source//\\/\\\\}
  source=${source//\"/\\\"}
  printf '"%s"\n' "$source"
done > "$SOURCES"
echo "compiling $(wc -l < "$SOURCES" | tr -d ' ') sources"
# -Xlint:all with no -Werror: the API mirrors another project's shapes and some
# of its warnings are inherent to that, but they are still worth seeing.
# --release 21, not whatever JDK happens to be on PATH. A jar built by a
# JDK 25 carries class file version 69, and a server on the Java 21 that
# Paper 26.2 itself requires cannot load it -- the plugin host dies at
# startup with an exception that names none of this.
javac --release 21 -Xlint:all -cp "$LIBS" -d "$CLASSES" "@$SOURCES"

# EssentialsX (and older Bukkit consumers) were compiled against the pre-generic BanEntry ABI, whose erased getTarget return type is String.
# Add a default binary bridge while retaining the generic Object method.
python3 "$REPO/dev/add-banentry-bridge.py" "$CLASSES/org/bukkit/BanEntry.class"

jar --create --file "$BUILD_JAR" -C "$CLASSES" .

# `mv` is an atomic replacement because the unique staging file lives in the
# destination directory. Readers therefore observe either the previous valid
# jar or this complete one, never bytes from an in-progress `jar --create`.
PUBLISH="$(mktemp "$OUT/.foton-plugin-api.jar.XXXXXX")"
cp "$BUILD_JAR" "$PUBLISH"
chmod 0644 "$PUBLISH"
mv -f -- "$PUBLISH" "$JAR"
PUBLISH=""
echo "wrote ${JAR#"$REPO"/} ($(du -h "$JAR" | cut -f1), $(find "$CLASSES" -name '*.class' | wc -l) classes)"

if [ "${1:-}" != "--check" ]; then
  exit 0
fi

# The fixture plugin exercises the parts of the event path that are easy to get
# wrong: a rewrite that has to travel back, a veto that has to travel back, and
# a priority order where a later handler must not undo an earlier cancel.
FIXTURE_SRC="$REPO/plugin-api/fixture/src"
if [ -d "$FIXTURE_SRC" ]; then
  FIX="$BUILD/fixture"
  mkdir -p "$FIX/classes"
  javac -nowarn -d "$FIX/classes" -cp "$API_CP" "$FIXTURE_SRC"/example/*.java
  cp "$FIXTURE_SRC/plugin.yml" "$FIX/classes/"
  cp "$FIXTURE_SRC/config.yml" "$FIX/classes/"
  jar --create --file "$FIX/EventFixture.jar" -C "$FIX/classes" .

  mkdir -p "$FIX/plugins"
  mv "$FIX/EventFixture.jar" "$FIX/plugins/"

  # The checks read the fixture's counters, so they compile against its
  # classes. The jar in plugins/ is still what gets loaded; this is only so
  # the names resolve.
  javac -nowarn -d "$FIX" -cp "$API_CP:$FIX/classes" "$REPO"/plugin-api/check/*.java
  java -cp "$FIX:$API_CP:$FIX/classes" Checks "$FIX/plugins"
fi

# A jar that compiles proves nothing about whether a plugin can be loaded
# against it. This boots one.
FIXTURE="${FOTON_PLUGIN_FIXTURE:-}"
if [ -z "$FIXTURE" ] || [ ! -f "$FIXTURE" ]; then
  echo "no fixture plugin: set FOTON_PLUGIN_FIXTURE to a plugin jar to check loading"
  echo "(the jar built; only the boot check was skipped)"
  exit 0
fi

FIXTURE_WORK="$(mktemp -d "${TMPDIR:-/tmp}/foton-plugin-fixture.XXXXXX")"
mkdir -p "$FIXTURE_WORK/plugins"
cp "$FIXTURE" "$FIXTURE_WORK/plugins/"

cat > "$FIXTURE_WORK/Boot.java" <<'JAVA'
public final class Boot {
    public static void main(String[] args) {
        int enabled = foton.PluginHost.loadAll(args[0]);
        foton.PluginHost.disableAll();
        if (enabled < 1) {
            System.err.println("no plugin enabled");
            System.exit(1);
        }
        System.out.println("booted " + enabled + " plugin(s)");
    }
}
JAVA

javac -nowarn -d "$FIXTURE_WORK" -cp "$API_CP" "$FIXTURE_WORK/Boot.java"
java -cp "$FIXTURE_WORK:$API_CP" Boot "$FIXTURE_WORK/plugins"
