#!/usr/bin/env bash
# Compile against a checksum-pinned official Paper 1.21.11 API, then load in Foton's JVM.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$REPO/dev/paper-api-test-lib.sh"
VERIFY_PAPER_ONLY=0
if [ "${1:-}" = '--verify-paper-api' ]; then
  VERIFY_PAPER_ONLY=1
  shift
fi
command -v sha256sum >/dev/null || { echo 'sha256sum is required' >&2; exit 1; }

scratch="$(mktemp -d "${TMPDIR:-/tmp}/foton-sqlite-plugin.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT
PAPER_API_JAR="$(paper_api_prepare "${1:-${FOTON_PAPER_API_JAR:-}}" "$scratch")"
if [ "$VERIFY_PAPER_ONLY" -eq 1 ]; then
  printf 'pinned Paper 1.21.11 API verified\n'
  exit 0
fi
command -v javac >/dev/null || { echo 'javac is required' >&2; exit 1; }
command -v jar >/dev/null || { echo 'jar is required' >&2; exit 1; }
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
