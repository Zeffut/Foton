#!/bin/bash
# Run a raid on a real village, end to end.
#
# A raid is the one event that crosses every system at once, and none of the
# crossings can be seen from a unit test on a hand-built world: the bed has to
# become a point of interest when it is placed, a villager has to claim it, the
# claim has to make the section a village, the raid has to find ground outside
# that village to drop a wave on, and the wave has to be entities the client is
# told about. This drives all of it through a real client.
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
# A raid stops itself on peaceful, and daylight keeps the wave from being
# confused with natural night spawns.
CMDS="$CMDS;;difficulty normal"
CMDS="$CMDS;;time set day"
# One throwaway command first: the very first `setblock` of a run can land
# before the chunk around the player is ready.
CMDS="$CMDS;;teleport @s 0 100 0"

# --- the village ---------------------------------------------------------
# A platform to stand the bed and the villager on, then the bed. Only the head
# half is a `minecraft:home` point of interest; the foot is there so the bed
# looks like one to a player watching.
CMDS="$CMDS;;setblock 0 99 0 minecraft:stone"
CMDS="$CMDS;;setblock 0 99 1 minecraft:stone"
CMDS="$CMDS;;setblock 1 99 0 minecraft:stone"
CMDS="$CMDS;;setblock 1 99 1 minecraft:stone"
CMDS="$CMDS;;setblock 0 100 1 minecraft:red_bed[facing=north,part=foot]"
CMDS="$CMDS;;setblock 0 100 0 minecraft:red_bed[facing=north,part=head]"
CMDS="$CMDS;;summon minecraft:villager 1 100 1"
CMDS="$CMDS;;teleport @s 1 100 0"
# The bed claim is booked on a jittered scan, so give it several scans' worth.
CMDS="$CMDS;;!wait 8"

# The villager has to be in the world at all for its bed claim to mean
# anything, so that is asserted before the raid is blamed for anything.
CMDS="$CMDS;;execute if entity @e[type=minecraft:villager] run tellraw @s \"VILLAGERSTANDING\""
CMDS="$CMDS;;raid start 1"

# --- the wave ------------------------------------------------------------
# Three hundred ticks of countdown, plus room for the spawn-position search
# and for the mobs to be streamed to the client.
CMDS="$CMDS;;!wait 20"
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

said VILLAGERSTANDING \
  || fail "the villager never spawned, so nothing could have claimed the bed"
# The one assertion read off a command's own output rather than a marker.
# `note_system_chat` drops every word shorter than four letters and prints the
# NBT length byte glued to the first word, so the "server says:" anchor and the
# articles are gone; what is left still separates the success line from the
# failure line, which reads "Failed to create a raid ...".
grep -q "Created raid your local village" join.log \
  || fail "no raid started: the bed was never claimed, so the section is not a village"
said RAIDWAVEARRIVED \
  || fail "the countdown ran out without a wave being spawned"
grep -q "the client saw a pillager spawn" join.log \
  || fail "the wave never reached the client"

echo "########## RAID TEST PASSED ##########"
