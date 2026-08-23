#!/bin/bash
# Feed a furnace minecart and watch it drive itself down a track.
#
# A furnace cart is the one cart that moves without a powered rail: the fuel
# gives it a push, pointing away from whoever fed it. The track ends against a
# block so the cart comes to rest on a detector rail, whose `powered` state is
# the only part of any of this a command can read.
#
# Usage: bash dev/furnace-minecart-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25597
RUN_DIR="$ROOT/run-furnacecart"

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
for z in 0 1 2 3 4 5 6 7; do
  CMDS="$CMDS;;setblock 0 99 $z minecraft:stone"
done
for z in 0 1 2 3 4 5; do
  CMDS="$CMDS;;setblock 0 100 $z minecraft:rail[shape=north_south]"
done
CMDS="$CMDS;;setblock 0 100 6 minecraft:detector_rail[shape=north_south]"
# A buffer, so the cart comes to rest on the detector rail rather than running
# off the end of the track and out of the test.
CMDS="$CMDS;;setblock 0 100 7 minecraft:stone"
CMDS="$CMDS;;execute if block 0 100 6 minecraft:detector_rail[powered=false] run tellraw @s \"RAILSTARTSQUIET\""

CMDS="$CMDS;;clear @s"
CMDS="$CMDS;;give @s minecraft:coal"
CMDS="$CMDS;;!hotbar 0"
# Standing behind the cart is how a player picks the direction: the push points
# away from whoever fed it.
CMDS="$CMDS;;teleport @s 0 100 -2"
CMDS="$CMDS;;summon furnace_minecart 0.5 100.0 0.5"
CMDS="$CMDS;;!useentity furnace_minecart"
CMDS="$CMDS;;!wait 6"
CMDS="$CMDS;;execute if block 0 100 6 minecraft:detector_rail[powered=true] run tellraw @s \"CARTARRIVED\""

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=2 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what happened ==="
grep "server says" join.log | grep -oE "RAILSTARTSQUIET|CARTARRIVED"
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic" | tail -5

fail() { echo "########## FURNACE MINECART TEST FAILED ($1) ##########"; exit 1; }
said() { grep "server says" join.log | grep -q "$1"; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }
said RAILSTARTSQUIET || fail "the detector rail was already on"
said CARTARRIVED     || fail "the cart never drove itself down the track"
echo "########## FURNACE MINECART TEST PASSED ##########"
