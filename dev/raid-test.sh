#!/bin/bash
# Run a raid on a real village, end to end, and watch the village answer it.
#
# A raid is the one event that crosses every system at once, and none of the
# crossings can be seen from a unit test on a hand-built world: the bed has to
# become a point of interest when it is placed, a villager has to claim it, the
# claim has to make the section a village, the raid has to find ground outside
# that village to drop a wave on, and the wave has to be entities the client is
# told about. This drives all of it through a real client.
#
# Three things are asked in order, because each needs the world in a different
# state:
#
# 1. A rung bell gives away the raiders standing in it. That is the bell's
#    block entity -- the swing counter, the sweep of everything within
#    forty-eight blocks, the two seconds of resonance -- and it is asked first,
#    with a witch and a redstone block, because it needs no raid and no
#    villager. A witch rather than a pillager for two reasons: nothing on
#    `VillagerHostilesSensor`'s list is a witch, so it cannot frighten the
#    villager summoned afterwards, and the wave this test later waits for is
#    pillagers, so `!spawned pillager` still means the wave.
# 2. A raid over the village pulls the villagers out of their evening and runs
#    them to the bell. The clock is set to the REST stretch and the bell is put
#    ten blocks the other side of the villager from its bed, so walking to the
#    bell is something only the PRE_RAID package would do -- a villager left
#    alone at that hour walks to its bed instead.
# 3. The wave itself arrives.
#
# The village is one bed and one villager. That is genuinely all vanilla asks
# for: `PoiManager.isVillageCenter` wants a single occupied POI tagged
# `#minecraft:village`, and a bed holds exactly one ticket, so one sleeper is a
# village.
#
# `/raid start` is used rather than a Bad Omen because the omen path adds a
# thirty-second wait for no extra coverage -- it is the same
# `Raids.createOrExtendRaid` either way, and the effect half is unit-tested.
#
# Usage: bash dev/raid-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25604
RUN_DIR="$ROOT/run-raid"

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

# Where the glowing check reads a raider's effects from. Nothing else in this
# run hands out Glowing, so a match can only be the bell's resonance.
GLOWING='{active_effects:[{id:"minecraft:glowing"}]}'

# The reveal a bell hands out lasts three seconds, and the client's default
# two-second settle between commands is too coarse to ask inside a window that
# short. Every wait this test actually depends on is written out below.
export JOIN_COMMAND_SETTLE_SECONDS=0.5

CMDS='gamemode creative'
# A raid stops itself on peaceful. Hard rather than normal only because the
# config already says normal, and the command answers with a failure line when
# it is asked for what is already set.
CMDS="$CMDS;;difficulty hard"
# The evening is what makes the walk to the bell mean something, and a raid
# ignores the spawning rule -- so natural spawns can be switched off outright
# rather than kept away with daylight, which also keeps a wandering zombie from
# frightening the villager into the PANIC package.
CMDS="$CMDS;;gamerule spawn_mobs false"
CMDS="$CMDS;;time set 13000"
# One throwaway command first: the very first `setblock` of a run can land
# before the chunk around the player is ready.
CMDS="$CMDS;;teleport @s 1 100 0"
CMDS="$CMDS;;!wait 3"

# --- the ground ----------------------------------------------------------
# A floor from the bed at x=0 out past the bell at x=10, wide enough to walk.
for x in -1 0 1 2 3 4 5 6 7 8 9 10 11 12 13; do
  for z in -1 0 1 2; do
    CMDS="$CMDS;;setblock $x 99 $z minecraft:stone"
  done
done
CMDS="$CMDS;;setblock 10 100 0 minecraft:bell"

# --- 1: the bell gives the raiders away -----------------------------------
CMDS="$CMDS;;summon minecraft:witch 12 100 0"
CMDS="$CMDS;;execute if entity @e[type=minecraft:witch] run tellraw @s \"WITCHSTANDING\""
CMDS="$CMDS;;execute unless entity @e[type=minecraft:witch,nbt=$GLOWING] run tellraw @s \"RAIDERNOTLITYET\""
# Redstone is how a script pulls a bell rope: `neighborChanged` rings once on
# the rising edge, which is the same `attemptToRing` a player's click takes.
CMDS="$CMDS;;setblock 10 100 1 minecraft:redstone_block"
# Five ticks before the resonance may start and forty of it, so the reveal
# lands about two seconds after the ring and the Glowing it hands out is gone
# three seconds later. The question is asked repeatedly across that window
# rather than once at a guessed moment: a single question timed against a
# server under load is a test that passes on an idle box and lies on a busy one.
CMDS="$CMDS;;!wait 2"
for _ in 1 2 3 4 5 6 7 8; do
  CMDS="$CMDS;;execute if entity @e[type=minecraft:witch,nbt=$GLOWING] run tellraw @s \"RAIDERREVEALED\""
