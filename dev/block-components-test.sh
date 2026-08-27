#!/bin/bash
# Carry a block entity's data through an item and back.
#
# This is the whole `collectComponents`/`applyImplicitComponents` loop, driven
# by a real client:
#
#   1. a chest named on an anvil is placed named,
#   2. breaking it drops an item that is still named, and re-placing that item
#      gives the name back -- the loot table's `copy_components` reading the
#      block entity is the only thing that can carry it,
#   3. a chest nobody named stays nameless, so the checks above are not just
#      "we always write a name",
#   4. a beehive item carrying `block_state={honey_level:5}` is placed with
#      that honey level, which is `BlockItem.updateBlockStateFromTag`,
#   5. a plain beehive is placed empty, and
#   6. a banner stamped with a pattern keeps it through place, break and
#      re-place -- the banner item goes through `StandingAndWallBlockItem`,
#      which used to skip the whole step.
#
# `execute if data block <pos> <path>` reads `saveWithFullMetadata`, so the
# assertions ask the live block entity rather than guessing an NBT shape.
#
# Usage: bash dev/block-components-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25612
RUN_DIR="$ROOT/run-block-components"

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
# The anti-spam counter drains one point per game tick, and this test sends a
# long run of commands back to back.
sed -i 's/^command_spam_threshold_seconds = .*/command_spam_threshold_seconds = 0/' "$RUN_DIR/config/config.toml"

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
CMDS="$CMDS;;time set day"
# One throwaway command first: the very first `setblock` of a run can land
# before the chunk around the player is ready.
CMDS="$CMDS;;setblock 0 99 0 minecraft:stone"
CMDS="$CMDS;;setblock 0 99 0 minecraft:stone"
CMDS="$CMDS;;setblock 0 99 2 minecraft:stone"
CMDS="$CMDS;;setblock 0 99 4 minecraft:stone"
CMDS="$CMDS;;setblock 0 99 6 minecraft:stone"
CMDS="$CMDS;;setblock 0 99 8 minecraft:stone"
CMDS="$CMDS;;setblock 0 99 10 minecraft:stone"
CMDS="$CMDS;;setblock 0 99 12 minecraft:stone"
# The platform has to actually be there: every check below places a block on
# top of one of these, and `useon` against air quietly does nothing.
CMDS="$CMDS;;execute if block 0 99 0 minecraft:stone run tellraw @s \"PLATFORMREADY\""

# 1. A named chest is placed named.
CMDS="$CMDS;;clear @s"
CMDS="$CMDS;;give @s minecraft:chest[minecraft:custom_name={\"text\":\"Emeralds\"}]"
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;teleport @s 2 100 0"
CMDS="$CMDS;;!useon 0 99 0 up"
CMDS="$CMDS;;execute if block 0 100 0 minecraft:chest run tellraw @s \"CHESTPLACED\""
CMDS="$CMDS;;execute if data block 0 100 0 CustomName run tellraw @s \"PLACEDCHESTNAMED\""

# 2. Breaking it and putting the drop back down keeps the name.
CMDS="$CMDS;;clear @s"
CMDS="$CMDS;;teleport @s 0 100 0"
CMDS="$CMDS;;setblock 0 100 0 minecraft:air destroy"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;teleport @s 2 100 2"
CMDS="$CMDS;;!useon 0 99 2 up"
CMDS="$CMDS;;execute if block 0 100 2 minecraft:chest run tellraw @s \"DROPPEDCHESTREPLACED\""
CMDS="$CMDS;;execute if data block 0 100 2 CustomName run tellraw @s \"DROPPEDCHESTKEPTNAME\""

# 3. The control: a chest nobody named comes back nameless.
CMDS="$CMDS;;clear @s"
CMDS="$CMDS;;give @s minecraft:chest"
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;teleport @s 2 100 4"
CMDS="$CMDS;;!useon 0 99 4 up"
CMDS="$CMDS;;execute if block 0 100 4 minecraft:chest run tellraw @s \"PLAINCHESTPLACED\""
CMDS="$CMDS;;execute unless data block 0 100 4 CustomName run tellraw @s \"PLAINCHESTUNNAMED\""

# 4. `updateBlockStateFromTag`: the honey level rides back down with the item.
CMDS="$CMDS;;clear @s"
CMDS="$CMDS;;give @s minecraft:beehive[minecraft:block_state={honey_level:\"5\"}]"
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;teleport @s 2 100 6"
CMDS="$CMDS;;!useon 0 99 6 up"
CMDS="$CMDS;;execute if block 0 100 6 minecraft:beehive[honey_level=5] run tellraw @s \"HIVEKEPTHONEY\""

# 5. The control: a plain beehive is placed empty.
CMDS="$CMDS;;clear @s"
CMDS="$CMDS;;give @s minecraft:beehive"
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;teleport @s 2 100 8"
CMDS="$CMDS;;!useon 0 99 8 up"
CMDS="$CMDS;;execute if block 0 100 8 minecraft:beehive[honey_level=0] run tellraw @s \"PLAINHIVEEMPTY\""

# 6. A banner keeps its layers through the whole loop.
CMDS="$CMDS;;clear @s"
CMDS="$CMDS;;give @s minecraft:white_banner[minecraft:banner_patterns=[{pattern:\"minecraft:stripe_top\",color:\"red\"}]]"
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;teleport @s 2 100 10"
CMDS="$CMDS;;!useon 0 99 10 up"
CMDS="$CMDS;;execute if data block 0 100 10 patterns[0] run tellraw @s \"PLACEDBANNERKEPTPATTERN\""
CMDS="$CMDS;;clear @s"
CMDS="$CMDS;;teleport @s 0 100 10"
CMDS="$CMDS;;setblock 0 100 10 minecraft:air destroy"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;teleport @s 2 100 12"
CMDS="$CMDS;;!useon 0 99 12 up"
CMDS="$CMDS;;execute if data block 0 100 12 patterns[0] run tellraw @s \"DROPPEDBANNERKEPTPATTERN\""

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=2 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what happened ==="
grep "server says" join.log | grep -oE "PLATFORMREADY|CHESTPLACED|PLACEDCHESTNAMED|DROPPEDCHESTREPLACED|DROPPEDCHESTKEPTNAME|PLAINCHESTPLACED|PLAINCHESTUNNAMED|HIVEKEPTHONEY|PLAINHIVEEMPTY|PLACEDBANNERKEPTPATTERN|DROPPEDBANNERKEPTPATTERN"
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic" | tail -5

fail() { echo "########## BLOCK COMPONENTS TEST FAILED ($1) ##########"; exit 1; }
said() { grep "server says" join.log | grep -q "$1"; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }
said PLATFORMREADY            || fail "the platform never got laid down"
said CHESTPLACED              || fail "the named chest item never placed a chest"
said PLACEDCHESTNAMED         || fail "a named chest item placed a nameless chest"
said DROPPEDCHESTREPLACED     || fail "the broken chest never came back as a placeable item"
said DROPPEDCHESTKEPTNAME     || fail "the chest lost its name on the way out"
said PLAINCHESTPLACED         || fail "a plain chest item never placed a chest"
said PLAINCHESTUNNAMED        || fail "a chest nobody named came back named"
said HIVEKEPTHONEY            || fail "a hive item carrying honey_level placed an empty hive"
said PLAINHIVEEMPTY           || fail "a plain hive item placed a hive with honey in it"
said PLACEDBANNERKEPTPATTERN  || fail "a stamped banner was placed blank"
said DROPPEDBANNERKEPTPATTERN || fail "the banner lost its layers on the way out"
echo "########## BLOCK COMPONENTS TEST PASSED ##########"
