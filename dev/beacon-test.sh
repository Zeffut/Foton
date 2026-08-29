#!/bin/bash
# Build a beacon, pay it, and check the effect reaches a player.
#
# Nothing a command can read says a beacon worked: the pyramid count, the
# chosen effects and the effect itself are all server-side state with no
# command behind them. What the client does see is the effect packet, so that
# is what this asserts -- which also means the whole chain is under test at
# once: pyramid, clear sky, payment, the SetBeacon packet, and the four-second
# beat the beacon applies on.
#
# Usage: bash dev/beacon-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25594
RUN_DIR="$ROOT/run-beacon"

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
# The beacon on its own, with nothing under it. A beacon with no pyramid must
# refuse the effect outright, which is the control for everything below.
CMDS="$CMDS;;setblock 0 100 0 minecraft:beacon"
CMDS="$CMDS;;clear @s"
CMDS="$CMDS;;give @s minecraft:iron_ingot 1"
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;teleport @s 2 100 0"
# Four seconds is one beacon beat: long enough for it to have counted a
# pyramid, if there were one.
CMDS="$CMDS;;!wait 4"
CMDS="$CMDS;;!useon 0 100 0 east"
# Slot 28 is the first hotbar square: one payment slot, then the twenty-seven
# main inventory squares.
CMDS="$CMDS;;!shiftclick 28"
CMDS="$CMDS;;!setbeacon speed"
CMDS="$CMDS;;!wait 5"
CMDS="$CMDS;;!close"

# Now the pyramid: one ring of nine iron blocks, which is all speed needs.
CMDS="$CMDS;;setblock -1 99 -1 minecraft:iron_block"
CMDS="$CMDS;;setblock 0 99 -1 minecraft:iron_block"
CMDS="$CMDS;;setblock 1 99 -1 minecraft:iron_block"
CMDS="$CMDS;;setblock -1 99 0 minecraft:iron_block"
CMDS="$CMDS;;setblock 0 99 0 minecraft:iron_block"
CMDS="$CMDS;;setblock 1 99 0 minecraft:iron_block"
CMDS="$CMDS;;setblock -1 99 1 minecraft:iron_block"
CMDS="$CMDS;;setblock 0 99 1 minecraft:iron_block"
CMDS="$CMDS;;setblock 1 99 1 minecraft:iron_block"
CMDS="$CMDS;;!wait 5"

CMDS="$CMDS;;clear @s"
CMDS="$CMDS;;give @s minecraft:iron_ingot 1"
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;!useon 0 100 0 east"
CMDS="$CMDS;;!shiftclick 28"
CMDS="$CMDS;;!setbeacon speed"
CMDS="$CMDS;;!close"
# The payment is only taken once the beacon accepts the effects, so an empty
# inventory afterwards is the second half of the same answer.
CMDS="$CMDS;;clear @s"

# A second beacon, roofed with bedrock. Bedrock dampens light as fully as
# stone, and vanilla's column walk spells out an exception for it -- which is
# the only reason a beacon under the Nether roof works. Haste rather than speed
# so the grant is told apart from the first beacon's.
CMDS="$CMDS;;setblock 31 99 -1 minecraft:iron_block"
CMDS="$CMDS;;setblock 32 99 -1 minecraft:iron_block"
CMDS="$CMDS;;setblock 33 99 -1 minecraft:iron_block"
CMDS="$CMDS;;setblock 31 99 0 minecraft:iron_block"
CMDS="$CMDS;;setblock 32 99 0 minecraft:iron_block"
CMDS="$CMDS;;setblock 33 99 0 minecraft:iron_block"
CMDS="$CMDS;;setblock 31 99 1 minecraft:iron_block"
CMDS="$CMDS;;setblock 32 99 1 minecraft:iron_block"
CMDS="$CMDS;;setblock 33 99 1 minecraft:iron_block"
CMDS="$CMDS;;setblock 32 100 0 minecraft:beacon"
CMDS="$CMDS;;setblock 32 106 0 minecraft:bedrock"
CMDS="$CMDS;;teleport @s 34 100 0"
CMDS="$CMDS;;!wait 5"
CMDS="$CMDS;;clear @s"
CMDS="$CMDS;;give @s minecraft:iron_ingot 1"
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;!useon 32 100 0 east"
CMDS="$CMDS;;!shiftclick 28"
CMDS="$CMDS;;!setbeacon haste"
CMDS="$CMDS;;!wait 5"
CMDS="$CMDS;;!close"

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=6 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what happened ==="
grep -E "got the effect|a screen opened|asked the beacon" join.log
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic" | tail -5

fail() { echo "########## BEACON TEST FAILED ($1) ##########"; exit 1; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }
screens=$(grep -c "a screen opened" join.log)
[ "$screens" -eq 3 ] || fail "expected a beacon to open three times, got $screens"

effects=$(grep -c "got the effect speed" join.log)
[ "$effects" -ge 1 ] || fail "the beacon never handed out speed"

roofed=$(grep -c "got the effect haste" join.log)
[ "$roofed" -ge 1 ] || fail "a beacon under a bedrock roof handed out nothing"

# The control: the first attempt came before the pyramid existed, so the very
# first `clear` must have found the ingot still in the inventory.
grep -q "commands.clear.success.single" join.log \
  || fail "a beacon with no pyramid took the payment anyway"

echo "########## BEACON TEST PASSED ##########"
