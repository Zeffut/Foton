#!/bin/bash
# Roll a TNT minecart onto a powered activator rail and let it go off.
#
# Two halves of this already existed and had never met: the rail machinery
# every cart runs on, and the explosion code TNT and creepers use. What the
# test proves is that they now meet -- a cart on a live activator rail lights
# its fuse and, eighty ticks later, takes the blocks around it with it.
#
# Usage: bash dev/tnt-minecart-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25596
RUN_DIR="$ROOT/run-tntcart"

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
# A live activator rail. Setting `powered=true` by hand is not enough: a rail
# recomputes that from the redstone around it, so the block under it has to be
# a real source.
CMDS="$CMDS;;setblock 0 99 0 minecraft:redstone_block"
CMDS="$CMDS;;setblock 0 100 0 minecraft:activator_rail"
CMDS="$CMDS;;execute if block 0 100 0 minecraft:activator_rail[powered=true] run tellraw @s \"RAILISLIVE\""
# The witness: a block just far enough from the rail to survive being stood
# next to, and near enough that a four-power blast takes it.
CMDS="$CMDS;;setblock 2 100 0 minecraft:dirt"
CMDS="$CMDS;;execute if block 2 100 0 minecraft:dirt run tellraw @s \"WITNESSPLACED\""

# Out of the blast, since a creative player is still thrown by it.
CMDS="$CMDS;;teleport @s 14 100 14"
CMDS="$CMDS;;summon tnt_minecart 0.5 100.0 0.5"
# The fuse is eighty ticks, so four seconds plus room for the tick to land.
CMDS="$CMDS;;!wait 6"
CMDS="$CMDS;;execute if block 2 100 0 minecraft:air run tellraw @s \"WITNESSBLOWNUP\""
# Vanilla spares the track: a cart that took its own rails with it could
# only ever be used once, so this is the half of the blast that must NOT
# happen.
CMDS="$CMDS;;execute if block 0 100 0 minecraft:activator_rail run tellraw @s \"RAILSURVIVED\""

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=2 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what happened ==="
grep "server says" join.log | grep -oE "WITNESSPLACED|RAILISLIVE|WITNESSBLOWNUP|RAILSURVIVED"
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic" | tail -5

fail() { echo "########## TNT MINECART TEST FAILED ($1) ##########"; exit 1; }
said() { grep "server says" join.log | grep -q "$1"; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }
said WITNESSPLACED  || fail "the witness block was never placed"
said RAILISLIVE     || fail "the activator rail never came on"
said WITNESSBLOWNUP || fail "the cart never went off"
said RAILSURVIVED   || fail "the blast took the rail with it"
echo "########## TNT MINECART TEST PASSED ##########"
