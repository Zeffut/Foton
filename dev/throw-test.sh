#!/bin/bash
# Throw a snowball and an egg, and hatch a chicken.
#
# Both entities and both items were missing, so this is first contact: a
# snowball that flies and breaks, an egg that does the same and hatches one
# throw in eight.
#
# Usage: bash dev/throw-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25591
RUN_DIR="$ROOT/run-throw"

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
# One throwaway command first: the very first `setblock` of a run can land
# before the chunk around the player is ready.
CMDS="$CMDS;;time set day"
for x in -2 -1 0 1 2; do
  for z in -2 -1 0 1 2; do
    CMDS="$CMDS;;setblock $x 99 $z minecraft:stone"
  done
done
CMDS="$CMDS;;teleport @s 0 100 0"
# Chickens spawn on their own, which would make both the "no chicken yet"
# control and the hatch assertion meaningless -- a natural spawn reads
# exactly like a hatched chick.
CMDS="$CMDS;;gamerule doMobSpawning false"
CMDS="$CMDS;;kill @e[type=minecraft:chicken]"

CMDS="$CMDS;;execute unless entity @e[type=minecraft:snowball] run tellraw @s \"NOSNOWBALLYET\""
CMDS="$CMDS;;execute unless entity @e[type=minecraft:chicken] run tellraw @s \"NOCHICKENYET\""

# Snowballs thrown at the ground. Asking the server whether one exists is a
# race -- a snowball is gone within a tick of landing, and one thrown upward
# drifts off the small platform and falls forever instead. So the two questions
# are asked of two different places: the client was told a snowball spawned,
# and the server has none left..
CMDS="$CMDS;;clear @s"
CMDS="$CMDS;;give @s minecraft:snowball 16"
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;!useitemx 8 0 80"
CMDS="$CMDS;;!spawned snowball"

# They all break on landing rather than lying around.
for _ in 1 2 3 4 5; do
  CMDS="$CMDS;;time set day"
done
CMDS="$CMDS;;execute unless entity @e[type=minecraft:snowball] run tellraw @s \"SNOWBALLSBROKE\""


# Forty eggs. A single egg hatches one throw in eight, so a run that hatches
# nothing has odds of about one in two hundred -- that is the flake rate of
# this assertion, and it is the price of testing a random branch at all.
CMDS="$CMDS;;clear @s"
CMDS="$CMDS;;give @s minecraft:egg 64"
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;!useitemx 40 0 80"
CMDS="$CMDS;;time set day"
CMDS="$CMDS;;time set day"
CMDS="$CMDS;;execute if entity @e[type=minecraft:chicken] run tellraw @s \"ACHICKENHATCHED\""

# A bottle o' enchanting breaks into experience wherever it lands.
CMDS="$CMDS;;clear @s"
CMDS="$CMDS;;kill @e[type=minecraft:experience_orb]"
CMDS="$CMDS;;execute unless entity @e[type=minecraft:experience_orb] run tellraw @s \"NOORBSYET\""
CMDS="$CMDS;;give @s minecraft:experience_bottle 16"
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;!useitemx 4 0 80"
CMDS="$CMDS;;time set day"
# Asked of the client, not the server: an orb is drawn to a nearby player and
# swallowed within a tick or two, so "is one lying there" is a race even though
# "was one ever made" is not.
CMDS="$CMDS;;!spawned experience_orb"

# A bow. Drawing and loosing are two packets: `!useitem` bends the string and
# `!releaseuse` lets go, and the two-second settle between them is more than
# the twenty ticks a full draw takes. Unlike a snowball an arrow stays where it
# landed for a full minute, so it can be asked about directly.
CMDS="$CMDS;;clear @s"
CMDS="$CMDS;;kill @e[type=minecraft:arrow]"
CMDS="$CMDS;;execute unless entity @e[type=minecraft:arrow] run tellraw @s \"NOARROWYET\""
# A wall to shoot at, five blocks off. It has to land out of arm's reach: a
# creative player walks over an arrow that fell at their feet and pockets it,
# and the question here is whether it stuck, not whether it was collected.
for x in -1 0 1; do
  for y in 100 101 102; do
    CMDS="$CMDS;;setblock $x $y 5 minecraft:stone"
  done
done
CMDS="$CMDS;;give @s minecraft:bow 1"
CMDS="$CMDS;;give @s minecraft:arrow 64"
CMDS="$CMDS;;!hotbar 0"
# Level, facing the wall. `handle_use_item` turns the player to these angles,
# and the bow fires along them when the string is let go.
CMDS="$CMDS;;!useitem 0 0"
CMDS="$CMDS;;!releaseuse"
CMDS="$CMDS;;!spawned arrow"
CMDS="$CMDS;;time set day"
CMDS="$CMDS;;execute if entity @e[type=minecraft:arrow] run tellraw @s \"ANARROWSTUCK\""

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=3 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what happened ==="
grep -E "server says|saw a snowball" join.log \
  | grep -oE "NOSNOWBALLYET|NOCHICKENYET|saw a snowball spawn|SNOWBALLSBROKE|ACHICKENHATCHED|NOORBSYET|NOARROWYET|ANARROWSTUCK|saw a arrow spawn"
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic|incorrect" | tail -5

fail() { echo "########## THROW TEST FAILED ($1) ##########"; exit 1; }
said() { grep "server says" join.log | grep -q "$1"; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }
said NOSNOWBALLYET   || fail "a snowball existed before one was thrown"
said NOCHICKENYET    || fail "a chicken existed before any egg was thrown"
grep -q "the client saw a snowball spawn" join.log \
  || fail "throwing a snowball spawned nothing"
said SNOWBALLSBROKE  || fail "the snowballs never broke; they are still lying about"
said ACHICKENHATCHED || fail "forty eggs hatched nothing"
said NOORBSYET       || fail "experience orbs were lying about before any bottle"
grep -q "the client saw a experience_orb spawn" join.log \
  || fail "the bottles broke into no experience"
said NOARROWYET      || fail "arrows were lying about before the bow was drawn"
grep -q "the client saw a arrow spawn" join.log \
  || fail "drawing and loosing a bow fired nothing"
said ANARROWSTUCK    || fail "the arrow did not stick where it landed"
echo "########## THROW TEST PASSED ##########"
