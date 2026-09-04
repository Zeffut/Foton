#!/bin/bash
# Hang an item frame, fill it, turn it, and read it with a comparator.
#
# The frame entity has been in the tree for a long time with no item to place
# one and no way to put anything in it, so all of this is first contact.
#
# Usage: bash dev/frame-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

# Respect $CARGO_TARGET_DIR the way `cargo build` itself already does; see
# the same lines in dev/join-test.sh for what hardcoding it used to cost.
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
BIN="$TARGET_DIR/debug/foton"

PORT=25589
RUN_DIR="$ROOT/run-frame"

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
nohup "$BIN" > server.log 2>&1 < /dev/null &
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

# A comparator does not read a frame beside it: it reads *through* the block
# the frame is hanging on. So the line is comparator, wall, frame -- the
# comparator sees the solid block, looks one further, and finds the frame on
# the far side of it.
CMDS='gamemode creative'
# One throwaway command first: the very first `setblock` of a run can land
# before the chunk around the player is ready, and a floor block that never
# arrives takes the redstone dust above it with it.
CMDS="$CMDS;;time set day"
for z in -1 0 1 2 3; do
  CMDS="$CMDS;;setblock 0 99 $z minecraft:stone"
done
CMDS="$CMDS;;setblock 0 100 1 minecraft:stone"
CMDS="$CMDS;;setblock 0 100 0 minecraft:comparator[facing=south]"
# and the dust's own support again, immediately before the dust
CMDS="$CMDS;;setblock 0 99 -1 minecraft:stone"
CMDS="$CMDS;;setblock 0 100 -1 minecraft:redstone_wire"
CMDS="$CMDS;;teleport @s 0 100 3"

# Asked separately from the power below, and not only for the extra tick it
# buys the freshly placed wire to settle: a missing wire fails a
# `[power=0]` check exactly like a lit one, and that is the wrong answer
# to the wrong question.
CMDS="$CMDS;;execute if block 0 100 -1 minecraft:redstone_wire run tellraw @s \"WIREEXISTS\""
CMDS="$CMDS;;execute if block 0 100 -1 minecraft:redstone_wire[power=0] run tellraw @s \"DUSTSTARTSDARK\""
CMDS="$CMDS;;execute unless entity @e[type=minecraft:item_frame] run tellraw @s \"NOFRAMEYET\""

# The frame hangs on the south face of the wall, into the block at z=2.
CMDS="$CMDS;;clear @s"
CMDS="$CMDS;;give @s minecraft:item_frame"
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;!useon 0 100 1 south"
CMDS="$CMDS;;execute if entity @e[type=minecraft:item_frame] run tellraw @s \"FRAMEHUNG\""

# An empty frame reads nothing.
CMDS="$CMDS;;execute if block 0 100 -1 minecraft:redstone_wire[power=0] run tellraw @s \"EMPTYFRAMEREADSNOTHING\""

# Put something in it: a filled frame at rotation zero reads 1.
CMDS="$CMDS;;clear @s"
CMDS="$CMDS;;give @s minecraft:diamond"
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;!useentity item_frame"
CMDS="$CMDS;;execute if block 0 100 -1 minecraft:redstone_wire[power=1] run tellraw @s \"FILLEDFRAMEREADSONE\""

# Turning it steps the signal.
CMDS="$CMDS;;!useentity item_frame"
CMDS="$CMDS;;execute if block 0 100 -1 minecraft:redstone_wire[power=2] run tellraw @s \"TURNEDFRAMEREADSTWO\""

# Breaking it is two hits, not one: the first takes the item out and leaves the
# frame on the wall, the second takes the frame down. Survival, because a
# creative player gets nothing back.
CMDS="$CMDS;;gamemode survival"

# A frame on the south face of the wall at y=100,z=1 sits at z=2.03, which is
# what the two checks below are anchored to. The world generates item frames of
# its own, so every check names the spot rather than asking whether one exists.
CMDS="$CMDS;;!attack item_frame"
CMDS="$CMDS;;execute positioned 0.5 100.5 2.03 if entity @e[type=minecraft:item_frame,distance=..1] run tellraw @s \"FRAMESURVIVEDTHEPUNCH\""
CMDS="$CMDS;;execute if block 0 100 -1 minecraft:redstone_wire[power=0] run tellraw @s \"PUNCHEMPTIEDTHEFRAME\""

# The second hit takes the frame itself down.
CMDS="$CMDS;;!attack item_frame"
CMDS="$CMDS;;execute positioned 0.5 100.5 2.03 unless entity @e[type=minecraft:item_frame,distance=..1] run tellraw @s \"FRAMECAMEOFFTHEWALL\""

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=2 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what happened ==="
grep "server says" join.log | grep -oE "WIREEXISTS|DUSTSTARTSDARK|NOFRAMEYET|FRAMEHUNG|EMPTYFRAMEREADSNOTHING|FILLEDFRAMEREADSONE|TURNEDFRAMEREADSTWO|FRAMESURVIVEDTHEPUNCH|PUNCHEMPTIEDTHEFRAME|FRAMECAMEOFFTHEWALL"
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic|incorrect" | tail -5

fail() { echo "########## FRAME TEST FAILED ($1) ##########"; exit 1; }
said() { grep "server says" join.log | grep -q "$1"; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }
said WIREEXISTS             || fail "the dust never got placed"
said DUSTSTARTSDARK         || fail "the dust was already powered"
said NOFRAMEYET             || fail "a frame existed before one was placed"
said FRAMEHUNG              || fail "the item frame item hung nothing"
said EMPTYFRAMEREADSNOTHING || fail "an empty frame gave a signal"
said FILLEDFRAMEREADSONE    || fail "putting an item in the frame read nothing"
said TURNEDFRAMEREADSTWO    || fail "turning the item did not step the signal"
said PUNCHEMPTIEDTHEFRAME   || fail "punching a full frame did not take the item out"
said FRAMESURVIVEDTHEPUNCH  || fail "the first punch broke the frame instead of emptying it"
said FRAMECAMEOFFTHEWALL    || fail "the frame survived the hit that should have broken it"
echo "########## FRAME TEST PASSED ##########"
