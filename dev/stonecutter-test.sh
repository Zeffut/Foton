#!/bin/bash
# Cut a block of stone into something else.
#
# A stonecutter is the first workstation whose whole point is a recipe list:
# one block matches many recipes and the player picks one by pressing a button,
# so the test has to press that button rather than just open the screen.
#
# Usage: bash dev/stonecutter-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25593
RUN_DIR="$ROOT/run-stonecutter"

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

CMDS='gamemode creative'
# One throwaway command first: the very first `setblock` of a run can land
# before the chunk around the player is ready.
CMDS="$CMDS;;time set day"
CMDS="$CMDS;;setblock 0 99 0 minecraft:stone"
CMDS="$CMDS;;setblock 0 100 0 minecraft:stonecutter"
CMDS="$CMDS;;teleport @s 1 100 0"

CMDS="$CMDS;;clear @s"
CMDS="$CMDS;;give @s minecraft:andesite 8"
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;!useon 0 100 0 east"

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=2 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what happened ==="
grep -E "a screen opened|server says" join.log | tail -4
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic|incorrect" | tail -5

fail() { echo "########## STONECUTTER TEST FAILED ($1) ##########"; exit 1; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }
grep -q "a screen opened" join.log || fail "right-clicking a stonecutter opened nothing"
echo "########## STONECUTTER TEST PASSED ##########"
