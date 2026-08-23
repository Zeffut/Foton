#!/bin/bash
# Put a boat over water and check it is still there afterwards.
#
# A boat that does not float sinks through the sea bed and keeps going, and a
# unit test on a synthetic pool cannot see the difference between that and a
# boat the server never ticked. This summons one into real water, waits, and
# asks the world whether an entity is still standing at the surface.
#
# Usage: bash dev/boat-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25576
RUN_DIR="$ROOT/run-boat"

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
nohup "$ROOT/target/debug/steel" > server.log 2>&1 < /dev/null &
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

# A stone bowl filled with water, a boat dropped a little above it.
CMDS='teleport @s 0 90 0'
for x in -3 -2 -1 0 1 2 3; do
  for z in -3 -2 -1 0 1 2 3; do
    CMDS="$CMDS;;setblock $x 79 $z minecraft:stone"
    CMDS="$CMDS;;setblock $x 80 $z minecraft:water"
  done
done
CMDS="$CMDS;;summon minecraft:oak_boat 0 82 0"
CMDS="$CMDS;;summon minecraft:bamboo_raft 2 82 0"
# Give the physics time to settle, then look. A boat that floats is still
# within a block of the surface; one that sank is far below or gone.
for _ in $(seq 1 8); do
  CMDS="$CMDS;;execute positioned 0 81 0 if entity @e[type=minecraft:oak_boat,distance=..3] run tellraw @s \"BOATSTILLAFLOAT\""
done
CMDS="$CMDS;;execute positioned 2 81 0 if entity @e[type=minecraft:bamboo_raft,distance=..3] run tellraw @s \"RAFTSTILLAFLOAT\""

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=3 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what the server answered ==="
grep "server says" join.log | grep -vE "setblock|teleport" | tail -8
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic|incorrect|unknown" | tail -8

if [ $STATUS -ne 0 ]; then
  echo "########## BOAT TEST FAILED (the client never settled) ##########"
  tail -20 join.log
  exit 1
fi
for marker in BOATSTILLAFLOAT RAFTSTILLAFLOAT; do
  # Only the server's own reply counts: join.py echoes the commands it sends.
  if ! grep "server says" join.log | grep -q "$marker"; then
    echo "########## BOAT TEST FAILED ($marker missing) ##########"
    exit 1
  fi
done
echo "########## BOAT TEST PASSED ##########"