done
CMDS="$CMDS;;kill @e[type=minecraft:witch]"

# --- 2: the village answers the raid --------------------------------------
# Only the head half is a `minecraft:home` point of interest; the foot is
# there so the bed looks like one to a player watching.
CMDS="$CMDS;;setblock 0 100 1 minecraft:red_bed[facing=north,part=foot]"
CMDS="$CMDS;;setblock 0 100 0 minecraft:red_bed[facing=north,part=head]"
CMDS="$CMDS;;summon minecraft:villager 1 100 1"
CMDS="$CMDS;;teleport @s 1 100 0"
# Both claims are booked on a jittered scan, and the bell is ten blocks off, so
# give the villager a good while to take them and settle into its evening.
CMDS="$CMDS;;!wait 20"

# The villager has to be in the world at all for its bed claim to mean
# anything, so that is asserted before the raid is blamed for anything.
CMDS="$CMDS;;execute if entity @e[type=minecraft:villager] run tellraw @s \"VILLAGERSTANDING\""
# And it has to start away from the bell, or walking to it would prove nothing.
CMDS="$CMDS;;execute unless entity @e[type=minecraft:villager,x=10,y=100,z=0,distance=..4] run tellraw @s \"VILLAGERAWAYFROMBELL\""

CMDS="$CMDS;;raid start 1"
# The countdown is three hundred ticks; the villager has ten blocks to cover at
# one and a half times its usual pace, so it should be at the bell well before
# the wave lands.
CMDS="$CMDS;;!wait 10"
CMDS="$CMDS;;execute if entity @e[type=minecraft:villager,x=10,y=100,z=0,distance=..4] run tellraw @s \"VILLAGERATBELL\""

# --- 3: the wave ----------------------------------------------------------
# The rest of the countdown, plus room for the spawn-position search and for
# the mobs to be streamed to the client.
CMDS="$CMDS;;!wait 15"
CMDS="$CMDS;;execute if entity @e[type=minecraft:pillager] run tellraw @s \"RAIDWAVEARRIVED\""
CMDS="$CMDS;;!spawned pillager"

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=4 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what happened ==="
grep -E "server says|the client saw|no .* has spawned|spawned around the player" join.log
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic" | tail -5

fail() { echo "########## RAID TEST FAILED ($1) ##########"; exit 1; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }

# Each marker has to arrive as chat. Grepping the whole log would also match
# the echo of the command that asks the question, which is printed whether the
# answer was yes or no.
said() { grep -q "server says: $1" join.log; }

said WITCHSTANDING \
  || fail "the witch never spawned, so nothing below is about the bell"
said RAIDERNOTLITYET \
  || fail "the witch was already glowing, or the effect query matches everything"
said RAIDERREVEALED \
  || fail "the bell was rung over a raider and never gave it away"

said VILLAGERSTANDING \
  || fail "the villager never spawned, so nothing could have claimed the bed"
said VILLAGERAWAYFROMBELL \
  || fail "the villager was already at the bell before the raid started"
# The one assertion read off a command's own output rather than a marker.
# `note_system_chat` drops every word shorter than four letters and prints the
# NBT length byte glued to the first word, so the "server says:" anchor and the
# articles are gone; what is left still separates the success line from the
# failure line, which reads "Failed to create a raid ...".
grep -q "Created raid your local village" join.log \
  || fail "no raid started: the bed was never claimed, so the section is not a village"
said VILLAGERATBELL \
  || fail "the raid never pulled the villager out of its evening and up to the bell"
said RAIDWAVEARRIVED \
  || fail "the countdown ran out without a wave being spawned"
grep -q "the client saw a pillager spawn" join.log \
  || fail "the wave never reached the client"

echo "########## RAID TEST PASSED ##########"
