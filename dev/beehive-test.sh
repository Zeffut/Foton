#!/bin/bash
# Harvest a beehive with shears and with a bottle.
#
# A hive's honey level is a block state, so unlike most interactions this one
# can be read straight back with `execute if block`. Three things are checked:
# shears empty a full hive, a glass bottle empties a full hive, and neither
# touches a hive that is not full yet.
#
# Usage: bash dev/beehive-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25595
RUN_DIR="$ROOT/run-beehive"

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

CMDS='gamemode creative'
# One throwaway command first: the very first `setblock` of a run can land
# before the chunk around the player is ready.
CMDS="$CMDS;;time set day"
CMDS="$CMDS;;setblock 0 99 0 minecraft:stone"
CMDS="$CMDS;;setblock 0 99 2 minecraft:stone"
CMDS="$CMDS;;setblock 0 99 4 minecraft:stone"
CMDS="$CMDS;;setblock 0 100 0 minecraft:beehive[honey_level=5]"
CMDS="$CMDS;;setblock 0 100 2 minecraft:beehive[honey_level=5]"
# The control: a hive that is not full yet gives nothing to anyone.
CMDS="$CMDS;;setblock 0 100 4 minecraft:beehive[honey_level=4]"
CMDS="$CMDS;;execute if block 0 100 0 minecraft:beehive[honey_level=5] run tellraw @s \"HIVESTARTSFULL\""

CMDS="$CMDS;;clear @s"
CMDS="$CMDS;;give @s minecraft:shears"
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;teleport @s 1 100 0"
CMDS="$CMDS;;!useon 0 100 0 east"
CMDS="$CMDS;;execute if block 0 100 0 minecraft:beehive[honey_level=0] run tellraw @s \"SHEARSEMPTIEDIT\""

CMDS="$CMDS;;teleport @s 1 100 4"
CMDS="$CMDS;;!useon 0 100 4 east"
CMDS="$CMDS;;execute if block 0 100 4 minecraft:beehive[honey_level=4] run tellraw @s \"HALFHIVEUNTOUCHED\""

CMDS="$CMDS;;clear @s"
CMDS="$CMDS;;give @s minecraft:glass_bottle"
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;teleport @s 1 100 2"
CMDS="$CMDS;;!useon 0 100 2 east"
CMDS="$CMDS;;execute if block 0 100 2 minecraft:beehive[honey_level=0] run tellraw @s \"BOTTLEEMPTIEDIT\""

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=2 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what happened ==="
grep "server says" join.log \
  | grep -oE "HIVESTARTSFULL|SHEARSEMPTIEDIT|HALFHIVEUNTOUCHED|BOTTLEEMPTIEDIT"
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic" | tail -5

fail() { echo "########## BEEHIVE TEST FAILED ($1) ##########"; exit 1; }
said() { grep "server says" join.log | grep -q "$1"; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }
said HIVESTARTSFULL    || fail "the hive was not full to begin with"
said SHEARSEMPTIEDIT   || fail "shears did not harvest the hive"
said HALFHIVEUNTOUCHED || fail "shears harvested a hive that was not full"
said BOTTLEEMPTIEDIT   || fail "a glass bottle did not harvest the hive"
echo "########## BEEHIVE TEST PASSED ##########"
