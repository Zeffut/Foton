#!/bin/bash
# Hang a horse and a llama off the same ghast and watch only one of them move.
#
# A quad leash is four ropes tied to the four corners of both ends, not one rope
# tied to both centers. Corners reach further than centers, so there is a band
# of distances where the four-rope pull is taut and the one-rope pull is still
# slack -- and that band is the only place the difference is visible from
# outside the server. Five and a half blocks from a ghast is inside it: the
# ghast's corners sit two blocks out, the horse's seven tenths of a block out,
# and the two far ropes come to seven blocks against a six-block slack.
#
# So the horse gets dragged in and the llama does not. Vanilla's llama is the
# one leashable in the abstract-horse family that answers `supportQuadLeash`
# with false, which puts it back on the single center rope at the same distance
# from the same holder. Nothing else about the two mobs differs here.
#
# Everything is frozen before anything is placed. A ghast drifts on its move
# control, a horse strolls, and either one moving half a block would spoil a
# geometry that only has a block of room in it. The interactions still land
# while frozen -- packets are not ticks -- and `tick step 8` is what lets the
# leash pull exactly eight times.
#
# The player is parked twenty blocks away before the step for one reason: if
# the transfer to the ghast had silently failed, the two mobs would still be on
# the player's own lead, and a lead that long snaps on the first tick instead of
# pulling. A horse that moves has to have moved because of the ghast.
#
# Usage: bash dev/leash-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25607
RUN_DIR="$ROOT/run-leash"

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
# Not peaceful: peaceful deletes the ghast. Not hard either -- a ghast that got
# a fireball away would move the holder this whole test measures from.
CMDS="$CMDS;;difficulty easy"
CMDS="$CMDS;;gamerule mobGriefing false"

# A strip of floor under the two mobs and the two places the player stands,
# laid one block at a time because Steel has no `/fill` yet. The horse ends up
# about a block and a half nearer the ghast than it started, so the strip has to
# be long enough to catch it there too.
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
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;teleport @s 4.0 100.0 0.0"

# Frozen before anything is summoned. The one tick after the summons is what
# tells the client the three entities exist -- `!useentity` needs the id the
# client was told -- and it is the only tick any of them gets before the
# measured eight. A mob summoned into a running world arrives with a stride
# already under way, and the velocity it carries into the freeze survives the
# teleport and shows up as drift in the eight ticks that matter.
CMDS="$CMDS;;tick freeze"
CMDS="$CMDS;;summon minecraft:ghast 0.0 100.0 0.0"
CMDS="$CMDS;;summon minecraft:horse -5.5 100.0 0.0"
CMDS="$CMDS;;summon minecraft:llama 5.5 100.0 0.0"
CMDS="$CMDS;;tick step 1"

# Now put all three exactly where the geometry needs them. Decimal coordinates
# throughout: a whole number is center-corrected to the middle of its block.
CMDS="$CMDS;;teleport @e[type=minecraft:ghast] 0.0 100.0 0.0 0.0 0.0"
CMDS="$CMDS;;teleport @e[type=minecraft:horse] -5.5 100.0 0.0 0.0 0.0"
CMDS="$CMDS;;teleport @e[type=minecraft:llama] 5.5 100.0 0.0 0.0 0.0"
CMDS="$CMDS;;execute if entity @e[type=minecraft:ghast] run tellraw @s \"GHASTISHERE\""
CMDS="$CMDS;;execute positioned 0.0 100.0 0.0 if entity @e[type=minecraft:horse,distance=5.4..5.6] run tellraw @s \"HORSESTARTSATFIVEANDAHALF\""
CMDS="$CMDS;;execute positioned 0.0 100.0 0.0 if entity @e[type=minecraft:llama,distance=5.4..5.6] run tellraw @s \"LLAMASTARTSATFIVEANDAHALF\""

# Lead in hand, one mob at a time, then a sneak at the ghast to hand both over.
CMDS="$CMDS;;teleport @s -4.0 100.0 0.0"
CMDS="$CMDS;;!useentity horse"
CMDS="$CMDS;;teleport @s 4.0 100.0 0.0"
CMDS="$CMDS;;!useentity llama"
CMDS="$CMDS;;!sneakuse ghast"

CMDS="$CMDS;;teleport @s 0.0 100.0 -20.0"
CMDS="$CMDS;;teleport @e[type=minecraft:horse] -5.5 100.0 0.0 0.0 0.0"
CMDS="$CMDS;;teleport @e[type=minecraft:llama] 5.5 100.0 0.0 0.0 0.0"
CMDS="$CMDS;;tick step 8"

CMDS="$CMDS;;execute positioned 0.0 100.0 0.0 if entity @e[type=minecraft:horse,distance=..4.9] run tellraw @s \"FOURROPESPULLEDTHEHORSEIN\""
CMDS="$CMDS;;execute positioned 0.0 100.0 0.0 if entity @e[type=minecraft:llama,distance=5.2..] run tellraw @s \"ONEROPELEFTTHELLAMAALONE\""

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=3 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what happened ==="
grep "server says" join.log \
  | grep -oE "GHASTISHERE|HORSESTARTSATFIVEANDAHALF|LLAMASTARTSATFIVEANDAHALF|FOURROPESPULLEDTHEHORSEIN|ONEROPELEFTTHELLAMAALONE"
grep -E "right-clicked the (horse|llama|ghast)" join.log
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic" | tail -5

fail() { echo "########## LEASH TEST FAILED ($1) ##########"; exit 1; }
# `server says` first: join.py echoes the command being run, so grepping the
# bare marker would match the question as well as the answer.
said() { grep "server says" join.log | grep -q "$1"; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }
grep -q "right-clicked the horse" join.log || fail "the lead never reached the horse"
grep -q "right-clicked the llama" join.log || fail "the lead never reached the llama"
grep -q "sneak-right-clicked the ghast" join.log || fail "the sneak never reached the ghast"

said GHASTISHERE               || fail "no ghast was summoned"
said HORSESTARTSATFIVEANDAHALF || fail "the horse did not start five and a half blocks from the ghast"
said LLAMASTARTSATFIVEANDAHALF || fail "the llama did not start five and a half blocks from the ghast"
said FOURROPESPULLEDTHEHORSEIN || fail "eight ticks on four ropes left the horse where it was"
said ONEROPELEFTTHELLAMAALONE  || fail "the llama moved, so it was not on the single center rope"
echo "########## LEASH TEST PASSED ##########"
