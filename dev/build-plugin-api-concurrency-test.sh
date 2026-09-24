#!/usr/bin/env bash
# Regression: simultaneous plugin API builds must never share class output or
# publish a partially written jar.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUILD_SCRIPT="$REPO/dev/build-plugin-api.sh"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/foton-plugin-api-concurrency.XXXXXX")"
cleanup() {
  rm -rf -- "$WORK"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

bash "$BUILD_SCRIPT" --check >"$WORK/first.log" 2>&1 &
FIRST=$!
bash "$BUILD_SCRIPT" --check >"$WORK/second.log" 2>&1 &
SECOND=$!

FIRST_STATUS=0
SECOND_STATUS=0
wait "$FIRST" || FIRST_STATUS=$?
wait "$SECOND" || SECOND_STATUS=$?

if [ "$FIRST_STATUS" -ne 0 ] || [ "$SECOND_STATUS" -ne 0 ]; then
  echo "concurrent plugin API build failed: first=$FIRST_STATUS second=$SECOND_STATUS" >&2
  echo "--- first build ---" >&2
  cat "$WORK/first.log" >&2
  echo "--- second build ---" >&2
  cat "$WORK/second.log" >&2
  exit 1
fi

for output in "$WORK/first.log" "$WORK/second.log"; do
  grep -q '^wrote plugin-api/build/foton-plugin-api.jar ' "$output" || {
    echo "successful build did not report its published jar: $output" >&2
    cat "$output" >&2
    exit 1
  }
  REPORTED_CLASSES="$(sed -n 's/^wrote .* (.*, \([0-9][0-9]*\) classes)$/\1/p' "$output")"
  if [ -z "$REPORTED_CLASSES" ] || [ "$REPORTED_CLASSES" -lt 900 ]; then
    echo "build reported an incomplete private class output: $output" >&2
    cat "$output" >&2
    exit 1
  fi
done

JAR="$REPO/plugin-api/build/foton-plugin-api.jar"
CONTENTS="$WORK/jar-contents.txt"
jar --list --file "$JAR" >"$CONTENTS"
grep -qx 'foton/PluginHost.class' "$CONTENTS"
grep -qx 'foton/network/FotonViaChannel.class' "$CONTENTS"
grep -qx 'org/bukkit/Material.class' "$CONTENTS"

CLASS_COUNT="$(grep -c '\.class$' "$CONTENTS")"
if [ "$CLASS_COUNT" -lt 900 ]; then
  echo "concurrent build published an incomplete jar ($CLASS_COUNT classes)" >&2
  exit 1
fi

if find "$REPO/plugin-api/build" -maxdepth 1 -name '.foton-plugin-api.jar.*' -print -quit \
    | grep -q .; then
  echo "concurrent build left an unpublished staging jar" >&2
  exit 1
fi

# The optional external-fixture boot used to replace the script's EXIT trap,
# cleaning its own work directory but leaking the much larger private build.
EXTERNAL="$WORK/external-fixture"
mkdir -p "$EXTERNAL/classes"
javac -nowarn -d "$EXTERNAL/classes" -cp "$JAR:$REPO/plugin-api/lib/*" \
  "$REPO"/plugin-api/fixture/src/example/*.java
cp "$REPO/plugin-api/fixture/src/plugin.yml" "$EXTERNAL/classes/"
cp "$REPO/plugin-api/fixture/src/config.yml" "$EXTERNAL/classes/"
jar --create --file "$EXTERNAL/EventFixture.jar" -C "$EXTERNAL/classes" .

FIXTURE_TMP="$WORK/fixture-tmp"
mkdir -p "$FIXTURE_TMP"
TMPDIR="$FIXTURE_TMP" FOTON_PLUGIN_FIXTURE="$EXTERNAL/EventFixture.jar" \
  bash "$BUILD_SCRIPT" --check >"$WORK/external-fixture.log" 2>&1 || {
    cat "$WORK/external-fixture.log" >&2
    exit 1
  }
if find "$FIXTURE_TMP" -mindepth 1 -print -quit | grep -q .; then
  echo "external fixture build leaked a private work directory" >&2
  find "$FIXTURE_TMP" -mindepth 1 -maxdepth 1 -print >&2
  exit 1
fi

echo "concurrent plugin API builds passed; final jar contains $CLASS_COUNT classes; external fixture cleanup passed"
