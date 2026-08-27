#!/bin/bash
# What holds scaffolding up, and what happens the moment it stops.
#
# Every scaffolding block carries a `distance`: how far it is from something
# solid. Nothing in the world ever wrote it, so every tower stood on the
# default seven forever and nothing ever came down. The three things that
# number decides are all here.
#
# A tower of five is built on stone and an arm of three reaches out sideways
# from the top of it. Stacking is free and reaching sideways is not, so the top
# of the tower must read zero and the far end of the arm must read three -- a
# number nothing but the stability tick can have written, since a `/setblock`
# leaves seven behind.
#
# Then the stone under the tower is taken away. The whole thing has to fall in,
# arm and all, and drop itself as items: a block that had a support and lost it
# is destroyed where it stands, and that is what tells the block above to check
# itself.
#
# The last one is the other half of the same branch. A single block set down in
# open air never had a support to lose, so it does not break -- it falls, as a
# falling block, and lands eleven blocks lower on the stone below. Finding
# scaffolding at the bottom is the proof: a block that had merely been deleted
# would have left an item there instead.
#
# Everything is built within a chunk or two of the player: only the nine chunks
# around them are loaded, and `setblock` outside that fails without stopping the
# script.
#
# Usage: bash dev/scaffolding-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25635
RUN_DIR="$ROOT/run-scaffolding"

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
sed -i 's/^command_spam_threshold_seconds = .*/command_spam_threshold_seconds = 0/' \
  "$RUN_DIR/config/config.toml"
grep -q '^command_spam_threshold_seconds' "$RUN_DIR/config/config.toml" ||
  echo 'command_spam_threshold_seconds = 0' >> "$RUN_DIR/config/config.toml"

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

add() { CMDS="$CMDS;;$1"; }

CMDS='gamemode creative'
add "time set 6000"
add "setblock 0 99 8 minecraft:stone"
add "teleport @s 0 100 8"
# The teleport crosses a chunk border and only the nine chunks around the
# player are loaded. `setblock` into an unloaded chunk fails quietly and every
# `execute if block` below would then be answered by nothing.
add "!wait 2"

# --- The tower ---
add "setblock 0 99 0 minecraft:stone"
for y in 100 101 102 103 104; do
  add "setblock 0 $y 0 minecraft:scaffolding"
done
# A `/setblock` writes the default state, which is distance seven. Only the
# stability tick can turn the top of a five-block tower into a zero.
add "execute if block 0 104 0 minecraft:scaffolding[distance=0] run tellraw @s \"TOWERSETTLEDATZERO\""

# --- The arm ---
for x in 1 2 3; do
  add "setblock $x 104 0 minecraft:scaffolding"
done
add "execute if block 1 104 0 minecraft:scaffolding[distance=1] run tellraw @s \"ARMSTARTEDATONE\""
add "execute if block 3 104 0 minecraft:scaffolding[distance=3] run tellraw @s \"ARMREACHEDTHREE\""
# A block hanging off the side has a gap under it, and grows the rim a player
# stands on. The tower does not.
add "execute if block 3 104 0 minecraft:scaffolding[bottom=true] run tellraw @s \"ARMGREWITSRIM\""
add "execute if block 0 102 0 minecraft:scaffolding[bottom=false] run tellraw @s \"TOWERHASNORIM\""

# --- The collapse ---
add "setblock 0 99 0 minecraft:air"
add "!wait 4"
add "execute if block 0 100 0 minecraft:air run tellraw @s \"TOWERFOOTWENT\""
add "execute if block 0 104 0 minecraft:air run tellraw @s \"TOWERTOPWENT\""
add "execute if block 3 104 0 minecraft:air run tellraw @s \"ARMWENTTOO\""
# A collapsing tower is destroyed where it stands, so it leaves items. That it
# is not the falling branch cannot be asked here -- a generated world makes
# falling blocks of its own, and an `unless entity` over the whole world says
# more about the terrain than about scaffolding. The unit test
# `scaffolding_that_loses_its_support_is_destroyed_and_drops` pins that branch
# in an empty world, where the count means something.
add "execute if entity @e[type=minecraft:item,nbt={Item:{id:\"minecraft:scaffolding\"}}] run tellraw @s \"COLLAPSEDROPPEDITSELF\""

# --- The one that never had a support ---
add "setblock 8 99 0 minecraft:stone"
add "setblock 8 110 0 minecraft:scaffolding"
add "execute if block 8 110 0 minecraft:air run tellraw @s \"LONEBLOCKLEFTITSPOT\""
add "!wait 4"
add "execute if block 8 100 0 minecraft:scaffolding run tellraw @s \"LONEBLOCKLANDEDWHOLE\""

export JOIN_COMMANDS="$CMDS"
JOIN_COMMAND_SETTLE_SECONDS=0.5 JOIN_WATCH_SECONDS=2 \
  python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what the server answered ==="
grep "server says" join.log | grep -vE "setblock|teleport|gamemode|summon|time" | tail -16
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic|incorrect|unknown" | tail -6

if [ $STATUS -ne 0 ]; then
  echo "########## SCAFFOLDING TEST FAILED (the client never settled) ##########"
  tail -20 join.log
  exit 1
fi
for marker in TOWERSETTLEDATZERO ARMSTARTEDATONE ARMREACHEDTHREE ARMGREWITSRIM \
              TOWERHASNORIM TOWERFOOTWENT TOWERTOPWENT ARMWENTTOO \
              COLLAPSEDROPPEDITSELF LONEBLOCKLEFTITSPOT \
              LONEBLOCKLANDEDWHOLE; do
  # Only the server's own reply counts: join.py echoes the commands it sends.
  if ! grep "server says" join.log | grep -q "$marker"; then
    echo "########## SCAFFOLDING TEST FAILED ($marker missing) ##########"
    exit 1
  fi
done
echo "########## SCAFFOLDING TEST PASSED ##########"
