#!/usr/bin/env bash
# Compile against a checksum-pinned official Paper 1.21.11 API, then load in Foton's JVM.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PAPER_API_SHA256=c577b181c11a8674310e56c92a91e31c010b7f04c9bd10b91c3be18374401070
PAPER_API_URL=https://repo.papermc.io/repository/maven-public/io/papermc/paper/paper-api/1.21.11-R0.1-SNAPSHOT/paper-api-1.21.11-R0.1-20260511.115010-91.jar
command -v javac >/dev/null || { echo 'javac is required' >&2; exit 1; }
command -v jar >/dev/null || { echo 'jar is required' >&2; exit 1; }
command -v sha256sum >/dev/null || { echo 'sha256sum is required' >&2; exit 1; }
[ -f "$REPO/plugin-api/build/foton-plugin-api.jar" ] || {
  echo 'run bash dev/build-plugin-api.sh first' >&2
  exit 2
}
bash "$REPO/dev/fetch-plugin-api-libs.sh" --check

scratch="$(mktemp -d "${TMPDIR:-/tmp}/foton-sqlite-plugin.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT
PAPER_API_JAR="${1:-$scratch/paper-api-1.21.11-91.jar}"
if [ "${1:-}" = '' ]; then
  command -v curl >/dev/null || { echo 'curl is required to fetch Paper API' >&2; exit 1; }
  curl -fsSL --retry 3 --max-time 180 -o "$PAPER_API_JAR" "$PAPER_API_URL"
fi
[ -f "$PAPER_API_JAR" ] || { echo 'Paper API JAR is missing' >&2; exit 2; }
actual_paper_sha256="$(sha256sum "$PAPER_API_JAR" | cut -d' ' -f1)"
if [ "$actual_paper_sha256" != "$PAPER_API_SHA256" ]; then
  echo "not the pinned official Paper 1.21.11 API: $PAPER_API_JAR" >&2
  echo "expected SHA-256 $PAPER_API_SHA256, got $actual_paper_sha256" >&2
  exit 2
fi

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
