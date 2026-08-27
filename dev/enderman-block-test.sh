#!/bin/bash
# Stand an enderman in a ring of grass and watch it walk off with a block.
#
# `EndermanTakeBlockGoal` and `EndermanLeaveBlockGoal` sit at priorities 11 and
# 10 of the enderman's goal selector, and both are gated behind a die roll --
# one attempt in ten to take, one in a thousand to leave. The unit tests drive
# the goal bodies directly, which proves what the bodies do but not that the
# selector ever reaches them. This does: it summons an enderman and lets the
# real tick run.
#
# Midnight, because daylight makes an enderman teleport away from its ring, and
# clear weather, because rain hurts it into teleporting too.
#
# The grass is at the enderman's own feet level and above rather than under it:
# the take box is `y .. y+3`, so a floor is out of reach by construction.
#
# Usage: bash dev/enderman-block-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25629
RUN_DIR="$ROOT/run-enderman-block"

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
# A scripted client fires commands far faster than a person, and the throttle
# decays per game tick -- so a busy server turns a normal rig into a kick.
sed -i 's/^command_spam_threshold_seconds = .*/command_spam_threshold_seconds = 0/' "$RUN_DIR/config/config.toml"

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

CMDS='gamemode spectator'
CMDS="$CMDS;;time set 18000"
CMDS="$CMDS;;weather clear"
CMDS="$CMDS;;teleport @s 0 108 0"
# Nothing that wandered in on its own may take part.
CMDS="$CMDS;;gamerule spawn_mobs false"
CMDS="$CMDS;;kill @e[type=minecraft:enderman]"

# A slab floor, then grass filling every cell around the middle for the
# enderman's whole height. The middle stays empty for it to stand in.
#
# Two details in that are load-bearing, both learned from watching this test
# fail. An enderman has a step height of one, so a single ring at foot level is
# a kerb it walks over: the first rig watched it stroll off the platform and
# spend two thousand ticks in a field with nothing holdable in reach. And the
# floor is a slab rather than stone because `canPlaceBlock` demands a full
# collision block underneath -- with a slab no cell in the leave goal's reach is
# ever legal, so a block this enderman picks up is a block it keeps. Otherwise
# the leave goal is free to set the block down in the enderman's own cell, which
# vanilla allows (it passes itself as the exclusion), and the suffocation that
# follows teleports it out of the rig.
for x in -1 0 1; do
  for z in -1 0 1; do
    CMDS="$CMDS;;setblock $x 99 $z minecraft:smooth_stone_slab"
    if [ "$x" != 0 ] || [ "$z" != 0 ]; then
      for y in 100 101 102; do
        CMDS="$CMDS;;setblock $x $y $z minecraft:grass_block"
      done
    fi
  done
done

CMDS="$CMDS;;summon minecraft:enderman 0 100 0 {PersistenceRequired:1b,Silent:1b,Tags:[\"carrier\"]}"

# The controls, asked before anything is allowed to tick.
CMDS="$CMDS;;execute if entity @e[type=minecraft:enderman,tag=carrier] run tellraw @s \"ENDERMANUP\""
CMDS="$CMDS;;execute if block 1 100 0 minecraft:grass_block run tellraw @s \"RINGDOWN\""
# And the flag came off the summon NBT, which is also the load path the carried
# block itself uses.
CMDS="$CMDS;;execute if entity @e[type=minecraft:enderman,tag=carrier,nbt={PersistenceRequired:1b}] run tellraw @s \"ENDERMANPINNED\""

# A take attempt is one tick in ten and lands on a reachable cell one time in
# four, so two hundred ticks is already a near-certainty and nothing in this rig
# can take the block away again. The checkpoints repeat only so a failure says
# how long it held out.
CARRYING='nbt={carriedBlockState:{Name:"minecraft:grass_block"}}'
for _ in 1 2 3; do
  CMDS="$CMDS;;tick sprint 200"
  CMDS="$CMDS;;execute if entity @e[type=minecraft:enderman,tag=carrier,$CARRYING] run tellraw @s \"ENDERMANCARRIES\""
done
# It is still there and still holding what it took: nothing quietly killed the
# subject between the checkpoints above.
CMDS="$CMDS;;execute if entity @e[type=minecraft:enderman,tag=carrier] run tellraw @s \"ENDERMANSURVIVED\""

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=3 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what the server said ==="
grep "server says" join.log | grep -oE "ENDERMANUP|RINGDOWN|ENDERMANPINNED|ENDERMANCARRIES|ENDERMANSURVIVED"
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "\[Error\]|panic|Unknown|Incorrect" | tail -8

# Only the server's own reply counts: join.py echoes the commands it sends.
said() { grep "server says" join.log | grep -q "$1"; }
fail() { echo "########## ENDERMAN BLOCK TEST FAILED ($1) ##########"; exit 1; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }

said ENDERMANUP     || fail "the enderman never spawned"
said RINGDOWN       || fail "the ring of grass never got placed"
said ENDERMANPINNED || fail "the summon NBT never reached the mob"

said ENDERMANSURVIVED || fail "the enderman did not live through the run"
said ENDERMANCARRIES  || fail "the enderman never picked a block up"
echo "########## ENDERMAN BLOCK TEST PASSED ##########"
