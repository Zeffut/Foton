#!/bin/bash
# A dispenser with a bucket in it, driven by redstone.
#
# The round trip is the point. One dispenser aimed into a walled hole is loaded
# with a water bucket and pulsed twice. The first pulse has to put a water
# source in the hole; the second has to take it back out again -- and the only
# thing that can take it back is the empty bucket the first pulse left in the
# slot. A dispenser that threw the bucket on the floor instead would leave the
# hole dry after the first pulse and hold nothing for the second.
#
# The hole is walled on five sides so the source stays a source: a water block
# with somewhere to run turns into flowing water, and `execute if block ...
# minecraft:water` would then be answered by a level the empty bucket refuses
# to pick up.
#
# The lava dispenser next door is the same first half with the other fluid, and
# the powder snow one is `SolidBucketItem.emptyContents`, which places a block
# rather than a fluid. The axolotl bucket proves `checkExtraContent` runs: the
# water lands and the fish comes out of it.
#
# The last dispenser is the control. It is loaded with a stone block, which has
# no dispense behavior at all, and its hole must still be empty afterwards --
# otherwise "there is water in the hole" would only mean "the dispenser fired".
#
# Everything is built within a chunk or two of the player: only the nine chunks
# around them are loaded, and `setblock` outside that fails without stopping the
# script.
#
# Usage: bash dev/dispenser-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25633
RUN_DIR="$ROOT/run-dispenser"

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
add "setblock 0 99 6 minecraft:stone"
add "teleport @s 0 100 6"
# The teleport crosses a chunk border and only the nine chunks around the
# player are loaded. `setblock` into an unloaded chunk fails quietly and every
# `execute if block` below would then be answered by nothing.
add "!wait 2"

# One bench per bucket. Each is a dispenser at x 100 0 pointing south into a
# hole at x 100 1 that is closed on its floor, both sides and its far wall.
build_bench() {
  x=$1
  add "setblock $x 99 1 minecraft:stone"
  add "setblock $x 100 2 minecraft:stone"
  add "setblock $((x - 1)) 100 1 minecraft:stone"
  add "setblock $((x + 1)) 100 1 minecraft:stone"
  add "setblock $x 100 1 minecraft:air"
}

# Vanilla's dispenser FACING is the way it points, and `south` is +Z.
load() {
  add "setblock $1 100 0 minecraft:dispenser[facing=south]{Items:[{Slot:0b,id:\"minecraft:$2\",count:1}]}"
}

# A pulse is a redstone block appearing beside the dispenser and going away
# again; the block only fires on the rising edge, so the removal is what makes
# a second pulse possible.
pulse() {
  add "setblock $1 101 0 minecraft:redstone_block"
  add "!wait 1"
  add "setblock $1 101 0 minecraft:air"
  add "!wait 1"
}

for x in 0 4 8 12 16; do
  build_bench $x
done

load 0 water_bucket
load 4 lava_bucket
load 8 powder_snow_bucket
load 12 axolotl_bucket
load 16 stone

add "execute if block 0 100 0 minecraft:dispenser run tellraw @s \"BENCHESBUILT\""
add "execute if block 0 100 1 minecraft:air run tellraw @s \"HOLESTARTEDEMPTY\""

for x in 0 4 8 12 16; do
  pulse $x
done
add "!wait 2"

add "execute if block 0 100 1 minecraft:water run tellraw @s \"WATERPLACED\""
add "execute if block 4 100 1 minecraft:lava run tellraw @s \"LAVAPLACED\""
add "execute if block 8 100 1 minecraft:powder_snow run tellraw @s \"POWDERSNOWPLACED\""
add "execute if block 12 100 1 minecraft:water run tellraw @s \"AXOLOTLWATERPLACED\""
add "execute if entity @e[type=minecraft:axolotl] run tellraw @s \"AXOLOTLCAMEOUT\""
# The control: stone has no dispense behavior, so it is thrown, and its hole
# stays empty. If this marker is missing, "there is water in the hole" above
# meant nothing more than "the block fired".
add "execute if block 16 100 1 minecraft:air run tellraw @s \"CONTROLHOLESTAYEDEMPTY\""
add "execute if entity @e[type=minecraft:item,nbt={Item:{id:\"minecraft:stone\"}}] run tellraw @s \"CONTROLTHREWITSSTONE\""

# The second pulse. Only a dispenser now holding an empty bucket can drain the
# hole it just filled.
#
# "The hole is air" is not evidence on its own -- a hole nothing ever filled is
# air too. What proves the round trip is what falls out of the dispenser when
# it is broken: a water bucket, which it can only be holding if the first pulse
# left an empty one in the slot and the second pulse filled it again.
add "execute unless entity @e[type=minecraft:item,nbt={Item:{id:\"minecraft:water_bucket\"}}] run tellraw @s \"NOLOOSEWATERBUCKET\""
pulse 0
add "!wait 2"
add "execute if block 0 100 1 minecraft:air run tellraw @s \"WATERTAKENBACK\""
add "setblock 0 100 0 minecraft:air destroy"
add "!wait 1"
add "execute if entity @e[type=minecraft:item,nbt={Item:{id:\"minecraft:water_bucket\"}}] run tellraw @s \"BUCKETCAMEBACKFULL\""

export JOIN_COMMANDS="$CMDS"
JOIN_COMMAND_SETTLE_SECONDS=0.5 JOIN_WATCH_SECONDS=2 \
  python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what the server answered ==="
grep "server says" join.log | grep -vE "setblock|teleport|gamemode|summon|time" | tail -14
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic|incorrect|unknown" | tail -6

if [ $STATUS -ne 0 ]; then
  echo "########## DISPENSER TEST FAILED (the client never settled) ##########"
  tail -20 join.log
  exit 1
fi
for marker in BENCHESBUILT HOLESTARTEDEMPTY WATERPLACED LAVAPLACED \
              POWDERSNOWPLACED AXOLOTLWATERPLACED AXOLOTLCAMEOUT \
              CONTROLHOLESTAYEDEMPTY CONTROLTHREWITSSTONE \
              NOLOOSEWATERBUCKET WATERTAKENBACK BUCKETCAMEBACKFULL; do
  # Only the server's own reply counts: join.py echoes the commands it sends.
  if ! grep "server says" join.log | grep -q "$marker"; then
    echo "########## DISPENSER TEST FAILED ($marker missing) ##########"
    exit 1
  fi
done
echo "########## DISPENSER TEST PASSED ##########"
