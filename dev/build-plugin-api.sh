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

# Java generators consume extracted registry assets and Rust registry source
# emitted by the foton-registry build script. The Rust files are intentionally
# ignored and can survive a branch checkout, so an existing entity source is
# accepted only when its consumed constants match the extracted registry.
REGISTRY_GENERATED="$REPO/foton-registry/src/generated"
REGISTRY_INPUTS=(
  "$REGISTRY_GENERATED/vanilla_entities.rs"
  "$REGISTRY_GENERATED/vanilla_enchantments.rs"
  "$REGISTRY_GENERATED/vanilla_potions.rs"
  "$REGISTRY_GENERATED/vanilla_banner_patterns.rs"
  "$REGISTRY_GENERATED/vanilla_biomes.rs"
  "$REGISTRY_GENERATED/vanilla_trim_materials.rs"
  "$REGISTRY_GENERATED/vanilla_trim_patterns.rs"
  "$REGISTRY_GENERATED/vanilla_instruments.rs"
)
entity_registry_is_current() {
  python3 - "$REPO/foton-registry/build_assets/entities.json" \
    "$REGISTRY_GENERATED/vanilla_entities.rs" <<'PY'
import json
from pathlib import Path
import re
import sys

asset = Path(sys.argv[1])
generated = Path(sys.argv[2])
if not generated.is_file():
    raise SystemExit(1)
expected = [entry["name"].upper() for entry in json.loads(asset.read_text(encoding="utf-8"))]
actual = re.findall(r"pub static ([A-Z][A-Z0-9_]*)\s*:", generated.read_text(encoding="utf-8"))
raise SystemExit(0 if actual == expected else 1)
PY
}

GENERATE_REGISTRY=false
for registry_input in "${REGISTRY_INPUTS[@]}"; do
  if [ ! -f "$registry_input" ]; then
    GENERATE_REGISTRY=true
    break
  fi
done
if ! entity_registry_is_current; then
  GENERATE_REGISTRY=true
fi
if [ "$GENERATE_REGISTRY" = true ]; then
  if ! command -v cargo >/dev/null 2>&1; then
    echo "cargo is missing; generated plugin API registry inputs are unavailable" >&2
    exit 1
  fi
  echo "refreshing missing or stale foton-registry sources"
  # Updating this tracked input's timestamp invalidates only the registry build
  # script. Unlike cargo clean, it preserves dependency artifacts and cannot
  # trigger an otherwise unnecessary asset rebuild.
  touch "$REPO/foton-registry/build/build.rs"
  cargo check --manifest-path "$REPO/Cargo.toml" -p foton-registry
fi
MISSING_REGISTRY_INPUTS=()
for registry_input in "${REGISTRY_INPUTS[@]}"; do
  if [ ! -f "$registry_input" ]; then
    MISSING_REGISTRY_INPUTS+=("$registry_input")
  fi
done
if [ "${#MISSING_REGISTRY_INPUTS[@]}" -ne 0 ]; then
  echo "foton-registry build did not generate required plugin API inputs:" >&2
  for registry_input in "${MISSING_REGISTRY_INPUTS[@]}"; do
    echo "  ${registry_input#"$REPO/"}" >&2
  done
  exit 1
