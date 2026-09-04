#!/bin/bash
# Watch a villager keep its working day, end to end: it sleeps at night, and it
# works a field in the morning.
#
# A villager's day crosses more systems than any unit test can stand up at
# once: the `villager_schedule` timeline has to be loaded and sampled as a
# string-valued track, the sample has to reach the brain's schedule, the brain
# has to be ticked from the mob's server step, `AcquirePoi` has to notice a bed
# is a point of interest and path to it, the REST package has to reach
# `SleepInBed`, and `WakeUp` has to get the villager out again when the clock
# turns. This drives all of it through a real client.
#
# The bed's own `occupied` block state is the assertion for the night half. It
# is the one part of sleeping a command can read back, and it can only be true
# if a body is in the bed -- nothing else in the game sets it.
#
# The bed is deliberately five blocks from where the villager is summoned, so
# the villager has to walk there: a test with the bed underfoot would pass even
# if `SetWalkTargetFromBlockMemory` and `MoveToTargetSink` never ran.
#
# The morning half is the field. A composter turns the villager into a farmer,
# the `SECONDARY_POIS` sensor has to see the farmland, and `HarvestFarmland` has
# to tell a ripe crop from a growing one through the block behavior, pull it,
# and put a seed back in the square. The field starts as `wheat[age=7]` on every
# square and random ticking is off, so `wheat[age=0]` on any of them can only be
# a seed this villager planted -- and a square that has gone to `air` is at
# least the harvest half.
#
# The villager is summoned holding its seeds on purpose. Where a farmer's seeds
# really come from is the drop of the crop it just pulled, but whether it ever
# gets them is a dice roll this test cannot make: `getPickupReach` is
# `(1, 0, 1)` with no vertical slack, so the villager only gathers the drop if
# it happens to be standing at field level when the crop falls, and the work
# package only offers `HarvestFarmland` one round in six. Waiting for both is
# how `SQUARESOWN` came and went between runs. The gathering link has a
# deterministic test of its own instead:
# `the_wheat_a_farmer_pulls_leaves_seeds_it_can_gather`.
#
# `CanPickUpLoot` is set for the same reason it belongs on any summoned mob
# meant to behave like a spawned one. `Mob.readAdditionalSaveData` reads the
# flag with a default of false and `/summon` runs that reader whether or not a
# compound was typed, so a summoned villager cannot pick anything up at all --
# vanilla's behaviour, and the one that silently switched this test's sowing
# off when `/summon` started loading entity NBT.
#
# Usage: bash dev/villager-day-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

# Respect $CARGO_TARGET_DIR the way `cargo build` itself already does; see
# the same lines in dev/join-test.sh for what hardcoding it used to cost.
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
BIN="$TARGET_DIR/debug/foton"

PORT=25605
RUN_DIR="$ROOT/run-villager-day"

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
# Peaceful so nothing spawns in the dark to frighten the villager into the
# PANIC package, which would quite correctly stop it going to bed.
CMDS="$CMDS;;difficulty peaceful"
# One throwaway command first: the very first `setblock` of a run can land
# before the chunk around the player is ready.
CMDS="$CMDS;;teleport @s 3 101 4"

# --- the bedroom ---------------------------------------------------------
# A floor to walk on, from the bed at z=0 out to the villager at x=5.
for x in -1 0 1 2 3 4 5 6; do
  for z in -1 0 1 2; do
    CMDS="$CMDS;;setblock $x 99 $z minecraft:stone"
  done
done
# Only the head half is a `minecraft:home` point of interest; the foot is
# there so the bed looks like one to a player watching.
CMDS="$CMDS;;setblock 0 100 1 minecraft:red_bed[facing=north,part=foot]"
CMDS="$CMDS;;setblock 0 100 0 minecraft:red_bed[facing=north,part=head]"
CMDS="$CMDS;;summon minecraft:villager 5 100 0 {CanPickUpLoot:1b,Inventory:[{id:\"minecraft:wheat_seeds\",count:8}]}"
CMDS="$CMDS;;execute if entity @e[type=minecraft:villager] run tellraw @s \"VILLAGERSTANDING\""
CMDS="$CMDS;;execute if block 0 100 0 minecraft:red_bed[occupied=false] run tellraw @s \"BEDSTARTSEMPTY\""

