#!/usr/bin/env bash
# Compile against the supplied official Paper 1.21.11 API, then load in Foton's JVM.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PAPER_API_JAR="${1:-}"
[ -f "$PAPER_API_JAR" ] || {
  echo 'usage: bash dev/sqlite-plugin-test.sh /path/to/paper-api-1.21.11.jar' >&2
  exit 2
}
command -v javac >/dev/null || { echo 'javac is required' >&2; exit 1; }
command -v jar >/dev/null || { echo 'jar is required' >&2; exit 1; }
jar tf "$PAPER_API_JAR" | grep -qx 'org/bukkit/plugin/java/JavaPlugin.class' || {
  echo 'the supplied JAR is not a Paper/Bukkit API' >&2
  exit 2
}
[ -f "$REPO/plugin-api/build/foton-plugin-api.jar" ] || {
  echo 'run bash dev/build-plugin-api.sh first' >&2
  exit 2
}
bash "$REPO/dev/fetch-plugin-api-libs.sh" --check

if [ -z "${FOTON_JAVA_HOME:-}" ]; then
  java_binary="$(readlink -f "$(command -v java)")"
  FOTON_JAVA_HOME="$(dirname "$(dirname "$java_binary")")"
fi
export FOTON_JAVA_HOME

scratch="$(mktemp -d "${TMPDIR:-/tmp}/foton-sqlite-plugin.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT
mkdir -p "$scratch/classes" "$scratch/plugins" "$scratch/plugin-runtime/lib" \
  "$scratch/plugin-runtime/licenses"
javac --release 21 -cp "$PAPER_API_JAR:$REPO/plugin-api/lib/*" -d "$scratch/classes" \
  "$REPO/dev/fixtures/sqlite-plugin/SqlitePlugin.java"
cp "$REPO/dev/fixtures/sqlite-plugin/plugin.yml" "$scratch/classes/"
jar --create --file "$scratch/plugins/FotonSqliteProbe.jar" -C "$scratch/classes" .
cp "$REPO/plugin-api/build/foton-plugin-api.jar" "$scratch/plugin-runtime/"
cp "$REPO"/plugin-api/lib/*.jar "$scratch/plugin-runtime/lib/"
cp "$REPO"/plugin-api/lib/licenses/*.txt "$scratch/plugin-runtime/licenses/"
(cd "$scratch/plugin-runtime" && sha256sum foton-plugin-api.jar lib/*.jar licenses/*.txt > SHA256SUMS \
  && sha256sum --check SHA256SUMS >/dev/null)

# This example embeds the same JVM and plugin host used by the server, pointed
# at a separately staged release-layout runtime rather than source-tree JARs.
# The marker is emitted only after SQL executed and the result was checked.
FOTON_PLUGIN_API_JAR="$scratch/plugin-runtime/foton-plugin-api.jar" \
FOTON_PLUGIN_LIBRARY_DIRECTORY="$scratch/plugin-runtime/lib" \
  cargo run --quiet -p foton-plugin --example load_plugins -- "$scratch/plugins" \
  > "$scratch/host.log" 2>&1 || {
    cat "$scratch/host.log" >&2
    exit 1
  }
if ! grep -qF '[sqlite-plugin] DriverManager SQL roundtrip passed' "$scratch/host.log" ||
   ! grep -qF -- '--- 1 plugin(s) enabled from Rust ---' "$scratch/host.log"; then
  cat "$scratch/host.log" >&2
  echo 'SQLite probe did not enable successfully' >&2
  exit 1
fi
printf 'Paper-compiled SQLite plugin loaded and SQL roundtrip passed\n'
