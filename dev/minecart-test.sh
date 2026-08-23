#!/bin/bash
# Put a minecart on a rail and check it actually rolls.
#
# Rails have worked for a long time and nothing ran on them, so this asks the
# three questions that separate a rolling cart from a decoration: does a
# powered rail push it, does it keep going down the line, and does a detector
# rail notice it pass.
#
# Usage: bash dev/minecart-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25585
RUN_DIR="$ROOT/run-minecart"

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

# A line running east, built the way a player would have to build one. A single
# powered rail is not enough: an empty cart coasts about ten blocks and stops,
# so there is a second one partway along, and the line ends on a detector rail
# against a wall -- the cart comes to rest on it, which keeps the detector
# powered long enough to be a stable thing to assert.
CMDS='gamemode creative'
for x in $(seq -1 16); do
  CMDS="$CMDS;;setblock $x 99 0 minecraft:stone"
done
for x in $(seq 0 15); do
  CMDS="$CMDS;;setblock $x 100 0 minecraft:rail[shape=east_west]"
done
# the two pushes, each over a redstone block
for x in 0 8; do
  CMDS="$CMDS;;setblock $x 99 0 minecraft:redstone_block"
  CMDS="$CMDS;;setblock $x 100 0 minecraft:powered_rail[shape=east_west]"
done
# A powered rail only launches a cart that is already stopped if there is
# something solid on one side to push off. Without this the cart sits on a lit
# rail forever, which is vanilla behavior and not a bug.
CMDS="$CMDS;;setblock -1 100 0 minecraft:stone"
CMDS="$CMDS;;setblock 15 100 0 minecraft:detector_rail[shape=east_west]"
CMDS="$CMDS;;setblock 16 100 0 minecraft:stone"
CMDS="$CMDS;;teleport @s 8 101 3"

# Nothing has moved yet.
CMDS="$CMDS;;execute if block 15 100 0 minecraft:detector_rail[powered=false] run tellraw @s \"DETECTORSTARTSOFF\""
CMDS="$CMDS;;execute if block 0 100 0 minecraft:powered_rail[powered=true] run tellraw @s \"RAILISPOWERED\""
CMDS="$CMDS;;summon minecraft:minecart 0 100 0"
CMDS="$CMDS;;execute if entity @e[type=minecraft:minecart] run tellraw @s \"CARTEXISTS\""

# A cart at full speed crosses this line in about two seconds.
CMDS="$CMDS;;time set day"
CMDS="$CMDS;;time set day"
CMDS="$CMDS;;execute unless entity @e[type=minecraft:minecart,x=0,y=100,z=0,distance=..2] run tellraw @s \"CARTLEFT\""
CMDS="$CMDS;;execute if entity @e[type=minecraft:minecart,x=15,y=100,z=0,distance=..2] run tellraw @s \"CARTREACHEDTHEEND\""
CMDS="$CMDS;;execute if block 15 100 0 minecraft:detector_rail[powered=true] run tellraw @s \"DETECTORSAWIT\""

# A corner, ten blocks north, out of the first line's way. Nothing in the code
# looks for a corner: the cart's speed is reprojected onto whatever line the
# rail under it runs along, every tick, and a curve is just a rail whose two
# ends point different ways. So this is the assertion that says the projection
# is right rather than merely harmless on a straight.
for x in $(seq -1 5); do
  CMDS="$CMDS;;setblock $x 99 -10 minecraft:stone"
done
for z in $(seq -15 -11); do
  CMDS="$CMDS;;setblock 5 99 $z minecraft:stone"
done
for x in $(seq 0 4); do
  CMDS="$CMDS;;setblock $x 100 -10 minecraft:rail[shape=east_west]"
done
CMDS="$CMDS;;setblock 5 100 -10 minecraft:rail[shape=north_west]"
for z in $(seq -15 -11); do
  CMDS="$CMDS;;setblock 5 100 $z minecraft:rail[shape=north_south]"
