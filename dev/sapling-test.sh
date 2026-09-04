#!/bin/bash
# Plant a sapling and check a tree grows where it stood.
#
# A sapling that never grows means a survival world has no renewable wood, and
# no unit test can see that: growing a tree runs a worldgen feature through the
# live-world block path. So this plants one, winds the random tick rate up, and
# asks the world what is standing there afterwards.
#
# Usage: bash dev/sapling-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

# Respect $CARGO_TARGET_DIR the way `cargo build` itself already does; see
# the same lines in dev/join-test.sh for what hardcoding it used to cost.
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
BIN="$TARGET_DIR/debug/foton"

PORT=25573
RUN_DIR="$ROOT/run-sapling"

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
# A scripted client fires commands far faster than a person, and the throttle
# decays one point per game tick -- so a busy server turns this rig's burst of
# `setblock`s into a `disconnect.spam` kick. This test failed exactly that way
# under load.
sed -i 's/^command_spam_threshold_seconds = .*/command_spam_threshold_seconds = 0/' \n  "$RUN_DIR/config/config.toml"
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

# One client does the whole thing: the server refuses a second connection under
# the same name.
#
# A patch of dirt in open air well above the terrain, so the tree has room and
# nothing else is in the way. The random tick rate goes up because a sapling
# needs two of them and the default rate would make this a coin flip on the
# clock rather than on the code.
CMDS='gamerule random_tick_speed 4000'
CMDS="$CMDS;;teleport @s 0 100 0"
for x in -2 -1 0 1 2; do
  for z in -2 -1 0 1 2; do
    CMDS="$CMDS;;setblock $x 99 $z minecraft:dirt"
  done
done
CMDS="$CMDS;;setblock 0 100 0 minecraft:oak_sapling"
# Ask repeatedly: each command is followed by a settle window, so this both
# waits and checks. An oak is at least four blocks tall, so a log two above the
# sapling means a tree and not a leftover.
for _ in $(seq 1 15); do
  CMDS="$CMDS;;execute if block 0 102 0 #minecraft:logs run tellraw @s \"SAPLINGTESTTREEGREW\""
done

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=5 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== join.log ==="
grep -E "server says|before the commands|spawned|JOIN" join.log | tail -8
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic|unknown|incorrect" | tail -10

if [ $STATUS -ne 0 ]; then
  echo "########## SAPLING TEST FAILED (the client never settled) ##########"
  tail -20 join.log
  exit 1
fi
# Only the server's own reply counts: `join.py` echoes each command it sends,
# and the command text contains the marker too.
if ! grep "server says" join.log | grep -q "SAPLINGTESTTREEGREW"; then
  echo "########## SAPLING TEST FAILED (no log block above the sapling) ##########"
  exit 1
fi
echo "########## SAPLING TEST PASSED ##########"
