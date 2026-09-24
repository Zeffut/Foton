#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_DIR="$(mktemp -d)"
trap 'find "$RUN_DIR" -type f -delete; find "$RUN_DIR" -depth -type d -empty -delete' EXIT

worlds_file="$RUN_DIR/worlds.toml"
cp "$ROOT/package-content/worlds.toml" "$worlds_file"
original_file="$RUN_DIR/worlds-original.toml"
cp "$worlds_file" "$original_file"

python3 - "$worlds_file" <<'PY'
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
lines = path.read_text(encoding="utf-8").splitlines(keepends=True)
path.write_text(
    "".join(line for line in lines if not line.startswith("seed = ")),
    encoding="utf-8",
)
PY

if grep -q '^seed = ' "$worlds_file"; then
  echo "fixture unexpectedly contains a seed" >&2
  exit 1
fi

WORLD_SEED='seed "quoted" \ path'
source "$ROOT/dev/join-test.sh"
write_world_seed "$worlds_file"

expected='seed = "seed \"quoted\" \\ path"'
grep -Fqx "$expected" "$worlds_file"
test "$(grep -c '^seed = ' "$worlds_file")" -eq 1
cmp "$original_file" "$ROOT/package-content/worlds.toml"

echo "join seed insertion test passed"
