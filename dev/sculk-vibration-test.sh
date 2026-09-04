#!/bin/bash
# Walk a player past a sculk sensor and watch the redstone come out of it.
#
# This is the one chain no unit test holds in one piece. A player sends position
# packets; `move_entity` turns them into real movement and, once the walk has
# carried them past the next whole block, fires the `step` game event from their
# feet; the chunk's game-event registry hands that event to the sensor's
# vibration listener, which measures the distance, checks that no wool is in the
# way and schedules a vibration; the sensor's block-entity ticker walks that
# vibration one block per tick and finally calls `SculkSensorBlock.activate`,
# which sets `sculk_sensor_phase=active` and a `power` that says how far away the
# step was. Movement, game events, vibrations and redstone, in that order --
# every one of which has looked fine on its own.
#
# Two sensors are laid out, and the second one is the point: it is six blocks
# from the middle of the walk, well inside its eight-block radius, but behind a
# wall of wool, and it must stay inactive while the first one fires. That is the
# occlusion test, and it is the half a "did anything happen at all" assertion
# misses.
#
# The clock is frozen for the walk and stepped by hand afterwards, which buys
# two things. A sensor is only active for thirty ticks and every question this
# script asks costs two seconds, so without the freeze it would have gone quiet
# again before the second question was asked. And with the clock stopped every
# footstep of the walk lands on the same game tick, which is exactly the case
# the vibration selector exists for: it must pick the nearest of them and throw
# the rest away. The walk runs from one side of the sensor to the other, so the
# nearest footstep is one of the two blocks either side of it -- both three
# blocks from the sensor once rounded, both power ten.
#
# The assertions are positive and read off block state -- `execute if block ...
# [sculk_sensor_phase=active]` -- never `unless ... [sculk_sensor_phase=inactive]`,
# which is also true when the block is not there at all.
#
# Usage: bash dev/sculk-vibration-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

# Respect $CARGO_TARGET_DIR the way `cargo build` itself already does; see
# the same lines in dev/join-test.sh for what hardcoding it used to cost.
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
BIN="$TARGET_DIR/debug/foton"

PORT=25617
RUN_DIR="$ROOT/run-sculk-vibration"

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

# Foton exempts operators from command-rate spam accounting, and this test asks
# the world several questions. `default_groups` alone does not make the player
# one: `is_operator` reads a stored permission state, which only exists once
# somebody has been opped by name.
CMDS='op SmokeTester'
CMDS="$CMDS;;gamemode creative"
# One throwaway command first: the very first `setblock` of a run can land
# before the chunk around the player is ready.
CMDS="$CMDS;;time set noon"

# --- the floor, the sensors and the wall ----------------------------------
# A hundred blocks up, where whatever the world generated is somebody else's
# problem and the air above is already air. The walk runs along z=0.
for X in $(seq -9 9); do
  for Z in -1 0 1 2 3 4 5 6 7; do
    CMDS="$CMDS;;setblock $X 99 $Z minecraft:stone"
  done
done
# The sensor that should hear the walk, three blocks off the path.
CMDS="$CMDS;;setblock 0 100 3 minecraft:sculk_sensor"
# A comparator reading out of that sensor. It is placed now rather than after
# the walk on purpose: `/setblock` never makes a comparator evaluate itself, in
# vanilla either -- only a neighbour update does, and the one the sensor sends
# when it activates is the update this is here to catch. `facing` points at what
# a comparator reads, so south is the sensor.
CMDS="$CMDS;;setblock 0 100 2 minecraft:comparator[facing=south]"
# The sensor that should not: six blocks off the middle of the path, still well
# inside its eight-block radius, but with a solid wool wall between it and every
# footstep close enough to reach it.
CMDS="$CMDS;;setblock 0 100 6 minecraft:sculk_sensor"
for X in $(seq -9 9); do
  CMDS="$CMDS;;setblock $X 100 5 minecraft:white_wool"
done

# --- the walk -------------------------------------------------------------
# Land on the floor first and let the teleport settle: the server holds the
# player at the old position until the client confirms it, and a walk that
# starts before that is a walk from nowhere.
CMDS="$CMDS;;teleport @s -8 100 0"
CMDS="$CMDS;;!wait 2"
# Freeze, then walk. A frozen server still moves a player -- that is what makes
# `/tick freeze` usable at all -- but nothing it sets in motion advances until
# the clock is stepped, and the game time the footsteps are stamped with does
# not move either.
CMDS="$CMDS;;tick freeze"
# Sixty-four strides of a quarter block carries the player from x=-8 to x=8,
# straight past the sensor. A step is emitted every time the distance walked
# crosses the next whole block, so this produces several, spread along the path,
# and the selector has a real choice to make.
CMDS="$CMDS;;!walk -8 100 0 0.25 0 64"
# Ten ticks: enough for the selector to commit its choice and for the vibration
# to travel its three blocks, and well short of the thirty the sensor then stays
# active for.
CMDS="$CMDS;;tick step 10"
CMDS="$CMDS;;!wait 2"

# --- what the sensors say -------------------------------------------------
CMDS="$CMDS;;execute if block 0 100 3 minecraft:sculk_sensor[sculk_sensor_phase=active] run tellraw @s \"SENSOR_HEARD\""
CMDS="$CMDS;;execute if block 0 100 6 minecraft:sculk_sensor[sculk_sensor_phase=active] run tellraw @s \"WALLED_SENSOR_HEARD\""
# The redstone half. The chosen footstep is three blocks from the sensor once
# rounded to a block, and `15 - floor(15/8 * 3)` is ten. Asking for the exact
# number rather than "not zero" is what catches a strength that stopped scaling
# with distance.
CMDS="$CMDS;;execute if block 0 100 3 minecraft:sculk_sensor[power=10] run tellraw @s \"SENSOR_POWERED_TEN\""
# The comparator beside the sensor reports the frequency stored in its block
# entity, which is non-zero for a step. That proves the block entity was written
# and not just the block state.
CMDS="$CMDS;;execute if block 0 100 2 minecraft:comparator[powered=true] run tellraw @s \"COMPARATOR_READ_FREQUENCY\""
CMDS="$CMDS;;tick unfreeze"

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=2 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what happened ==="
grep -E "server says: (SENSOR|WALLED|COMPARATOR)|walked .* strides" join.log
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic" | tail -5

fail() { echo "########## SCULK VIBRATION TEST FAILED ($1) ##########"; exit 1; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }

# The marker has to arrive as chat. Grepping the whole log would also match the
# echo of the command that asks the question, which is printed whether the
# condition held or not.
said() { grep -q "server says: $1" join.log; }

said SENSOR_HEARD \
  || fail "the sensor never went active, so the walk never became a vibration"
said SENSOR_POWERED_TEN \
  || fail "the sensor is active but its redstone power is not the walk's distance"
said COMPARATOR_READ_FREQUENCY \
  || fail "a comparator on the active sensor reads nothing, so no frequency was stored"
if said WALLED_SENSOR_HEARD; then
  fail "the walled sensor heard through wool, so occlusion is not stopping anything"
fi

echo "########## SCULK VIBRATION TEST PASSED ##########"
