#!/usr/bin/env bash
# Checks -- and can restore -- the jars the Bukkit API compiles against.
#
# The jars are committed; see plugin-api/lib/README.md for why, and for their
# licenses. This script is what makes that vendoring provable: it holds the
# SHA-256 of every jar the build is allowed to see, so an edited, swapped or
# added jar is a build failure rather than a surprise in the bytecode.
#
# The set and the versions are not a matter of taste. They are what
# `io.papermc.paper:paper-api:26.2.build.121-stable` declares, read from its
# POM and from `net.kyori:adventure-bom:5.2.0`, so a plugin compiled against
# real Paper meets the same signatures here. The directory once held Adventure
# 4.26.1 beside a 5.2.0 logger built against Adventure 5, and it compiled --
# which is the whole argument for checking rather than trusting a build.
#
#     bash dev/fetch-plugin-api-libs.sh --check  # verify only; what the build runs
#     bash dev/fetch-plugin-api-libs.sh          # download a missing or changed jar
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LIB="$REPO/plugin-api/lib"
MIRROR="${FOTON_MAVEN_MIRROR:-https://repo.papermc.io/repository/maven-public}"
CHECK_ONLY=0
if [ "${1:-}" = "--check" ]; then CHECK_ONLY=1; fi

# path/in/maven artifact version sha256
PINNED=$(cat <<'LIST'
net/kyori adventure-api 5.2.0 7e52fe7190be3e87b3b3f71712cfa12315fcd27109f302a7b440c12f01fae827
net/kyori adventure-key 5.2.0 0184d173200e2eef8fbc791f622d1d58fd459f8930c616b5a4fe79e83eda6c55
net/kyori adventure-text-logger-slf4j 5.2.0 ae79b7f3846c5d973b37c7eec03190bd44e182918d1346e7705c5991a1b5dfb3
net/kyori adventure-text-serializer-plain 5.2.0 f6424cc038a631b79cc4b74b6b353d5d007c99b38f9be482e0c2448a00eecd21
org/jetbrains annotations 26.1.0 ebc7aec252ed0c7d2d04c039d7f00e69f7b86b1f493c741d67b3ef31b986b054
com/mojang brigadier 1.3.10 c8ee4136e474ac7723ca2b432ec8d1a2bc88ef7d1ec57c314ba9e33cdc83dd75
com/google/code/gson gson 2.14.0 2cbd119bf1961c28788310963dc80ba65f58cdeec1dd139c8bdb1240faa2c36f
com/google/guava guava 33.6.0-jre dc573e1fca4fd5454f4a5fd3d7da2df03002876a4175bafc14a95980dd7713b3
org/joml joml 1.10.8 bf19510145178df82cd3bd37edd514c13f411531ec5545299fd3abcbc98fe7c2
org/slf4j slf4j-api 2.0.17 7b751d952061954d5abfed7181c1f645d336091b679891591d63329c622eb832
org/yaml snakeyaml 2.2 1467931448a0817696ae2805b7b8b20bfb082652bf9c4efaed528930dc49389b
LIST
)

if ! command -v curl >/dev/null 2>&1; then
  echo "curl is missing; cannot fetch the plugin API libraries" >&2
  exit 1
fi

digest() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1
  else shasum -a 256 "$1" | cut -d' ' -f1  # macOS ships shasum, not sha256sum
  fi
}

pinned_names() {
  echo "$PINNED" | awk 'NF {print $2 "-" $3 ".jar"}'
}

mkdir -p "$LIB"
missing=0
fetched=0

while read -r path artifact version want; do
  [ -n "$path" ] || continue
  jar="$LIB/$artifact-$version.jar"

  if [ -f "$jar" ] && [ "$(digest "$jar")" = "$want" ]; then
    continue
  fi

  if [ -f "$jar" ]; then
    echo "$artifact-$version.jar does not match its pinned digest; refetching" >&2
    rm -f "$jar"
  fi

  if [ "$CHECK_ONLY" -eq 1 ]; then
    echo "missing or stale: $artifact-$version.jar" >&2
    missing=$((missing + 1))
    continue
  fi

  url="$MIRROR/$path/$artifact/$version/$artifact-$version.jar"
  if ! curl -fsSL --retry 3 --max-time 180 -o "$jar.part" "$url"; then
    echo "could not download $url" >&2
    rm -f "$jar.part"
    missing=$((missing + 1))
    continue
  fi

  got="$(digest "$jar.part")"
  if [ "$got" != "$want" ]; then
    # Refuse the file rather than compile against it. A mirror serving
    # something else is a supply-chain problem, not a network hiccup.
    echo "digest mismatch for $artifact-$version.jar" >&2
    echo "  expected $want" >&2
    echo "  got      $got" >&2
    rm -f "$jar.part"
    missing=$((missing + 1))
    continue
  fi

  mv "$jar.part" "$jar"
  fetched=$((fetched + 1))
done <<EOF
$PINNED
EOF

# Anything else in the directory is not on the pinned list, and javac would
# happily compile against it. Name it rather than let it drift in silently.
for jar in "$LIB"/*.jar; do
  [ -e "$jar" ] || continue
  name="$(basename "$jar")"
  if ! pinned_names | grep -qxF "$name"; then
    echo "unpinned jar in plugin-api/lib: $name" >&2
    missing=$((missing + 1))
  fi
done

if [ "$missing" -gt 0 ]; then
  echo "plugin-api/lib is not the pinned set ($missing problem(s))" >&2
  exit 1
fi

if [ "$fetched" -gt 0 ]; then echo "fetched $fetched jar(s) into plugin-api/lib"; fi
exit 0
