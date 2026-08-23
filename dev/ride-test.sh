#!/bin/bash
# Right-click a boat and check the player is actually in it.
#
# Nothing in a command boards a vehicle, so a boat that can be ridden and one
# that only floats look identical from the outside. This sends a real
# ServerboundInteract and reads the SetPassengers that must come back.
#
# Usage: bash dev/ride-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25583
RUN_DIR="$ROOT/run-ride"

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

# A jetty to stand on and a pool beside it, laid one block at a time because
# Steel has no `/fill` yet. All of it matters: a player teleported into mid-air
# falls out of the world, a boat with no water under it sinks slowly enough to
# look fine for a second and then be far out of reach by the time the next
# command lands, and water with an open side pours away into a current that
# carries the boats off. Every water block here is walled in, so it stays put.
CMDS='gamemode creative'
for x in $(seq -1 4); do
  CMDS="$CMDS;;setblock $x 99 0 minecraft:stone"    # the jetty
  CMDS="$CMDS;;setblock $x 98 1 minecraft:stone"    # the pool floor
  CMDS="$CMDS;;setblock $x 99 2 minecraft:stone"    # the far wall
done
CMDS="$CMDS;;setblock -1 99 1 minecraft:stone"      # and the two ends
CMDS="$CMDS;;setblock 4 99 1 minecraft:stone"
for x in 0 1 2 3; do
  CMDS="$CMDS;;setblock $x 99 1 minecraft:water"
done

CMDS="$CMDS;;teleport @s 0 100 0"
CMDS="$CMDS;;summon minecraft:oak_boat 0 100 1"
# Sneaking must not board -- that is the gap a chest boat opens into, and a
# boat that boarded on a sneak would be impossible to open.
CMDS="$CMDS;;!sneakuse oak_boat"
CMDS="$CMDS;;summon minecraft:oak_boat 1 100 1"
CMDS="$CMDS;;teleport @s 1 100 0"
CMDS="$CMDS;;!useentity oak_boat"
# Off again: a rider stays put until the boat is gone, and boarding a second
# vehicle while still in the first is a different question from this one.
CMDS="$CMDS;;kill @e[type=minecraft:oak_boat]"
# A chest boat answers the same two gestures differently: sneaking opens the
# chest, and a plain click still boards.
CMDS="$CMDS;;summon minecraft:oak_chest_boat 2 100 1"
CMDS="$CMDS;;teleport @s 2 100 0"
CMDS="$CMDS;;!sneakuse oak_chest_boat"
CMDS="$CMDS;;!close"
CMDS="$CMDS;;summon minecraft:oak_chest_boat 3 100 1"
CMDS="$CMDS;;teleport @s 3 100 0"
CMDS="$CMDS;;!useentity oak_chest_boat"

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=3 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what happened ==="
grep -E "right-clicked|is carrying|a screen opened|screen was closed" join.log | tail -12
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic|incorrect|unknown" | tail -5

fail() { echo "########## RIDE TEST FAILED ($1) ##########"; exit 1; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }
grep -q "right-clicked the oak_boat" join.log || fail "no boat spawned to click"

# The rider has to be this player, not merely somebody.
player=$(grep -o 'joined the world as entity [0-9]*' join.log | head -1 | awk '{print $NF}')
[ -n "$player" ] || fail "never learned the player entity id"
ridden=$(grep -o '^  right-clicked the oak_boat (entity [0-9]*' join.log   | head -1 | awk '{print $NF}')
[ -n "$ridden" ] || fail "the plain right-click never reached a boat"
grep -q "entity $ridden is carrying \[$player\]" join.log   || fail "boat $ridden is not carrying the player (id $player)"

# and the boat that was only sneaked at must have taken nobody
sneaked=$(grep -o 'sneak-right-clicked the oak_boat (entity [0-9]*' join.log   | head -1 | awk '{print $NF}')
[ -n "$sneaked" ] || fail "the sneak never reached a boat"
! grep -q "entity $sneaked is carrying \[" join.log   || fail "sneaking boarded the boat (entity $sneaked), so a chest boat could never be opened"
# the chest boat: sneaking opened it, and a plain click boarded a second one
grep -q "a screen opened" join.log   || fail "sneaking at a chest boat opened nothing"
chest_ridden=$(grep -o '^  right-clicked the oak_chest_boat (entity [0-9]*' join.log   | head -1 | awk '{print $NF}')
[ -n "$chest_ridden" ] || fail "the chest boat was never right-clicked"
grep -q "entity $chest_ridden is carrying \[$player\]" join.log   || fail "a plain right-click on a chest boat did not board it"
echo "########## RIDE TEST PASSED ##########"