done
CMDS="$CMDS;;setblock 0 99 -10 minecraft:redstone_block"
CMDS="$CMDS;;setblock 0 100 -10 minecraft:powered_rail[shape=east_west]"
CMDS="$CMDS;;setblock -1 100 -10 minecraft:stone"
CMDS="$CMDS;;summon minecraft:minecart 0 100 -10"
CMDS="$CMDS;;time set day"
CMDS="$CMDS;;time set day"
CMDS="$CMDS;;execute if entity @e[type=minecraft:minecart,x=5,y=100,z=-13,distance=..4] run tellraw @s \"CARTTOOKTHECORNER\""

# A player has no `/summon`, so the item is the only way they get a cart onto a
# rail at all. It also has to refuse anything that is not a rail.
CMDS="$CMDS;;teleport @s 8 100 10"
CMDS="$CMDS;;setblock 8 99 11 minecraft:stone"
CMDS="$CMDS;;setblock 8 100 11 minecraft:rail[shape=east_west]"
CMDS="$CMDS;;setblock 10 100 11 minecraft:stone"
CMDS="$CMDS;;clear @s"
CMDS="$CMDS;;give @s minecraft:minecart"
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;!useon 10 100 11 up"
CMDS="$CMDS;;execute unless entity @e[type=minecraft:minecart,x=10,y=101,z=11,distance=..2] run tellraw @s \"NOCARTONSTONE\""
CMDS="$CMDS;;!useon 8 100 11 up"
CMDS="$CMDS;;execute if entity @e[type=minecraft:minecart,x=8,y=100,z=11,distance=..2] run tellraw @s \"CARTPLACEDFROMITEM\""
# And a cart is something to sit in.
CMDS="$CMDS;;!useentity minecart"
# A chest minecart is not. It rolls the same way and opens instead of seating,
# which is the one thing that separates the two.
CMDS="$CMDS;;setblock 8 99 13 minecraft:stone"
CMDS="$CMDS;;setblock 8 100 13 minecraft:rail[shape=east_west]"
CMDS="$CMDS;;teleport @s 8 100 12"
CMDS="$CMDS;;summon minecraft:chest_minecart 8 100 13"
CMDS="$CMDS;;!useentity chest_minecart"
CMDS="$CMDS;;!close"

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=3 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what happened ==="
grep "server says" join.log | grep -oE "DETECTORSTARTSOFF|RAILISPOWERED|CARTEXISTS|CARTLEFT|CARTREACHEDTHEEND|DETECTORSAWIT|CARTTOOKTHECORNER|NOCARTONSTONE|CARTPLACEDFROMITEM"; grep -E "is carrying|a screen opened" join.log | tail -3
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic|incorrect|unknown" | tail -5

fail() { echo "########## MINECART TEST FAILED ($1) ##########"; exit 1; }
said() { grep "server says" join.log | grep -q "$1"; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }
said DETECTORSTARTSOFF  || fail "the detector rail was already powered"
said CARTEXISTS         || fail "no cart was summoned onto the rail"
said CARTLEFT           || fail "the cart never moved off the powered rail"
said CARTREACHEDTHEEND  || fail "the cart moved but never reached the far end"
said DETECTORSAWIT      || fail "the cart is on the detector rail and it did not notice"
said CARTTOOKTHECORNER  || fail "the cart did not follow the curve round to the north leg"
said NOCARTONSTONE      || fail "a minecart was placed on a plain block"
said CARTPLACEDFROMITEM || fail "the minecart item placed nothing on a rail"
player=$(grep -o 'joined the world as entity [0-9]*' join.log | head -1 | awk '{print $NF}')
[ -n "$player" ] || fail "never learned the player entity id"
grep -q "is carrying \[$player\]" join.log || fail "right-clicking a minecart put nobody in it"
screens=$(grep -c "a screen opened" join.log)
[ "$screens" -eq 1 ] \
  || fail "expected exactly the chest minecart to open a screen, got $screens"
chest_cart=$(grep -o 'right-clicked the chest_minecart (entity [0-9]*' join.log | head -1 | awk '{print $NF}')
[ -n "$chest_cart" ] || fail "the chest minecart was never right-clicked"
! grep -q "entity $chest_cart is carrying \[" join.log \
  || fail "a chest minecart seated the player instead of opening"
echo "########## MINECART TEST PASSED ##########"
