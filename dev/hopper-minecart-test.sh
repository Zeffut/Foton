#!/bin/bash
# Park a hopper minecart under a loaded chest and watch it empty the chest.
#
# Nothing reads a cart's contents, but a comparator reads the chest above it,
# so the assertion runs the other way round: the chest goes from lit to dark
# because the cart underneath sucked it dry. The chest is loaded by the player
# shift-clicking into it, since no command can put items in a container.
#
# The second rig is the control, and it is the whole point of this cart: on a
# hopper minecart a powered activator rail switches the sucking *off*, the
# opposite of every other cart. Its chest must still be full at the end.
#
# Usage: bash dev/hopper-minecart-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25598
RUN_DIR="$ROOT/run-hoppercart"

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
# Two identical rigs, four blocks apart: an ordinary rail at x=0, a live
# activator rail at x=4.
for x in 0 4; do
  CMDS="$CMDS;;setblock $x 99 0 minecraft:stone"
  CMDS="$CMDS;;setblock $((x + 1)) 100 0 minecraft:stone"
  CMDS="$CMDS;;setblock $x 100 1 minecraft:stone"
  CMDS="$CMDS;;setblock $x 100 2 minecraft:stone"
  CMDS="$CMDS;;setblock $x 101 0 minecraft:chest"
  # The comparator reads the chest. As with the jukebox, its `facing` names
  # the side it reads from, so a comparator south of the chest faces north.
  CMDS="$CMDS;;setblock $x 101 1 minecraft:comparator[facing=north]"
  CMDS="$CMDS;;setblock $x 101 2 minecraft:redstone_wire"
done
CMDS="$CMDS;;setblock 0 100 0 minecraft:rail[shape=north_south]"
# A rail recomputes `powered` from the redstone around it, so the block under
# it has to be a real source.
CMDS="$CMDS;;setblock 4 99 0 minecraft:redstone_block"
CMDS="$CMDS;;setblock 4 100 0 minecraft:activator_rail[shape=north_south]"
CMDS="$CMDS;;execute if block 4 100 0 minecraft:activator_rail[powered=true] run tellraw @s \"RAILISLIVE\""
CMDS="$CMDS;;execute if block 0 101 2 minecraft:redstone_wire[power=0] run tellraw @s \"CHESTSTARTSEMPTY\""

for x in 0 4; do
  CMDS="$CMDS;;clear @s"
  CMDS="$CMDS;;give @s minecraft:coal 16"
  CMDS="$CMDS;;!hotbar 0"
  CMDS="$CMDS;;teleport @s $((x + 1)) 101 0"
  CMDS="$CMDS;;!useon $x 101 0 east"
  # Slot 54 is the first hotbar square: twenty-seven chest slots, then the
  # twenty-seven main inventory squares.
  CMDS="$CMDS;;!shiftclick 54"
  CMDS="$CMDS;;!close"
done
CMDS="$CMDS;;execute if block 0 101 2 minecraft:redstone_wire[power=1] run tellraw @s \"CHESTLOADED\""
CMDS="$CMDS;;execute if block 4 101 2 minecraft:redstone_wire[power=1] run tellraw @s \"CONTROLLOADED\""

# Out of the way, so the player does not pick the coal up if anything spills.
CMDS="$CMDS;;teleport @s 12 101 12"
CMDS="$CMDS;;summon hopper_minecart 0.5 100.0 0.5"
CMDS="$CMDS;;summon hopper_minecart 4.5 100.0 0.5"
# A hopper moves one item every eight ticks, so sixteen coal needs a while.
CMDS="$CMDS;;!wait 9"
CMDS="$CMDS;;execute if block 0 101 2 minecraft:redstone_wire[power=0] run tellraw @s \"CARTDRAINEDIT\""
CMDS="$CMDS;;execute if block 4 101 2 minecraft:redstone_wire[power=1] run tellraw @s \"CONTROLUNTOUCHED\""

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=2 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what happened ==="
grep "server says" join.log \
  | grep -oE "RAILISLIVE|CHESTSTARTSEMPTY|CHESTLOADED|CONTROLLOADED|CARTDRAINEDIT|CONTROLUNTOUCHED"
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic" | tail -5

fail() { echo "########## HOPPER MINECART TEST FAILED ($1) ##########"; exit 1; }
said() { grep "server says" join.log | grep -q "$1"; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }
said CHESTSTARTSEMPTY || fail "the comparator read something from an empty chest"
said CHESTLOADED      || fail "the coal never reached the chest"
said CARTDRAINEDIT    || fail "the cart never emptied the chest above it"
said RAILISLIVE       || fail "the activator rail never came on"
said CONTROLLOADED    || fail "the control chest was never loaded"
said CONTROLUNTOUCHED || fail "a cart on a powered activator rail kept sucking"
echo "########## HOPPER MINECART TEST PASSED ##########"
