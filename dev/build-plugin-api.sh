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

if ! command -v javac >/dev/null 2>&1; then
  echo "javac is missing; the plugin API cannot be built" >&2
  echo "install a JDK 21 or newer, or see dev/doctor.sh" >&2
  exit 1
fi

rm -rf "$OUT/classes"
mkdir -p "$OUT/classes"

# javac reads a file of sources with @, which avoids both mapfile (bash 4+,
# and macOS ships bash 3.2) and an argument list long enough to overflow exec.
SOURCES="$OUT/sources.txt"
find "$SRC" -name '*.java' | sort > "$SOURCES"
echo "compiling $(wc -l < "$SOURCES" | tr -d ' ') sources"
# -Xlint:all with no -Werror: the API mirrors another project's shapes and some
# of its warnings are inherent to that, but they are still worth seeing.
javac -Xlint:all -d "$OUT/classes" "@$SOURCES"

jar --create --file "$JAR" -C "$OUT/classes" .
echo "wrote ${JAR#"$REPO"/} ($(du -h "$JAR" | cut -f1), $(find "$OUT/classes" -name '*.class' | wc -l) classes)"

if [ "${1:-}" != "--check" ]; then
  exit 0
fi

LIBS=""
if [ -d "$REPO/plugin-api/lib" ]; then
  LIBS="$(find "$REPO/plugin-api/lib" -name '*.jar' -printf ':%p')"
fi

# The fixture plugin exercises the parts of the event path that are easy to get
# wrong: a rewrite that has to travel back, a veto that has to travel back, and
# a priority order where a later handler must not undo an earlier cancel.
FIXTURE_SRC="$REPO/plugin-api/fixture/src"
if [ -d "$FIXTURE_SRC" ]; then
  FIX="$OUT/fixture"
  rm -rf "$FIX"
  mkdir -p "$FIX/classes"
  javac -nowarn -d "$FIX/classes" -cp "$JAR" "$FIXTURE_SRC"/example/*.java
  cp "$FIXTURE_SRC/plugin.yml" "$FIX/classes/"
  jar --create --file "$FIX/EventFixture.jar" -C "$FIX/classes" .

  mkdir -p "$FIX/plugins"
  mv "$FIX/EventFixture.jar" "$FIX/plugins/"

  cat > "$FIX/Events.java" <<'JAVA'
/** Loads the fixture and checks what its handlers actually decide. */
public final class Events {
    public static void main(String[] args) {
        if (foton.PluginHost.loadAll(args[0]) != 1) {
            throw new AssertionError("the fixture plugin should have enabled");
        }

        String id = "00000000-0000-0000-0000-000000000001";

        String join = foton.EventBridge.fireJoin(id, "original");
        if (!"rewritten by the fixture".equals(join)) {
            throw new AssertionError("a handler's rewrite did not travel back: " + join);
        }

        if (foton.EventBridge.fireChat(id, "hush now") != null) {
            throw new AssertionError("a cancelled chat should come back as nothing");
        }
        if (!"hello".equals(foton.EventBridge.fireChat(id, "hello"))) {
            throw new AssertionError("an uncancelled chat should come back unchanged");
        }

        // The LOWEST handler cancels; the HIGH one would undo it but did not
        // ask to see cancelled events, so it must never run.
        if (foton.EventBridge.fireBlockBreak(id, 1, 2, 3, "minecraft:overworld")) {
            throw new AssertionError("a cancelled break was reported as allowed");
        }

        foton.PluginHost.disableAll();
        System.out.println("event path checked: rewrite, veto and priority all hold");
    }
}
JAVA
  javac -nowarn -d "$FIX" -cp "$JAR$LIBS" "$FIX/Events.java"
  java -cp "$FIX:$JAR$LIBS" Events "$FIX/plugins"
fi

# A jar that compiles proves nothing about whether a plugin can be loaded
# against it. This boots one.
FIXTURE="${FOTON_PLUGIN_FIXTURE:-}"
if [ -z "$FIXTURE" ] || [ ! -f "$FIXTURE" ]; then
  echo "no fixture plugin: set FOTON_PLUGIN_FIXTURE to a plugin jar to check loading"
  echo "(the jar built; only the boot check was skipped)"
  exit 0
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$WORK/plugins"
cp "$FIXTURE" "$WORK/plugins/"

cat > "$WORK/Boot.java" <<'JAVA'
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

javac -nowarn -d "$WORK" -cp "$JAR$LIBS" "$WORK/Boot.java"
java -cp "$WORK:$JAR$LIBS" Boot "$WORK/plugins"
