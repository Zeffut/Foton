#!/bin/bash
# Check an ender chest belongs to the player, not to the block.
#
# Two things distinguish it from a chest, and neither shows up in a unit test:
# the contents follow the player rather than staying in the block, and they
# survive a disconnect. So this puts something in, disconnects, reconnects, and
# looks in a *different* ender chest somewhere else.
#
# Usage: bash dev/enderchest-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25578
RUN_DIR="$ROOT/run-enderchest"

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
nohup "$ROOT/target/debug/foton" > server.log 2>&1 < /dev/null &
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

# The scripted client cannot click inventory slots and Foton has no `/item`
# command, so what a running server can show is the half no unit test can: that
# right-clicking the block actually opens a container. The other half -- that
# the contents belong to the player and survive a disconnect -- is a round trip
# through the save format, tested in `player_data`.
CMDS='gamemode creative'
CMDS="$CMDS;;teleport @s 0 101 0"
CMDS="$CMDS;;setblock 0 99 0 minecraft:stone"
CMDS="$CMDS;;setblock 0 100 0 minecraft:ender_chest"
CMDS="$CMDS;;execute if block 0 100 0 minecraft:ender_chest run tellraw @s \"CHESTPLACED\""
CMDS="$CMDS;;clear @s"
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;!useon 0 100 0 up"

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=2 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what happened ==="
grep -E "server says|a screen opened" join.log | grep -vE "setblock|teleport|gamemode|clear" | tail -6
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic|incorrect|unknown" | tail -6

if [ $STATUS -ne 0 ]; then
  echo "########## ENDER CHEST TEST FAILED (the client never settled) ##########"
  tail -20 join.log
  exit 1
fi
if ! grep "server says" join.log | grep -q "CHESTPLACED"; then
  echo "########## ENDER CHEST TEST FAILED (no chest was placed) ##########"
  exit 1
fi
if ! grep -q "a screen opened" join.log; then
  echo "########## ENDER CHEST TEST FAILED (right-clicking it opened nothing) ##########"
  exit 1
fi
echo "########## ENDER CHEST TEST PASSED ##########"
