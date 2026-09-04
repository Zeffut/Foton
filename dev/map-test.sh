#!/bin/bash
# Draw a map, then take it to a cartography table.
#
# What a map does is mostly arithmetic over block colors, and that is tested
# in Rust where the arithmetic lives. What only a real client can answer is
# whether the whole chain runs at all: a blank map turning into a filled one,
# the per-tick pass noticing it, the map packet reaching the client, and the
# cartography table taking that map and handing back a zoomed-out one.
#
# The map packet is the assertion. It carries the map's id and its zoom level,
# so seeing `id=0 scale=0` and then `id=1 scale=1` is the whole feature end to
# end: a map was created, drawn and sent, then cloned into a new one at the
# next zoom.
#
# Usage: bash dev/map-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

# Respect $CARGO_TARGET_DIR the way `cargo build` itself already does; see
# the same lines in dev/join-test.sh for what hardcoding it used to cost.
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
BIN="$TARGET_DIR/debug/foton"

PORT=25599
RUN_DIR="$ROOT/run-map"

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

CMDS='gamemode creative'
# One throwaway command first: the very first `setblock` of a run can land
# before the chunk around the player is ready.
CMDS="$CMDS;;time set day"
CMDS="$CMDS;;clear @s"

# A blank map turns into a filled one where the player stands.
CMDS="$CMDS;;give @s minecraft:map 1"
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;!useitem"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute if data entity @s Inventory[{id:\"minecraft:filled_map\"}] run tellraw @s \"MAPMADE\""

# In creative the blank map is not spent, so it has to go before the table
# sees it -- a blank map in the second slot is the copy recipe, not the zoom
# one. That leaves the filled map alone in the hotbar.
CMDS="$CMDS;;clear @s minecraft:map"

# The cartography table. A stone pedestal first, the way the workstation test
# does it, so the table has something to sit on.
CMDS="$CMDS;;setblock 0 99 0 minecraft:stone"
CMDS="$CMDS;;setblock 0 100 0 minecraft:cartography_table"
CMDS="$CMDS;;give @s minecraft:paper 1"
CMDS="$CMDS;;teleport @s 1 100 0"
CMDS="$CMDS;;!useon 0 100 0 east"
# Slots 3 to 39 are the player inventory; 30 and 31 are the first two hotbar
# squares, holding the filled map and the paper.
CMDS="$CMDS;;!shiftclick 30"
CMDS="$CMDS;;!shiftclick 31"
# Taking the result spends one of each input and hands back a map marked to be
# zoomed out. The mark is resolved on the first tick after it lands in a real
# slot, which is what the second map packet reports.
CMDS="$CMDS;;!shiftclick 2"
CMDS="$CMDS;;!close"
CMDS="$CMDS;;!wait 1"

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=2 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what happened ==="
grep -c "a screen opened" join.log | sed 's/^/screens opened: /'
grep -o "map data id=[0-9]* scale=[0-9]* locked=[0-9]*" join.log | sort -u
grep "server says" join.log | grep -oE "MAPMADE"
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic|incorrect" | tail -5

fail() { echo "########## MAP TEST FAILED ($1) ##########"; exit 1; }
said() { grep "server says" join.log | grep -q "$1"; }
sent() { grep -q "map data $1" join.log; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }
said MAPMADE || fail "using a blank map did not produce a filled one"
sent "id=0 scale=0 locked=0" || fail "the new map never reached the client"
screens=$(grep -c "a screen opened" join.log)
[ "$screens" -eq 1 ] || fail "expected the cartography table to open once, got $screens"
sent "id=1 scale=1 locked=0" || fail "the cartography table did not zoom the map out"
echo "########## MAP TEST PASSED ##########"
