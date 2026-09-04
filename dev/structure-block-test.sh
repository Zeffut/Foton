#!/bin/bash
# Pulse a load-mode structure block and check the structure it names is standing.
#
# Loading runs `StructureTemplate::place_in_world` against the live world, which
# only became possible once template placement stopped requiring a mid-generation
# `WorldGenRegion`. The whole path -- a redstone edge, the block entity, the
# template loader, placement -- only exists on a running server.
#
# `nether_fossils/fossil_5` is two by five by one, four bone blocks standing on
# the Y axis with one laid on the X axis across the top, and no processors, so
# exactly which positions it fills is fixed.
#
# Usage: bash dev/structure-block-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

# Respect $CARGO_TARGET_DIR the way `cargo build` itself already does; see
# the same lines in dev/join-test.sh for what hardcoding it used to cost.
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
BIN="$TARGET_DIR/debug/foton"

PORT=25621
RUN_DIR="$ROOT/run-structure-block"

echo "=== Building ==="
cargo build 2>&1 | tail -2
# A pipeline's status is its last command's, so `if ! cargo build | tail`
# tested `tail` and never failed. That made the branch below unreachable: a
# broken build fell straight through and the test ran against a stale binary.
if [ "${PIPESTATUS[0]}" -ne 0 ]; then
  echo "BUILD FAILED"
  exit 1
fi

rm -rf "$RUN_DIR"
mkdir -p "$RUN_DIR" || exit 1
if [ ! -d "$ROOT/run-offline/config" ]; then
  echo "RUN dev/join-test.sh FIRST so a config exists"
  exit 1
fi
cp -r "$ROOT/run-offline/config" "$RUN_DIR/config"
sed -i "s/^server_port = .*/server_port = $PORT/" "$RUN_DIR/config/config.toml"
sed -i 's/^default_groups = .*/default_groups = ["op"]/' "$RUN_DIR/config/groups.toml"

cd "$RUN_DIR" || exit 1
nohup "$BIN" > server.log 2>&1 < /dev/null &
PID=$!
cleanup() {
  kill "$PID" 2>/dev/null
  for _ in $(seq 1 30); do kill -0 "$PID" 2>/dev/null || break; sleep 1; done
  kill -9 "$PID" 2>/dev/null
}

for _ in $(seq 1 180); do
  ss -ltn 2>/dev/null | grep -q ":$PORT" && break
  sleep 1
done
if ! ss -ltn 2>/dev/null | grep -q ":$PORT"; then
  echo "SERVER NEVER LISTENED ON $PORT"
  sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | tail -20
  cleanup; exit 1
fi

# `mode` is written as vanilla's ordinal, and 1 is LOAD.
STRUCTURE_NBT='{name:"minecraft:nether_fossils/fossil_5",mode:1,posX:0,posY:1,posZ:0,sizeX:2,sizeY:5,sizeZ:1,rotation:0,mirror:0,integrity:1.0f,seed:0L,ignoreEntities:1b,powered:0b}'

CMDS='gamemode creative'
CMDS="$CMDS;;teleport @s 4 101 4"
# Clear the box the structure will land in, so a bone block afterwards can only
# have come from the placement.
for y in 101 102 103 104 105; do
  CMDS="$CMDS;;setblock 0 $y 0 minecraft:air"
  CMDS="$CMDS;;setblock 1 $y 0 minecraft:air"
done
CMDS="$CMDS;;setblock 0 100 0 minecraft:structure_block[mode=load]$STRUCTURE_NBT"
CMDS="$CMDS;;execute if block 0 101 0 minecraft:air run tellraw @s \"STRUCTUREBOXCLEAR\""
# The rising edge is what triggers a load.
CMDS="$CMDS;;setblock 1 100 0 minecraft:redstone_block"
CMDS="$CMDS;;execute if block 0 101 0 minecraft:bone_block[axis=y] run tellraw @s \"STRUCTUREBONEBOTTOM\""
CMDS="$CMDS;;execute if block 0 104 0 minecraft:bone_block[axis=y] run tellraw @s \"STRUCTUREBONETOP\""
CMDS="$CMDS;;execute if block 1 105 0 minecraft:bone_block[axis=x] run tellraw @s \"STRUCTUREBONECROSS\""

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=2 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what the server answered ==="
grep "server says" join.log | grep -E "STRUCTURE" | tail -8
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic|incorrect|unknown" | tail -6

if [ $STATUS -ne 0 ]; then
  echo "########## STRUCTURE BLOCK TEST FAILED (the client never settled) ##########"
  tail -20 join.log
  exit 1
fi
# Only the server's own reply counts: join.py echoes the commands it sends, and
# the command text carries the marker too.
for marker in STRUCTUREBOXCLEAR STRUCTUREBONEBOTTOM STRUCTUREBONETOP STRUCTUREBONECROSS; do
  if ! grep "server says" join.log | grep -q "$marker"; then
    echo "########## STRUCTURE BLOCK TEST FAILED ($marker missing) ##########"
    exit 1
  fi
done
echo "########## STRUCTURE BLOCK TEST PASSED ##########"
