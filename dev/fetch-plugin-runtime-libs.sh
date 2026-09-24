#!/usr/bin/env bash
# Fetches the run-time libraries a Paper server gives plugins without being
# asked, and that plugins therefore do not ship.
#
# plugin-api/lib is what Paper's *API* declares, and it is committed. This is
# what Paper's *server jar* adds on top, read from META-INF/libraries.list of
# paper-26.2-129.jar: a plugin opening `jdbc:sqlite:` or `jdbc:mysql:` finds
# the driver because the server carries it. Observer and Zelda Civ both do
# exactly that, and on Foton both failed with "No suitable driver" until these
# were here. The SLF4J binding is Foton's choice rather than Paper's -- Paper
# routes SLF4J to Log4j, Foton's plugin loggers are java.util.logging -- so
# that what a library logs through SLF4J is printed instead of dropped.
#
# Fetched, not committed: sixteen megabytes of drivers, most of it SQLite's
# native libraries for every platform, is a cost only a server that hosts
# plugins should pay. Pinned by digest all the same.
#
#     bash dev/fetch-plugin-runtime-libs.sh
#
# Output: plugin-api/build/runtime-libs, which the plugin host puts on the
# class path by default (see foton/src/lib.rs).
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$REPO/plugin-api/build/runtime-libs"
MIRROR="${FOTON_MAVEN_MIRROR:-https://repo.papermc.io/repository/maven-public}"

# path/in/maven artifact version sha256
PINNED=$(cat <<'LIST'
org/xerial sqlite-jdbc 3.49.1.0 5c8609d2ca341deb8c6f71778974b5ba4995c7d32d7c7c89d9392a3e72c39291
com/mysql mysql-connector-j 9.2.0 7e9941bbdcca244d878ea95bfff788fd9ba6a65af757f24be6c632930d61c7ed
com/google/protobuf protobuf-java 4.29.0 16901851ebe5e89fe88aaad3c26866373695bc2e30627bb8932847e2f5fc2e76
org/slf4j slf4j-jdk14 2.0.17 ead25c1b15f59db1fb5552b76fe63de4164f0df40024d19287b75dece47ad3be
LIST
)

digest() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1
  else shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

mkdir -p "$OUT"
while read -r group artifact version sha; do
  [ -n "$artifact" ] || continue
  jar="$OUT/$artifact-$version.jar"
  if [ -f "$jar" ] && [ "$(digest "$jar")" = "$sha" ]; then continue; fi
  url="$MIRROR/$group/$artifact/$version/$artifact-$version.jar"
  echo "fetching $artifact $version"
  fetched=0
  for delay in 0 2 4 8 16; do
    sleep "$delay"
    if curl -sSfL "$url" -o "$jar.part"; then fetched=1; break; fi
  done
  [ "$fetched" = 1 ] || { echo "could not fetch $url" >&2; exit 1; }
  mv "$jar.part" "$jar"
  actual="$(digest "$jar")"
  if [ "$actual" != "$sha" ]; then
    echo "$artifact-$version.jar has digest $actual, expected $sha" >&2
    rm -f "$jar"
    exit 1
  fi
done <<< "$PINNED"
echo "runtime libraries in ${OUT#"$REPO"/}"
