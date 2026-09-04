#!/bin/bash
# Use a spawn egg the way a player does, and check the mob arrives.
#
# A spawn egg works through `use_on`, which no command can reach: `/summon`
# bypasses the item entirely. The only honest check is a real right-click, so
# this gives the egg, selects the hotbar slot, clicks a block, and watches the
# entity stream for what turns up.
#
# Usage: bash dev/spawnegg-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

# Respect $CARGO_TARGET_DIR the way `cargo build` itself already does; see
# the same lines in dev/join-test.sh for what hardcoding it used to cost.
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
BIN="$TARGET_DIR/debug/foton"

PORT=25575
RUN_DIR="$ROOT/run-spawnegg"

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

# A slab of stone to stand the eggs on, then one click each. Creative so the
# egg is not consumed between clicks, and so the give always lands.
CMDS='gamemode creative'
CMDS="$CMDS;;teleport @s 0 100 0"
for x in -1 0 1 2; do
  CMDS="$CMDS;;setblock $x 99 0 minecraft:stone"
done
CMDS="$CMDS;;give @s minecraft:strider_spawn_egg 1"
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;!useon 0 99 0 up"
CMDS="$CMDS;;give @s minecraft:magma_cube_spawn_egg 1"
CMDS="$CMDS;;!hotbar 1"
CMDS="$CMDS;;!useon 2 99 0 up"

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=4 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== join.log ==="
grep -E "server says|before the commands|spawned|useon|hotbar|JOIN" join.log | tail -14
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic|unknown|incorrect" | tail -8

if [ $STATUS -ne 0 ]; then
  echo "########## SPAWN EGG TEST FAILED (the client never settled) ##########"
  tail -20 join.log
  exit 1
fi
for mob in strider magma_cube; do
  if ! grep -qE "(before the commands|spawned around the player):.*\b$mob x" join.log; then
    echo "########## SPAWN EGG TEST FAILED (no $mob appeared) ##########"
    exit 1
  fi
done
echo "########## SPAWN EGG TEST PASSED ##########"
