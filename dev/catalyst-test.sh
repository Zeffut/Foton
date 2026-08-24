#!/bin/bash
# Kill mobs next to a sculk catalyst and watch sculk grow out of them.
#
# This is the one block whose work crosses three systems at once. A mob dies,
# `LivingEntity.die` fires the `ENTITY_DIE` game event before it drops anything,
# the catalyst's block entity hears it from within eight blocks, takes the
# experience the death was about to drop, and hands it to a `SculkSpreader` as
# charge cursors; the catalyst then walks those cursors a step per tick and
# converts the stone under them into sculk. Nothing short of a running server
# exercises that chain -- the catalyst is also the first block entity in Steel
# to publish a game-event listener at all, so this is the only test that proves
# the chunk registry actually delivers one.
#
# The assertion is positive and read off block state: stone around the death
# spot becomes `minecraft:sculk`. Which block exactly is up to a shuffle, so the
# floor is swept one block at a time and any hit reports the same marker.
#
# That single marker is also what proves the experience was consumed rather than
# dropped, because a charge cursor has no other source than
# `getExperienceReward`: sculk on the floor means the reward went into the
# spreader. The kills use `/kill`, which credits no player, so vanilla would
# drop no orbs here either -- the orb half of the story is the ordering inside
# `living_die`, which the unit tests cover.
#
# Usage: bash dev/catalyst-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25603
RUN_DIR="$ROOT/run-catalyst"

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

# Steel exempts operators from command-rate spam accounting, and this test asks
# the world a few dozen questions. `default_groups` alone does not make the
# player one: `is_operator` reads a stored permission state, which only exists
# once somebody has been opped by name.
CMDS='op SmokeTester'
CMDS="$CMDS;;gamemode creative"
# A monster refuses to exist on peaceful, and its reward is what feeds this.
CMDS="$CMDS;;difficulty normal"
# One throwaway command first: the very first `setblock` of a run can land
# before the chunk around the player is ready.
CMDS="$CMDS;;time set noon"

# --- the floor -----------------------------------------------------------
# Stone is in `#minecraft:sculk_replaceable`, so it is what the charge turns
# into sculk. A hundred blocks up, where the ground below is somebody else's
# problem and the air above is already air.
for X in -2 -1 0 1 2; do
  for Z in -2 -1 0 1 2; do
    CMDS="$CMDS;;setblock $X 99 $Z minecraft:stone"
  done
done
CMDS="$CMDS;;setblock 0 100 0 minecraft:sculk_catalyst"
CMDS="$CMDS;;teleport @s 0 100 -2"
# The listener is committed by the chunk once the block entity starts ticking,
# so give it a moment before anything dies.
CMDS="$CMDS;;!wait 2"
# --- the deaths -----------------------------------------------------------
# Six blazes at ten experience each is sixty charge: enough to convert a
# handful of blocks and plant a growth or two. They die two blocks from the
# catalyst, well inside its eight-block listening radius.
#
# A blaze rather than the obvious zombie, and the reason is a bug in the mobs
# rather than in the catalyst: vanilla's `Monster` constructor sets
# `xpReward = 5`, and Steel's monsters never do. Fifteen hostiles -- zombie,
# skeleton, creeper, spider, husk, stray, drowned, cave spider, enderman,
# silverfish, witch, wither skeleton, zombified piglin, and the two slimes
# whose reward is their size -- currently reward nothing at all, so a catalyst
# next to one correctly eats nothing. The blaze is one of the eight that do
# carry their reward, which is what makes it the mob that measures the
# catalyst instead of the gap.
for _ in 1 2 3 4 5 6; do
  CMDS="$CMDS;;summon minecraft:blaze 0 100 2"
done
CMDS="$CMDS;;!wait 2"
CMDS="$CMDS;;kill @e[type=minecraft:blaze]"

# --- the spread -----------------------------------------------------------
# A cursor moves one block a tick and rolls for decay on the way, so give it a
# few seconds before looking.
CMDS="$CMDS;;!wait 8"
# Only the two rows the deaths happen over: every question costs two seconds,
# and a cursor cannot leave the platform anyway -- it only ever steps onto a
# block that is already sculk or sculk vein.
for X in -2 -1 0 1 2; do
  for Z in 1 2; do
    CMDS="$CMDS;;execute if block $X 99 $Z minecraft:sculk run tellraw @s {\"text\":\"SCULK_GREW\"}"
  done
done

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=2 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what happened ==="
grep -E "server says: SCULK|before the commands|spawned around the player" join.log
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic" | tail -5

fail() { echo "########## CATALYST TEST FAILED ($1) ##########"; exit 1; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }

# The marker has to arrive as chat. Grepping the whole log would also match the
# echo of the command that asks the question, which is printed whether the
# condition held or not.
said() { grep -q "server says: $1" join.log; }

said SCULK_GREW \
  || fail "no sculk grew, so the deaths never reached the catalyst's spreader"

echo "########## CATALYST TEST PASSED ##########"
