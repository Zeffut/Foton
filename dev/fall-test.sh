#!/bin/bash
# Drop a player and a mob and read back what the fall cost them.
#
# A player is authoritative over their own position: the server never simulates
# their fall, it rebuilds it from the position packets they send. So the only
# honest way to test fall damage is to send that arc -- `!fall` does, one
# packet per tick, claiming to be in the air until the last one. A teleport
# proves nothing: it arrives as a single jump the server does not read as a
# fall.
#
# Every health here is an exact number, and the falls are ordered so that each
# one is read against the one before it. A cushion that stopped cushioning
# would not leave the health where the previous step left it -- a twelve block
# drop costs nine half hearts on stone -- so the two "costs nothing" checks are
# about the cushion and not about the starting health.
#
# Natural regeneration is off on purpose: a full-fed player heals a heart every
# ten ticks, which is faster than the settle between two commands, and every
# reading below would come back at twenty.
#
# Usage: bash dev/fall-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25631
RUN_DIR="$ROOT/run-fall"

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
sed -i 's/^command_spam_threshold_seconds = .*/command_spam_threshold_seconds = 0/' "$RUN_DIR/config/config.toml"
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

# One throwaway command first: the very first `setblock` of a run can land
# before the chunk around the player is ready.
CMDS='gamemode creative'
CMDS="$CMDS;;time set day"
CMDS="$CMDS;;gamerule spawn_mobs false"
CMDS="$CMDS;;gamerule natural_health_regeneration false"

# One landing pad per surface, far enough apart that a fall cannot pick up the
# wrong one, and high enough that the terrain is nowhere near.
pad() { # pad <x> <block>
  for dx in -1 0 1; do
    for dz in -1 0 1; do
      CMDS="$CMDS;;setblock $(( $1 + dx )) 150 $dz $2"
    done
  done
}
pad 0 minecraft:stone
pad 8 minecraft:stone
pad 16 minecraft:hay_block
pad 24 minecraft:slime_block
pad 32 minecraft:stone
pad 40 minecraft:stone
CMDS="$CMDS;;setblock 8 151 0 minecraft:water"
CMDS="$CMDS;;setblock 8 152 0 minecraft:water"

CMDS="$CMDS;;teleport @s 0 151 0"
CMDS="$CMDS;;gamemode survival"

# The fall is sent at the middle of the pad; `/teleport` already centers the
# player, and a fall that drifted half a block would land on the pad's edge.
drop() { # drop <x> <topY>
  CMDS="$CMDS;;teleport @s $1 $2 0"
  CMDS="$CMDS;;!fall $(( $1 )).5 $2 0.5 151"
}

# Eight blocks onto stone: `ceil(8 - 3)` is five half hearts, and a landing
# that hurts also puffs the block underfoot. Nothing but the particle packet
# reports that puff, and anything else drawing within thirty-two blocks lands
# in the same channel, so it is cleared on the command before the drop and
# read on the command after it -- the narrowest window the harness has.
CMDS="$CMDS;;teleport @s 0 159 0"
CMDS="$CMDS;;!forgetparticles"
CMDS="$CMDS;;!fall 0.5 159 0.5 151"
CMDS="$CMDS;;!sawparticle block"
CMDS="$CMDS;;execute if entity @s[nbt={Health:15.0f}] run tellraw @s \"STONECOSTSFIVE\""

# The same eight blocks onto a hay bale, which multiplies the fall by 0.2.
drop 16 159
CMDS="$CMDS;;execute if entity @s[nbt={Health:14.0f}] run tellraw @s \"HAYCOSTSONE\""

# Twelve blocks into water, which would be nine half hearts onto stone.
drop 8 163
CMDS="$CMDS;;execute if entity @s[nbt={Health:14.0f}] run tellraw @s \"WATERCOSTSNOTHING\""

# And twelve onto a slime block, which cancels the fall outright.
drop 24 163
CMDS="$CMDS;;execute if entity @s[nbt={Health:14.0f}] run tellraw @s \"SLIMECOSTSNOTHING\""

# Feather Falling IV is twelve points of protection, which is 48% off: the
# five half hearts of the first fall become 2.6.
CMDS="$CMDS;;clear @s"
CMDS="$CMDS;;give @s minecraft:diamond_boots 1"
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;enchant @s minecraft:feather_falling 4"
CMDS="$CMDS;;!wear 36"
CMDS="$CMDS;;execute if entity @s[nbt={equipment:{feet:{id:\"minecraft:diamond_boots\"}}}] run tellraw @s \"THEBOOTSAREWORN\""
drop 32 159
CMDS="$CMDS;;execute if entity @s[nbt={Health:11.4f}] run tellraw @s \"FEATHERFALLINGCUTSITBYHALF\""

# A mob has no client to send it packets: the server drops it itself. Six
# blocks is three half hearts off a pig's ten. The settle after `/summon` is
# longer than the fall, so the pig has already landed when the first check
# runs -- that one only asks whether there is a pig at all.
CMDS="$CMDS;;summon minecraft:pig 40 157 0"
CMDS="$CMDS;;execute if entity @e[type=minecraft:pig] run tellraw @s \"APIGWASDROPPED\""
CMDS="$CMDS;;execute if entity @e[type=minecraft:pig,nbt={Health:7.0f}] run tellraw @s \"THEPIGTOOKTHREE\""

export JOIN_COMMANDS="$CMDS"
python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what happened ==="
grep -oE "the client saw block particles|no block particles reached the client" join.log
grep "server says" join.log | grep -oE "STONECOSTSFIVE|HAYCOSTSONE|WATERCOSTSNOTHING|SLIMECOSTSNOTHING|THEBOOTSAREWORN|FEATHERFALLINGCUTSITBYHALF|APIGWASDROPPED|THEPIGTOOKTHREE"
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic|wrongly|too quickly" | tail -5

fail() { echo "########## FALL TEST FAILED ($1) ##########"; exit 1; }
said() { grep "server says" join.log | grep -q "$1"; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }
grep -q "the client saw block particles" join.log \
  || fail "a landing hard enough to hurt kicked up no block particles"
said STONECOSTSFIVE || fail "an eight block fall onto stone did not cost five half hearts"
said HAYCOSTSONE || fail "a hay bale did not soften the same fall to one"
said WATERCOSTSNOTHING || fail "a twelve block fall into water hurt"
said SLIMECOSTSNOTHING || fail "a twelve block fall onto a slime block hurt"
said THEBOOTSAREWORN || fail "the enchanted boots never reached the player's feet"
said FEATHERFALLINGCUTSITBYHALF || fail "Feather Falling IV did not take 48% off the fall"
said APIGWASDROPPED || fail "nothing was summoned to drop"
said THEPIGTOOKTHREE || fail "a pig dropped six blocks by the server itself was not hurt"
echo "########## FALL TEST PASSED ##########"
