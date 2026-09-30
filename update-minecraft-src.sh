#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MINECRAFT_SRC_DIR="$SCRIPT_DIR/minecraft-src"
GITCRAFT_REPOSITORY="https://github.com/WinPlay02/GitCraft"
GITCRAFT_REVISION="61c79f013547b4782096c7a15c183b3f79548e60"

MINECRAFT_VERSION="$(python3 - "$SCRIPT_DIR/Cargo.toml" <<'PY'
import re
import sys

text = open(sys.argv[1], encoding="utf-8").read()
section = re.search(r"(?ms)^\[workspace\.package\]\s*$\n(.*?)(?=^\[|\Z)", text)
if section is None:
    raise SystemExit("Cargo.toml has no [workspace.package] section")
version_match = re.search(r'^version\s*=\s*"([^"]+)"\s*$', section.group(1), re.MULTILINE)
if version_match is None:
    raise SystemExit("[workspace.package] has no version")
version = version_match.group(1)
match = re.search(r"\+mc(.+)$", version)
if match is None:
    raise SystemExit(f"workspace version {version!r} has no +mc target suffix")
print(match.group(1))
PY
)"

GITCRAFT_ARGS=(
  "--override-repo-target=$MINECRAFT_SRC_DIR"
  "--only-version=$MINECRAFT_VERSION"
  "--only-unobfuscated"
  "--mappings=identity_unmapped"
  "--only-stable"
)
GITCRAFT_ARGS_SERIALIZED="$(python3 - "${GITCRAFT_ARGS[@]}" <<'PY'
import sys

serialized = []
for argument in sys.argv[1:]:
    if '"' not in argument:
        serialized.append(f'"{argument}"')
    elif "'" not in argument:
        serialized.append(f"'{argument}'")
    else:
        raise SystemExit("Gradle --args cannot represent an argument containing both quote types")
print(" ".join(serialized))
PY
)"

if [ "${1:-}" = "--dry-run" ]; then
  echo "temporary directory: /tmp/foton-gitcraft.XXXXXX"
  echo "git clone $GITCRAFT_REPOSITORY <temporary>/GitCraft"
  echo "git -C <temporary>/GitCraft checkout --detach $GITCRAFT_REVISION"
  printf './gradlew run --args=%s\n' "$GITCRAFT_ARGS_SERIALIZED"
  exit 0
fi
if [ "$#" -ne 0 ]; then
  echo "usage: $0 [--dry-run]" >&2
  exit 2
fi

TEMP_DIR="$(mktemp -d /tmp/foton-gitcraft.XXXXXX)"
cleanup() {
  rm -rf "$TEMP_DIR"
}
trap cleanup EXIT
echo "Cloning GitCraft into $TEMP_DIR..."

git clone "$GITCRAFT_REPOSITORY" "$TEMP_DIR/GitCraft"
git -C "$TEMP_DIR/GitCraft" checkout --detach "$GITCRAFT_REVISION"

# Increase heap from default 4G to 8G
sed -i.bak "s/-Xmx4G/-Xmx8G/" "$TEMP_DIR/GitCraft/build.gradle" && rm -f "$TEMP_DIR/GitCraft/build.gradle.bak"

# Run GitCraft
cd "$TEMP_DIR/GitCraft"
echo "Running GitCraft..."
./gradlew run --args="$GITCRAFT_ARGS_SERIALIZED"

echo "Done! minecraft-src has been updated."