fi
if ! entity_registry_is_current; then
  echo "foton-registry generated stale vanilla_entities.rs" >&2
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
python3 "$REPO/dev/gen-attribute.py" "$REPO/foton-registry/build_assets/attributes.json" "$GENERATED"
python3 "$REPO/dev/gen-potion-effect-type.py" "$REPO/foton-registry/build_assets/mob_effects.json" "$GENERATED"
python3 "$REPO/dev/gen-particle.py" "$GENERATED"
python3 "$REPO/dev/gen-registry-values.py" "$GENERATED"
python3 "$REPO/dev/gen-game-rules.py" "$GENERATED"

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
write_javac_argfile() {
  local destination="$1"
  local excluded_name="$2"
  shift 2
  local source
  while IFS= read -r -d '' source; do
    [ "$(basename "$source")" = "$excluded_name" ] && continue
    source=${source//\\/\\\\}
    source=${source//\"/\\\"}
    printf '"%s"\n' "$source"
  done < <(find "$@" -type f -name '*.java' -print0) | LC_ALL=C sort > "$destination"
}

SOURCES="$BUILD/sources.txt"
write_javac_argfile "$SOURCES" "" "$SRC" "$GENERATED"
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

# This consumer is compiled against the committed Paper 26.2 ABI fixture, then
# run against Foton's jar with the fixture deliberately omitted. This verifies
# both Builder's interface linkage and the inherited erased build() descriptor
# without depending on a cache, override, download, or network.
PAPER_ABI_FIXTURE_SRC="$REPO/plugin-api/check/paper-26.2-abi-fixture/src"
PAPER_ABI_FIXTURE_CLASSES="$BUILD/paper-26.2-abi-fixture-classes"
PAPER_BINARY_CLASSES="$BUILD/paper-binary-classes"
FOTON_BINARY_RUNNER_CLASSES="$BUILD/foton-binary-runner-classes"
rm -rf "$PAPER_ABI_FIXTURE_CLASSES" "$PAPER_BINARY_CLASSES" "$FOTON_BINARY_RUNNER_CLASSES"
mkdir -p "$PAPER_ABI_FIXTURE_CLASSES" "$PAPER_BINARY_CLASSES" "$FOTON_BINARY_RUNNER_CLASSES"
write_javac_argfile "$BUILD/paper-26.2-abi-fixture-sources.txt" "" \
  "$PAPER_ABI_FIXTURE_SRC"
javac --release 21 -nowarn -d "$PAPER_ABI_FIXTURE_CLASSES" -cp "$API_CP" \
  @"$BUILD/paper-26.2-abi-fixture-sources.txt"
javac --release 21 -nowarn -d "$PAPER_BINARY_CLASSES" \
  -cp "$PAPER_ABI_FIXTURE_CLASSES:$API_CP" \
  "$REPO/plugin-api/check/PaperAttributeConsumer.java" \
  "$REPO/plugin-api/check/PaperAttributeOldEnumConsumer.java" \
  "$REPO/plugin-api/check/PaperResolvableProfileConsumer.java" \
  "$REPO/plugin-api/check/PaperRecipeChoiceConsumer.java" \
  "$REPO/plugin-api/check/PaperSpawnReasonConsumer.java"
javac --release 21 -nowarn -d "$FOTON_BINARY_RUNNER_CLASSES" \
  -cp "$PAPER_BINARY_CLASSES:$API_CP" \
  "$REPO/plugin-api/check/FotonAttributeBinaryRunner.java" \
  "$REPO/plugin-api/check/FotonPotionLookupRunner.java" \
  "$REPO/plugin-api/check/FotonResolvableProfileBinaryRunner.java" \
  "$REPO/plugin-api/check/FotonRecipeChoiceBinaryRunner.java"
# PAPER_ABI_FIXTURE_CLASSES is intentionally absent: Foton must supply every
# linked Paper type at runtime.
java -cp "$FOTON_BINARY_RUNNER_CLASSES:$API_CP" \
  FotonPotionLookupRunner
java -cp "$PAPER_BINARY_CLASSES:$FOTON_BINARY_RUNNER_CLASSES:$API_CP" \
  FotonAttributeBinaryRunner
java -cp "$PAPER_BINARY_CLASSES:$FOTON_BINARY_RUNNER_CLASSES:$API_CP" \
  FotonResolvableProfileBinaryRunner
java -cp "$PAPER_BINARY_CLASSES:$FOTON_BINARY_RUNNER_CLASSES:$API_CP" \
  FotonRecipeChoiceBinaryRunner

java -cp "$PAPER_BINARY_CLASSES:$API_CP" PaperSpawnReasonConsumer

# The fixture plugin exercises the parts of the event path that are easy to get
# wrong: a rewrite that has to travel back, a veto that has to travel back, and
# a priority order where a later handler must not undo an earlier cancel.
FIXTURE_SRC="$REPO/plugin-api/fixture/src"
if [ -d "$FIXTURE_SRC" ]; then
  FIX="$BUILD/fixture"
  EVENT_CLASSES="$FIX/event-classes"
  LIFECYCLE_CLASSES="$FIX/lifecycle-classes"
  DEPENDENCY_CLASSES="$FIX/dependency-classes"
  LIBRARY_CLASSES="$FIX/library-classes"
  LIBRARY_PLUGIN_CLASSES="$FIX/library-plugin-classes"
  PAPER_CLASSES="$FIX/paper-classes"
  HOST_RUNTIME_CLASSES="$FIX/host-runtime-classes"
  HOST_DRIVER_CLASSES="$FIX/host-driver-classes"
  CHECK_CLASSES="$FIX/check-classes"
  mkdir -p "$EVENT_CLASSES" "$LIFECYCLE_CLASSES" "$DEPENDENCY_CLASSES" \
    "$LIBRARY_CLASSES" "$LIBRARY_PLUGIN_CLASSES" "$PAPER_CLASSES" \
    "$HOST_RUNTIME_CLASSES" "$HOST_DRIVER_CLASSES" "$CHECK_CLASSES"

  javac -nowarn -d "$CHECK_CLASSES" -cp "$API_CP" "$FIXTURE_SRC"/support/*.java
  javac -nowarn -d "$EVENT_CLASSES" -cp "$API_CP" \
    "$FIXTURE_SRC/example/EventFixture.java"
  cp "$FIXTURE_SRC/plugin.yml" "$EVENT_CLASSES/"
  cp "$FIXTURE_SRC/config.yml" "$EVENT_CLASSES/"
  jar --create --file "$FIX/EventFixture.jar" -C "$EVENT_CLASSES" .

  write_javac_argfile "$FIX/lifecycle-sources.txt" "EventFixture.java" \
    "$FIXTURE_SRC/example"
  javac -nowarn -d "$LIFECYCLE_CLASSES" -cp "$API_CP:$CHECK_CLASSES" \
    @"$FIX/lifecycle-sources.txt"
  javac -nowarn -d "$DEPENDENCY_CLASSES" -cp "$API_CP" \
    "$REPO"/plugin-api/fixture/dependencies/src/*.java
  javac -nowarn -d "$LIBRARY_CLASSES" \
    "$REPO"/plugin-api/fixture/libraries/src/fixture/libraries/DescriptorLibraryApi.java \
    "$REPO"/plugin-api/fixture/libraries/src/fixture/libraries/PaperLibraryApi.java
  javac -nowarn -d "$LIBRARY_PLUGIN_CLASSES" -cp "$API_CP:$LIBRARY_CLASSES" \
    "$REPO"/plugin-api/fixture/libraries/src/fixture/libraries/LibraryPlugin.java \
    "$REPO"/plugin-api/fixture/libraries/src/fixture/libraries/MalformedLibraryPlugin.java
  javac -nowarn -d "$PAPER_CLASSES" -cp "$API_CP" \
    "$REPO"/plugin-api/fixture/paper/src/example/*.java
  javac -nowarn -d "$HOST_DRIVER_CLASSES" \
    "$REPO/plugin-api/fixture/host-runtime/src/fixture/jdbc/HostDriver.java"
  HOST_DRIVER_JAR="$FIX/HostDriver.jar"
  jar --create --file "$HOST_DRIVER_JAR" \
    -C "$HOST_DRIVER_CLASSES" . \
    -C "$REPO/plugin-api/fixture/host-runtime/host-services" .
  javac -nowarn -d "$HOST_RUNTIME_CLASSES" -cp "$API_CP" \
    "$REPO"/plugin-api/fixture/host-runtime/src/example/*.java

  mkdir -p "$FIX/plugins"
  mv "$FIX/EventFixture.jar" "$FIX/plugins/"

  LIFECYCLE_PLUGINS="$FIX/lifecycle-plugins"
  REPLACEMENT_PLUGINS="$FIX/replacement-plugins"
  mkdir -p "$LIFECYCLE_PLUGINS" "$REPLACEMENT_PLUGINS"
  for fixture in FailingEnable EnableDependent FailingLoad LoadDependent Healthy; do
    class="${fixture}Plugin"
    STAGE="$FIX/stage-$fixture"
    mkdir -p "$STAGE/example"
    cp "$LIFECYCLE_CLASSES/example/$class.class" "$STAGE/example/"
    if [ "$fixture" = "FailingEnable" ] || [ "$fixture" = "FailingLoad" ]; then
      cp "$LIFECYCLE_CLASSES/example/FailingEvent.class" "$STAGE/example/"
    fi
    cp "$REPO/plugin-api/fixture/lifecycle/$fixture/plugin.yml" "$STAGE/"
    jar --create --file "$LIFECYCLE_PLUGINS/$fixture.jar" -C "$STAGE" .
  done

  LIBRARY_PLUGINS="$FIX/library-plugins"
  MALFORMED_LIBRARY_PLUGINS="$FIX/malformed-library-plugins"
  LIBRARY_CACHE="$LIBRARY_PLUGINS/.foton-libraries"
  mkdir -p "$LIBRARY_PLUGINS" "$MALFORMED_LIBRARY_PLUGINS" \
    "$LIBRARY_CACHE/example/fixture/descriptor-library/1.2.3" \
    "$LIBRARY_CACHE/example/fixture/paper-library/4.5.6"
  LIBRARY_STAGE="$FIX/stage-library"
  mkdir -p "$LIBRARY_STAGE/fixture/libraries"
  cp "$LIBRARY_PLUGIN_CLASSES/fixture/libraries/LibraryPlugin.class" \
    "$LIBRARY_STAGE/fixture/libraries/"
  cp "$REPO/plugin-api/fixture/libraries/valid/plugin.yml" \
    "$REPO/plugin-api/fixture/libraries/valid/paper-libraries.json" "$LIBRARY_STAGE/"
  jar --create --file "$LIBRARY_PLUGINS/LibraryFixture.jar" -C "$LIBRARY_STAGE" .
  DESCRIPTOR_CACHE_JAR="$LIBRARY_CACHE/example/fixture/descriptor-library/1.2.3/descriptor-library-1.2.3.jar"
  PAPER_CACHE_JAR="$LIBRARY_CACHE/example/fixture/paper-library/4.5.6/paper-library-4.5.6.jar"
  jar --create --file "$DESCRIPTOR_CACHE_JAR" \
    -C "$LIBRARY_CLASSES" fixture/libraries/DescriptorLibraryApi.class
  jar --create --file "$PAPER_CACHE_JAR" \
    -C "$LIBRARY_CLASSES" fixture/libraries/PaperLibraryApi.class
  INTERRUPTED_CACHE_JAR="$DESCRIPTOR_CACHE_JAR.interrupted.tmp"
  printf 'truncated' > "$INTERRUPTED_CACHE_JAR"

  for descriptor in "$REPO/plugin-api/fixture/libraries/malformed"/*/plugin.yml; do
    fixture="$(basename "$(dirname "$descriptor")")"
    STAGE="$FIX/stage-library-malformed-$fixture"
    mkdir -p "$STAGE/fixture/libraries"
    cp "$LIBRARY_PLUGIN_CLASSES/fixture/libraries/MalformedLibraryPlugin.class" \
      "$STAGE/fixture/libraries/"
    cp "$descriptor" "$STAGE/"
    if [ -f "$(dirname "$descriptor")/paper-libraries.json" ]; then
      cp "$(dirname "$descriptor")/paper-libraries.json" "$STAGE/"
    fi
    jar --create --file "$MALFORMED_LIBRARY_PLUGINS/$fixture.jar" -C "$STAGE" .
  done
  # Keep the RED run offline even if an unsafe implementation accepts one of
  # these coordinates and constructs its escaped path.
  EMPTY_LIBRARY="$FIX/empty-library"
  mkdir -p "$EMPTY_LIBRARY" \
    "$MALFORMED_LIBRARY_PLUGINS/group-library/1.0" \
    "$MALFORMED_LIBRARY_PLUGINS/paper-library/1.0" \
    "$MALFORMED_LIBRARY_PLUGINS/.foton-libraries/example/1.0" \
    "$MALFORMED_LIBRARY_PLUGINS/.foton-libraries/example/fixture"
  jar --create --file \
    "$MALFORMED_LIBRARY_PLUGINS/group-library/1.0/group-library-1.0.jar" \
    -C "$EMPTY_LIBRARY" .
  jar --create --file \
    "$MALFORMED_LIBRARY_PLUGINS/paper-library/1.0/paper-library-1.0.jar" \
    -C "$EMPTY_LIBRARY" .
  jar --create --file \
    "$MALFORMED_LIBRARY_PLUGINS/.foton-libraries/example/1.0/..-1.0.jar" \
    -C "$EMPTY_LIBRARY" .
  jar --create --file \
    "$MALFORMED_LIBRARY_PLUGINS/.foton-libraries/example/fixture/version-library-...jar" \
    -C "$EMPTY_LIBRARY" .

  REPLACEMENT_STAGE="$FIX/stage-Replacement"
  mkdir -p "$REPLACEMENT_STAGE/example"
  cp "$LIFECYCLE_CLASSES/example/ReplacementPlugin.class" \
    "$REPLACEMENT_STAGE/example/"
  cp "$REPO/plugin-api/fixture/lifecycle/Replacement/plugin.yml" \
    "$REPLACEMENT_STAGE/"
  jar --create --file "$REPLACEMENT_PLUGINS/Replacement.jar" \
    -C "$REPLACEMENT_STAGE" .

  SELF_DISABLE_PLUGINS="$FIX/self-disable-plugins"
  EVENT_DISABLE_PLUGINS="$FIX/event-disable-plugins"
  JDBC_PLUGINS="$FIX/jdbc-plugins"
  mkdir -p "$SELF_DISABLE_PLUGINS" "$EVENT_DISABLE_PLUGINS" "$JDBC_PLUGINS"
  for fixture in EnableObserver SelfDisabling EventDisabled EventDisabledDependent; do
    STAGE="$FIX/stage-host-runtime-$fixture"
    mkdir -p "$STAGE/example"
    cp "$HOST_RUNTIME_CLASSES/example/$fixture"Plugin.class "$STAGE/example/"
    cp "$REPO/plugin-api/fixture/host-runtime/$fixture/plugin.yml" "$STAGE/"
    case "$fixture" in
      EnableObserver)
        jar --create --file "$SELF_DISABLE_PLUGINS/$fixture.jar" -C "$STAGE" .
        jar --create --file "$EVENT_DISABLE_PLUGINS/$fixture.jar" -C "$STAGE" .
        ;;
      SelfDisabling)
        jar --create --file "$SELF_DISABLE_PLUGINS/$fixture.jar" -C "$STAGE" .
        ;;
      *)
        jar --create --file "$EVENT_DISABLE_PLUGINS/$fixture.jar" -C "$STAGE" .
        ;;
    esac
  done
  JDBC_STAGE="$FIX/stage-host-runtime-JdbcFixture"
  mkdir -p "$JDBC_STAGE/example"
  cp "$HOST_RUNTIME_CLASSES/example/JdbcPlugin.class" \
    "$HOST_RUNTIME_CLASSES/example/PrivateDriver.class" "$JDBC_STAGE/example/"
  cp "$REPO/plugin-api/fixture/host-runtime/JdbcFixture/plugin.yml" "$JDBC_STAGE/"
  cp -R "$REPO/plugin-api/fixture/host-runtime/plugin-services/META-INF" "$JDBC_STAGE/"
  jar --create --file "$JDBC_PLUGINS/JdbcFixture.jar" -C "$JDBC_STAGE" .

  for fixture_set in graph duplicates existing cross-call; do
    SET_PLUGINS="$FIX/dependency-$fixture_set-plugins"
    mkdir -p "$SET_PLUGINS"
    for descriptor in "$REPO/plugin-api/fixture/dependencies/$fixture_set"/*/*.yml; do
      fixture="$(basename "$(dirname "$descriptor")")"
      STAGE="$FIX/stage-dependency-$fixture_set-$fixture"
      mkdir -p "$STAGE/fixture/dependencies"
      cp "$DEPENDENCY_CLASSES/fixture/dependencies/DependencyPlugin.class" \
        "$DEPENDENCY_CLASSES/fixture/dependencies/DependencyPlugins.class" \
        "$DEPENDENCY_CLASSES/fixture/dependencies/DependencyPlugins\$$fixture.class" \
        "$STAGE/fixture/dependencies/"
      if [ "$fixture" = "BootstrapProvider" ]; then
        cp "$DEPENDENCY_CLASSES/fixture/dependencies/BootstrapApi.class" \
          "$STAGE/fixture/dependencies/"
      fi
      if [ "$fixture" = "NoJoinProvider" ]; then
        cp "$DEPENDENCY_CLASSES/fixture/dependencies/NoJoinApi.class" \
          "$STAGE/fixture/dependencies/"
      fi
      if [ "$fixture" = "AfterProvider" ]; then
        cp "$DEPENDENCY_CLASSES/fixture/dependencies/AfterApi.class" \
          "$STAGE/fixture/dependencies/"
      fi
      if [ "$fixture" = "ZOmitProvider" ]; then
        cp "$DEPENDENCY_CLASSES/fixture/dependencies/OmitApi.class" \
          "$STAGE/fixture/dependencies/"
      fi
      if [ "$fixture" = "ExistingProvider" ]; then
        cp "$DEPENDENCY_CLASSES/fixture/dependencies/ExistingApi.class" \
          "$STAGE/fixture/dependencies/"
      fi
      cp "$descriptor" "$STAGE/"
      jar --create --file "$SET_PLUGINS/$fixture.jar" -C "$STAGE" .
    done
  done

  PAPER_PLUGINS="$FIX/paper-plugins"
  mkdir -p "$PAPER_PLUGINS"
  for descriptor in "$REPO/plugin-api/fixture/paper"/{valid,invalid}/*/paper-plugin.yml; do
    fixture="$(basename "$(dirname "$descriptor")")"
    STAGE="$FIX/stage-paper-$fixture"
    mkdir -p "$STAGE/example"
    cp "$PAPER_CLASSES"/example/*.class "$STAGE/example/"
    cp "$descriptor" "$STAGE/"
    jar --create --file "$PAPER_PLUGINS/$fixture.jar" -C "$STAGE" .
  done

  # Only the original event fixture is parent-visible because older checks
  # inspect its counters directly. Lifecycle classes exist solely in their
  # plugin jars, so the PluginClassLoader owns their classes and lambdas.
  javac -nowarn -d "$CHECK_CLASSES" \
    -cp "$API_CP:$EVENT_CLASSES:$CHECK_CLASSES:$HOST_DRIVER_JAR" \
    "$REPO"/plugin-api/check/*.java
  java -cp "$CHECK_CLASSES:$API_CP" SpawnReasonCheck
  java -cp "$CHECK_CLASSES:$API_CP" SpawnReasonCheck fallback
  java -cp "$CHECK_CLASSES:$API_CP:$EVENT_CLASSES" Checks \
    "$FIX/plugins" "$LIFECYCLE_PLUGINS" "$REPLACEMENT_PLUGINS" \
    "$FIX/dependency-graph-plugins" "$FIX/dependency-duplicates-plugins" \
    "$FIX/dependency-existing-plugins" "$FIX/dependency-cross-call-plugins" \
    "$LIBRARY_PLUGINS" "$MALFORMED_LIBRARY_PLUGINS" \
    "$DESCRIPTOR_CACHE_JAR" "$INTERRUPTED_CACHE_JAR" "$PAPER_PLUGINS" \
    "$OUT/fixture-evidence.json"
  java -cp "$CHECK_CLASSES:$API_CP:$HOST_DRIVER_JAR" HostRuntimeCheck \
    jdbc "$JDBC_PLUGINS"
  java -cp "$CHECK_CLASSES:$API_CP:$HOST_DRIVER_JAR" HostRuntimeCheck \
    self-disable "$SELF_DISABLE_PLUGINS"
  java -cp "$CHECK_CLASSES:$API_CP:$HOST_DRIVER_JAR" HostRuntimeCheck \
    event-disable "$EVENT_DISABLE_PLUGINS"

  mkdir -p "$REPO/build"
  python3 "$REPO/dev/plugin_api_usage.py" \
    --covered "$JAR" --events "$SRC" \
    --summary-json "$REPO/build/plugin-api-evidence.json" >/dev/null
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
