#!/bin/bash
# Hang a horse off a happy ghast, then put a player on it.
#
# Two claims no unit test can make. The first is that a happy ghast really is a
# quad-leash holder in a running world: a horse five and a half blocks out is
# inside the band where the four corner ropes are taut and a single center rope
# is not, so the horse is dragged in and a llama -- the one leashable vanilla
# excludes from quad leashes -- is left where it stands. The second is that the
# mob is reachable at all: summoned, harnessed by a right-click, and ridden. The
# ride is the honest end of it, because a mob that exists in a registry and a
# mob a player can sit on look identical from the outside until somebody clicks.
#
# The leash half runs frozen. A ghast drifts on its move control and a horse
# strolls, and either one moving half a block would spoil a geometry with only a
# block of room in it. The ride half runs unfrozen, because the passenger packet
# the test reads is sent by the entity tracker, and the tracker runs on ticks.
#
# Usage: bash dev/happy-ghast-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25609
RUN_DIR="$ROOT/run-happy-ghast"

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
# One throwaway command first: the very first command of a run can land before
# the chunk around the player is ready.
CMDS="$CMDS;;time set day"
CMDS="$CMDS;;difficulty easy"
CMDS="$CMDS;;gamerule mobGriefing false"

# A strip of floor under the two leashables and the two places the player
# stands, laid one block at a time because Steel has no `/fill` yet.
for x in -8 -7 -6 -5 -4 -3 -2 3 4 5; do
  CMDS="$CMDS;;setblock $x 99 -1 minecraft:stone"
  CMDS="$CMDS;;setblock $x 99 0 minecraft:stone"
done
# and the pad the player is parked on, out of leash range of everything.
for x in -1 0; do
  CMDS="$CMDS;;setblock $x 99 -21 minecraft:stone"
  CMDS="$CMDS;;setblock $x 99 -20 minecraft:stone"
done

CMDS="$CMDS;;give @s minecraft:lead 8"
CMDS="$CMDS;;give @s minecraft:white_harness 1"
CMDS="$CMDS;;teleport @s 4.0 100.0 0.0"

CMDS="$CMDS;;tick freeze"
CMDS="$CMDS;;summon minecraft:happy_ghast 0.0 100.0 0.0"
CMDS="$CMDS;;summon minecraft:horse -5.5 100.0 0.0"
CMDS="$CMDS;;summon minecraft:llama 5.5 100.0 0.0"
# One tick: enough for the client to be told the three exist, which is what
# `!useentity` needs, and not enough for any of them to move.
CMDS="$CMDS;;tick step 1"

# Decimal coordinates throughout: a whole number is center-corrected to the
# middle of its block.
CMDS="$CMDS;;teleport @e[type=minecraft:happy_ghast] 0.0 100.0 0.0 0.0 0.0"
CMDS="$CMDS;;teleport @e[type=minecraft:horse] -5.5 100.0 0.0 0.0 0.0"
CMDS="$CMDS;;teleport @e[type=minecraft:llama] 5.5 100.0 0.0 0.0 0.0"
CMDS="$CMDS;;execute if entity @e[type=minecraft:happy_ghast] run tellraw @s \"HAPPYGHASTISHERE\""
CMDS="$CMDS;;execute positioned 0.0 100.0 0.0 if entity @e[type=minecraft:horse,distance=5.4..5.6] run tellraw @s \"HORSESTARTSATFIVEANDAHALF\""
CMDS="$CMDS;;execute positioned 0.0 100.0 0.0 if entity @e[type=minecraft:llama,distance=5.4..5.6] run tellraw @s \"LLAMASTARTSATFIVEANDAHALF\""

CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;teleport @s -4.0 100.0 0.0"
CMDS="$CMDS;;!useentity horse"
CMDS="$CMDS;;teleport @s 4.0 100.0 0.0"
CMDS="$CMDS;;!useentity llama"
CMDS="$CMDS;;!sneakuse happy_ghast"

CMDS="$CMDS;;teleport @s 0.0 100.0 -20.0"
CMDS="$CMDS;;teleport @e[type=minecraft:horse] -5.5 100.0 0.0 0.0 0.0"
CMDS="$CMDS;;teleport @e[type=minecraft:llama] 5.5 100.0 0.0 0.0 0.0"
CMDS="$CMDS;;tick step 8"

CMDS="$CMDS;;execute positioned 0.0 100.0 0.0 if entity @e[type=minecraft:horse,distance=..4.9] run tellraw @s \"FOURROPESPULLEDTHEHORSEIN\""
CMDS="$CMDS;;execute positioned 0.0 100.0 0.0 if entity @e[type=minecraft:llama,distance=5.2..] run tellraw @s \"ONEROPELEFTTHELLAMAALONE\""

# The riding half, still frozen. Two seconds of running world between the
# teleport and the click is eighty ticks of a ghast drifting on its move
# control, and the first version of this test lost the second click to a mob
# that had floated out of arm's reach. The passenger packet does not need a
# tick: the server sends it to the rider as the mount happens.
CMDS="$CMDS;;teleport @e[type=minecraft:happy_ghast] 0.0 100.0 0.0 0.0 0.0"
CMDS="$CMDS;;teleport @s 3.0 100.0 0.0"
CMDS="$CMDS;;!hotbar 1"
# First click puts the harness on. A happy ghast without one cannot be ridden,
# so a click that boarded here would mean the harness gate is not being read.
CMDS="$CMDS;;!useentity happy_ghast"
CMDS="$CMDS;;!useentity happy_ghast"

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=3 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what happened ==="
grep "server says" join.log \
  | grep -oE "HAPPYGHASTISHERE|HORSESTARTSATFIVEANDAHALF|LLAMASTARTSATFIVEANDAHALF|FOURROPESPULLEDTHEHORSEIN|ONEROPELEFTTHELLAMAALONE"
grep -E "right-clicked the (horse|llama|happy_ghast)|is carrying" join.log
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic" | tail -5

fail() { echo "########## HAPPY GHAST TEST FAILED ($1) ##########"; exit 1; }
# `server says` first: join.py echoes the command being run, so grepping the
# bare marker would match the question as well as the answer.
said() { grep "server says" join.log | grep -q "$1"; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }
said HAPPYGHASTISHERE           || fail "no happy ghast was summoned"
said HORSESTARTSATFIVEANDAHALF  || fail "the horse did not start five and a half blocks out"
said LLAMASTARTSATFIVEANDAHALF  || fail "the llama did not start five and a half blocks out"
grep -q "sneak-right-clicked the happy_ghast" join.log || fail "the sneak never reached the happy ghast"
said FOURROPESPULLEDTHEHORSEIN  || fail "eight ticks on four ropes left the horse where it was"
said ONEROPELEFTTHELLAMAALONE   || fail "the llama moved, so it was not on the single center rope"

# and the ride: this player, on this happy ghast.
player=$(grep -o 'joined the world as entity [0-9]*' join.log | head -1 | awk '{print $NF}')
[ -n "$player" ] || fail "never learned the player entity id"
ghast=$(grep -o '^  right-clicked the happy_ghast (entity [0-9]*' join.log | head -1 | awk '{print $NF}')
[ -n "$ghast" ] || fail "the harness click never reached the happy ghast"
grep -q "entity $ghast is carrying \[$player\]" join.log \
  || fail "the happy ghast (entity $ghast) is not carrying the player (id $player)"
echo "########## HAPPY GHAST TEST PASSED ##########"
