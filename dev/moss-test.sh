#!/bin/bash
# Bone-meal a moss block and check a moss patch grew around it.
#
# `BonemealableFeaturePlacerBlock` does not grow the block it is used on: it runs
# a whole configured feature above it. That means the worldgen feature dispatcher
# has to accept a live world, which no unit test of the block alone can prove --
# the path runs from a right-click through the bone meal item into placement.
#
# The patch's radius is sampled from 1..2 and then has one added, so every column
# within one block of the origin is interior rather than edge, and is always
# turned into moss. That is what the markers check.
#
# Usage: bash dev/moss-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

# Respect $CARGO_TARGET_DIR the way `cargo build` itself already does; see
# the same lines in dev/join-test.sh for what hardcoding it used to cost.
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
BIN="$TARGET_DIR/debug/foton"

PORT=25620
RUN_DIR="$ROOT/run-moss"

echo "=== Building ==="
if ! cargo build 2>&1 | tail -2; then
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

# A stone floor well above the terrain with open air over it: the patch replaces
# ground that is `#minecraft:moss_replaceable`, and stone is, through
# `#minecraft:base_stone_overworld`.
CMDS='gamemode creative'
CMDS="$CMDS;;teleport @s 0 103 0"
for x in -3 -2 -1 0 1 2 3; do
  for z in -3 -2 -1 0 1 2 3; do
    CMDS="$CMDS;;setblock $x 100 $z minecraft:stone"
    CMDS="$CMDS;;setblock $x 101 $z minecraft:air"
  done
done
CMDS="$CMDS;;setblock 0 100 0 minecraft:moss_block"
# Nothing is moss yet except the block itself.
CMDS="$CMDS;;execute if block 1 100 0 minecraft:stone run tellraw @s \"MOSSFLOORISSTONE\""

CMDS="$CMDS;;give @s minecraft:bone_meal 8"
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;!useon 0 100 0 up"

CMDS="$CMDS;;execute if block 1 100 0 minecraft:moss_block run tellraw @s \"MOSSPATCHEAST\""
CMDS="$CMDS;;execute if block -1 100 0 minecraft:moss_block run tellraw @s \"MOSSPATCHWEST\""
CMDS="$CMDS;;execute if block 0 100 1 minecraft:moss_block run tellraw @s \"MOSSPATCHSOUTH\""
CMDS="$CMDS;;execute if block 0 100 -1 minecraft:moss_block run tellraw @s \"MOSSPATCHNORTH\""

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=2 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what the server answered ==="
grep "server says" join.log | grep -E "MOSS" | tail -8
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic|incorrect|unknown" | tail -6

if [ $STATUS -ne 0 ]; then
  echo "########## MOSS TEST FAILED (the client never settled) ##########"
  tail -20 join.log
  exit 1
fi
# Only the server's own reply counts: join.py echoes the commands it sends, and
# the command text carries the marker too.
for marker in MOSSFLOORISSTONE MOSSPATCHEAST MOSSPATCHWEST MOSSPATCHSOUTH MOSSPATCHNORTH; do
  if ! grep "server says" join.log | grep -q "$marker"; then
    echo "########## MOSS TEST FAILED ($marker missing) ##########"
    exit 1
  fi
done
echo "########## MOSS TEST PASSED ##########"
