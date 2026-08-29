#!/bin/bash
# Put a record in a jukebox and check everything a jukebox is supposed to do.
#
# A jukebox answers redstone two different ways at once: a full signal while
# the music runs, and a per-disc number to a comparator. Getting one and not
# the other is easy, so both are checked, along with the disc going in, the
# model changing, and the disc coming back out.
#
# Usage: bash dev/jukebox-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25587
RUN_DIR="$ROOT/run-jukebox"

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

# A lamp beside the jukebox reads the playing signal, and dust on the far side
# reads it too. `music_disc_13` has comparator output 1 in vanilla, which is
# what makes the comparator answer different from the lamp's.
CMDS='gamemode creative'
# One throwaway command first: the very first `setblock` of a run can land
# before the chunk around the player is ready, and then the floor under the
# jukebox is never built and the ejected disc falls out of the world.
CMDS="$CMDS;;time set day"
CMDS="$CMDS;;setblock 0 99 0 minecraft:stone"
CMDS="$CMDS;;setblock 0 100 0 minecraft:jukebox"
CMDS="$CMDS;;setblock 1 100 0 minecraft:redstone_lamp"
# A comparator reading the jukebox and feeding dust. Its `facing` names the
# side it reads from, not the side it feeds, so a comparator north of the
# jukebox faces south to read it. This is the jukebox's other answer: the lamp
# only knows whether music is playing, while the comparator says which record
# it is. `music_disc_13` reads 1.
CMDS="$CMDS;;setblock 0 99 -1 minecraft:stone"
CMDS="$CMDS;;setblock 0 99 -2 minecraft:stone"
CMDS="$CMDS;;setblock 0 100 -1 minecraft:comparator[facing=south]"
CMDS="$CMDS;;setblock 0 100 -2 minecraft:redstone_wire"
CMDS="$CMDS;;teleport @s 2 100 0"

CMDS="$CMDS;;execute if block 0 100 0 minecraft:jukebox[has_record=false] run tellraw @s \"STARTSEMPTY\""
CMDS="$CMDS;;execute if block 1 100 0 minecraft:redstone_lamp[lit=false] run tellraw @s \"LAMPSTARTSOFF\""
CMDS="$CMDS;;execute if block 0 100 -2 minecraft:redstone_wire[power=0] run tellraw @s \"DUSTSTARTSDARK\""

CMDS="$CMDS;;clear @s"
CMDS="$CMDS;;give @s minecraft:music_disc_13"
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;!useon 0 100 0 up"

CMDS="$CMDS;;execute if block 0 100 0 minecraft:jukebox[has_record=true] run tellraw @s \"DISCWENTIN\""
CMDS="$CMDS;;execute if block 1 100 0 minecraft:redstone_lamp[lit=true] run tellraw @s \"LAMPCAMEON\""
CMDS="$CMDS;;execute if block 0 100 -2 minecraft:redstone_wire[power=1] run tellraw @s \"COMPARATORREADSONE\""


# Taking it back out has to stop the music and the signal with it.
CMDS="$CMDS;;!useon 0 100 0 up"
CMDS="$CMDS;;execute if block 0 100 0 minecraft:jukebox[has_record=false] run tellraw @s \"DISCCAMEOUT\""
CMDS="$CMDS;;execute if block 1 100 0 minecraft:redstone_lamp[lit=false] run tellraw @s \"LAMPWENTOFF\""
# The ejected disc becomes an item entity. Where it ends up is not asserted:
# it leaves with a random nudge and rolls, so a radius would be asserting the
# item physics rather than the jukebox. Nothing else in this test drops
# anything, so the existence of an item entity is specific enough to mean the
# disc came out.
CMDS="$CMDS;;execute if entity @e[type=minecraft:item] run tellraw @s \"DISCBECAMEANITEM\""

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=2 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what happened ==="
grep "server says" join.log \
  | grep -oE "STARTSEMPTY|LAMPSTARTSOFF|DUSTSTARTSDARK|DISCWENTIN|LAMPCAMEON|COMPARATORREADSONE|DISCCAMEOUT|LAMPWENTOFF|DISCBECAMEANITEM"
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic|incorrect" | tail -5

fail() { echo "########## JUKEBOX TEST FAILED ($1) ##########"; exit 1; }
said() { grep "server says" join.log | grep -q "$1"; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }
said STARTSEMPTY       || fail "the jukebox already had a record in it"
said LAMPSTARTSOFF     || fail "the lamp was lit before anything played"
said DISCWENTIN        || fail "the disc did not go in"
said LAMPCAMEON        || fail "a playing jukebox powered nothing"
said DUSTSTARTSDARK    || fail "the comparator read something from an empty jukebox"
said COMPARATORREADSONE || fail "the comparator did not read the disc"
said DISCCAMEOUT       || fail "right-clicking a full jukebox did not eject the disc"
said LAMPWENTOFF       || fail "the jukebox kept powering redstone after the disc came out"
said DISCBECAMEANITEM  || fail "the ejected disc did not become an item"
echo "########## JUKEBOX TEST PASSED ##########"