# --- the field -----------------------------------------------------------
# Ripe wheat on farmland, right where the villager is summoned, with the
# composter that gives it the farmer trade next to it. `AcquirePoi` only takes
# a workstation it can path to and a composter's own reach is one block, so the
# composter is adjacent rather than across the room.
FIELD_X="3 4 5"
FIELD_Z="-1 0 1"
for x in $FIELD_X; do
  for z in $FIELD_Z; do
    CMDS="$CMDS;;setblock $x 99 $z minecraft:farmland"
    CMDS="$CMDS;;setblock $x 100 $z minecraft:wheat[age=7]"
  done
done
CMDS="$CMDS;;setblock 6 100 0 minecraft:composter"
CMDS="$CMDS;;execute if block 5 100 0 minecraft:wheat[age=7] run tellraw @s \"FIELDISRIPE\""

# --- night ---------------------------------------------------------------
# 12000 onward is the REST stretch of `Timelines.VILLAGER_SCHEDULE`.
CMDS="$CMDS;;time set 13000"
# The bed claim is booked on a jittered scan, and then the villager has five
# blocks to walk, so give it a good while. Thirty rather than twenty because
# this half failed one run in four while another build had the machine: the
# scan's jitter plus a five-block walk is not a fixed cost, and a test that
# only passes on an idle box is a test that lies on a busy one.
CMDS="$CMDS;;!wait 30"
CMDS="$CMDS;;execute if block 0 100 0 minecraft:red_bed[occupied=true] run tellraw @s \"VILLAGERINBED\""

# --- morning -------------------------------------------------------------
# 2000..9000 is WORK, so the villager has no business in bed.
CMDS="$CMDS;;time set 3000"
CMDS="$CMDS;;!wait 8"
CMDS="$CMDS;;execute if block 0 100 0 minecraft:red_bed[occupied=false] run tellraw @s \"VILLAGERUPANDABOUT\""

# --- the working day -----------------------------------------------------
# The villager has to walk back from the bed, claim the composter and take the
# farmer trade before the WORK package starts offering `HarvestFarmland` at
# all -- and that package picks one of six behaviors at random each round, so
# the wait for the farming one is measured in thousands of ticks rather than
# hundreds. Sprinting is what keeps this test to a sane wall-clock length;
# freezing the day first is what stops the sprint running the clock out of the
# WORK stretch and putting the villager back to bed.
CMDS="$CMDS;;gamerule advance_time false"
# Stop crops growing during the sprint. `SQUARESOWN` asks for `wheat[age=0]`,
# and twelve thousand ticks of random ticks are ample for a freshly sown seed
# to leave that state -- the assertion would then miss a replant that did
# happen. The field is set to `age=7` by hand, so nothing here needs growth.
CMDS="$CMDS;;gamerule random_tick_speed 0"
CMDS="$CMDS;;time set 3000"
CMDS="$CMDS;;tick sprint 12000t"
CMDS="$CMDS;;!wait 90"
for x in $FIELD_X; do
  for z in $FIELD_Z; do
    CMDS="$CMDS;;execute if block $x 100 $z minecraft:air run tellraw @s \"SQUAREPULLED\""
    CMDS="$CMDS;;execute if block $x 100 $z minecraft:wheat[age=0] run tellraw @s \"SQUARESOWN\""
  done
done

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=4 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what happened ==="
grep "server says" join.log \
  | grep -oE "VILLAGERSTANDING|BEDSTARTSEMPTY|FIELDISRIPE|VILLAGERINBED|VILLAGERUPANDABOUT|SQUAREPULLED|SQUARESOWN" \
  | sort | uniq -c
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic" | tail -5

fail() { echo "########## VILLAGER DAY TEST FAILED ($1) ##########"; exit 1; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }

# Each marker has to arrive as chat. Grepping the whole log would also match
# the echo of the command that asks the question, which is printed whether the
# answer was yes or no.
said() { grep -q "server says: $1" join.log; }

said VILLAGERSTANDING \
  || fail "the villager never spawned, so nothing below is about its day"
said BEDSTARTSEMPTY \
  || fail "the bed was not placed, or was occupied before anyone slept in it"
said VILLAGERINBED \
  || fail "the villager never walked to its bed and got in once the clock said REST"
said VILLAGERUPANDABOUT \
  || fail "the villager stayed in bed after the clock moved on to WORK"
said FIELDISRIPE \
  || fail "the field was never planted, so nothing below is about farming"
said SQUAREPULLED || said SQUARESOWN \
  || fail "the farmer never pulled a ripe crop out of its own field"
said SQUARESOWN \
  || fail "the farmer pulled the wheat but never sowed a square again"

echo "########## VILLAGER DAY TEST PASSED ##########"
