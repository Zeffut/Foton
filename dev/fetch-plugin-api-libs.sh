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
com/google/guava failureaccess 1.0.3 cbfc3906b19b8f55dd7cfd6dfe0aa4532e834250d7f080bd8d211a3e246b59cb
org/jspecify jspecify 1.0.0 1fad6e6be7557781e4d33729d49ae1cdc8fdda6fe477bb0cc68ce351eafdfbab
com/google/errorprone error_prone_annotations 2.47.0 5364bc6f22e72e98195e406a58d3ba1c09ffa11dea0729592cb870dc2de4056d
com/google/j2objc j2objc-annotations 3.1 84d3a150518485f8140ea99b8a985656749629f6433c92b80c75b36aba3b099b
org/joml joml 1.10.8 bf19510145178df82cd3bd37edd514c13f411531ec5545299fd3abcbc98fe7c2
org/jetbrains/kotlin kotlin-stdlib-jdk8 1.8.20 e398b67977622718bf18ff99b739c7d9da060f33fb458a2e25203221c16af010
org/jetbrains/kotlin kotlin-stdlib-jdk7 1.8.20 af1ec40c3b951afdcc0c2a0173c7b81763c5281c2d5bafbf0a8544a24c5dcc0c
org/jetbrains/kotlin kotlin-stdlib 1.8.20 4395647b1961d9fb730a34e8dbe56c293157bc0759004cca63d9b5ee6653e5c7
org/jetbrains/kotlin kotlin-stdlib-common 1.8.20 fa20188abaa8ecf1d0035e93a969b071f10e45a1c8378c314521eade73f75fd5
org/slf4j slf4j-api 2.0.17 7b751d952061954d5abfed7181c1f645d336091b679891591d63329c622eb832
org/yaml snakeyaml 2.2 1467931448a0817696ae2805b7b8b20bfb082652bf9c4efaed528930dc49389b
io/netty netty-common 4.2.15.Final 78206aa7f6d197caa926291408c01889b6b910ca0f74017d3fcbdaccf9562959
io/netty netty-buffer 4.2.15.Final 1361fd9c9ba85b9831cf54a1b2e45ddc3ce34a768931726c099d3f5ef0efe4a3
io/netty netty-transport 4.2.15.Final 9fb671e96651066cf1a28dad3f1382c7c99df5b8326d4a214d9a375b311f8dc1
io/netty netty-resolver 4.2.15.Final 24318497f2a3a645964fed418f48879ba11365cab8e1f8d66c47fa7da15ef19a
io/netty netty-codec-base 4.2.15.Final 2c6d39d7270628b8cfc3166fbd7a93d595958e59c461bc32ddb383c8c91bf811
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
part=""
cleanup() {
  [ -z "$part" ] || rm -f "$part"
}
trap cleanup EXIT
trap 'exit 130' HUP INT TERM

while read -r path artifact version want; do
  [ -n "$path" ] || continue
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
