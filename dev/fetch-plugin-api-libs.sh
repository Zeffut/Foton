#!/usr/bin/env bash
# Checks -- and can restore -- the jars the Bukkit API compiles against.
#
# The jars are committed; see plugin-api/lib/README.md for why, and for their
# licenses. This script is what makes that vendoring provable: it holds the
# SHA-256 of every jar the build is allowed to see, so an edited, swapped or
# added jar is a build failure rather than a surprise in the bytecode.
#
# The set and the versions are not a matter of taste. Compile-time libraries
# come from `io.papermc.paper:paper-api:26.2.build.121-stable` and its Adventure
# BOM. Host runtime libraries, currently Xerial SQLite JDBC, match Paper's
# server runtime. The directory once held Adventure 4.26.1 beside a 5.2.0
# logger built against Adventure 5, and it compiled -- which is the whole
# argument for checking rather than trusting a build.
#
#     bash dev/fetch-plugin-api-libs.sh --check  # verify only; what the build runs
#     bash dev/fetch-plugin-api-libs.sh          # download a missing or changed jar
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LIB="$REPO/plugin-api/lib"
MANIFEST="$LIB/manifest.txt"
MIRROR="${FOTON_MAVEN_MIRROR:-https://repo.papermc.io/repository/maven-public}"
CHECK_ONLY=0
if [ "${1:-}" = "--check" ]; then CHECK_ONLY=1; fi

if [ ! -f "$MANIFEST" ]; then
  echo "missing pinned library manifest: $MANIFEST" >&2
  exit 1
fi

if [ "$CHECK_ONLY" -eq 0 ] && ! command -v curl >/dev/null 2>&1; then
  echo "curl is missing; cannot fetch the plugin API libraries" >&2
  exit 1
fi

digest() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1
  else shasum -a 256 "$1" | cut -d' ' -f1  # macOS ships shasum, not sha256sum
  fi
}

pinned_names() {
  awk 'NF && $1 !~ /^#/ {print $2 "-" $3 ".jar"}' "$MANIFEST"
}

if [ "$CHECK_ONLY" -eq 0 ]; then mkdir -p "$LIB"; fi
missing=0
fetched=0
part=""
cleanup() {
  [ -z "$part" ] || rm -f "$part"
}
trap cleanup EXIT
trap 'exit 130' HUP INT TERM

while read -r path artifact version want; do
  [ -n "$path" ] || continue
  case "$path" in \#*) continue ;; esac
  jar="$LIB/$artifact-$version.jar"

  if [ -f "$jar" ] && [ ! -L "$jar" ] && [ "$(digest "$jar")" = "$want" ]; then
    continue
  fi

  if [ "$CHECK_ONLY" -eq 1 ]; then
    echo "missing or stale: $artifact-$version.jar" >&2
    missing=$((missing + 1))
    continue
  fi

  if [ -e "$jar" ] || [ -L "$jar" ]; then
    echo "$artifact-$version.jar does not match its pinned digest; refetching" >&2
    rm -f "$jar"
  fi

  if [ "$path" = "io/netty" ]; then
    # These exact modules and versions are Minecraft 26.2 runtime inputs, and
    # libraries.minecraft.net is the authoritative URL recorded by its
    # generated dependencies.json.
    url="https://libraries.minecraft.net/$path/$artifact/$version/$artifact-$version.jar"
  else
    url="$MIRROR/$path/$artifact/$version/$artifact-$version.jar"
  fi
  part=$(mktemp "$LIB/.${artifact}-${version}.part.XXXXXX") \
    || { echo "could not create a temporary download for $artifact-$version.jar" >&2; missing=$((missing + 1)); continue; }
  if ! curl -fsSL --retry 3 --max-time 180 -o "$part" "$url"; then
    echo "could not download $url" >&2
    rm -f "$part"
    missing=$((missing + 1))
    continue
  fi

  got="$(digest "$part")"
  if [ "$got" != "$want" ]; then
    # Refuse the file rather than compile against it. A mirror serving
    # something else is a supply-chain problem, not a network hiccup.
    echo "digest mismatch for $artifact-$version.jar" >&2
    echo "  expected $want" >&2
    echo "  got      $got" >&2
    rm -f "$part"
    missing=$((missing + 1))
    continue
  fi

  mv -f "$part" "$jar"
  part=""
  fetched=$((fetched + 1))
done < "$MANIFEST"

# Anything else in the directory is not on the pinned list, and javac would
# happily compile against it. Name it rather than let it drift in silently.
for jar in "$LIB"/*.jar; do
  [ -e "$jar" ] || [ -L "$jar" ] || continue
  name="$(basename "$jar")"
  if ! pinned_names | grep -qxF "$name"; then
    echo "unpinned jar in plugin-api/lib: $name" >&2
    missing=$((missing + 1))
  fi
done

# The compiler only supports the direct pinned set. A nested jar used to evade
# the check while still entering build-plugin-api.sh's recursive classpath.
if find "$LIB" -mindepth 2 \( -type f -o -type l \) -name '*.jar' -print -quit | grep -q .; then
  echo "nested jar in plugin-api/lib is not part of the pinned set" >&2
  missing=$((missing + 1))
fi

if [ "$missing" -gt 0 ]; then
  echo "plugin-api/lib is not the pinned set ($missing problem(s))" >&2
  exit 1
fi

if [ "$fetched" -gt 0 ]; then echo "fetched $fetched jar(s) into plugin-api/lib"; fi
exit 0
