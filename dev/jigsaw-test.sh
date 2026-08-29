#!/bin/bash
# Press the generate button on a jigsaw block and check the pool it names is standing.
#
# Nothing else reaches this path. `ServerboundJigsawGeneratePacket` has no
# command behind it, so the only way in is a client sending the packet the
# editor screen sends, which `dev/join.py` does with `!jigsawgenerate`.
#
# `village/common/well_bottoms` holds exactly one element, the legacy template
# `village/common/well_bottom`, which holds exactly one jigsaw block: named
# `minecraft:bottom`, at local (3, 2, 0), with `minecraft:cobblestone` under it
# at (3, 1, 0) and (3, 0, 0) and `minecraft:cobblestone` for its final state.
# So the assembly has nothing to choose except a rotation -- and a rotation
# cannot move any of those three, because the anchored jigsaw always lands on
# the block the jigsaw block faces and the other two share its column. The whole
# piece then drops by one, vanilla's ground level delta, so the anchor is a block
# below what the jigsaw block faces.
#
# Usage: bash dev/jigsaw-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25622
RUN_DIR="$ROOT/run-jigsaw"

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
# The anti-spam counter drains one point per tick, and this test types more
# lines than that allows.
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

JIGSAW_NBT='{name:"minecraft:empty",target:"minecraft:bottom",pool:"minecraft:village/common/well_bottoms",final_state:"minecraft:air",joint:"aligned",selection_priority:0,placement_priority:0}'

CMDS='gamemode creative'
CMDS="$CMDS;;teleport @s 4 108 4"
# Clear both columns the two runs write into, so a cobblestone or a jigsaw block
# afterwards can only have come from the placement.
for y in 96 97 98 99 100; do
  CMDS="$CMDS;;setblock 0 $y -1 minecraft:air"
  CMDS="$CMDS;;setblock 0 $y 3 minecraft:air"
done

# Run one: the button with `keep jigsaws` off, which is the default. The
# template's own jigsaw block becomes its final state, cobblestone.
CMDS="$CMDS;;setblock 0 100 0 minecraft:jigsaw[orientation=north_up]$JIGSAW_NBT"
CMDS="$CMDS;;!jigsawgenerate 0 100 0 0"
CMDS="$CMDS;;execute if block 0 99 -1 minecraft:cobblestone run tellraw @s \"JIGSAWANCHORSTATE\""
CMDS="$CMDS;;execute if block 0 98 -1 minecraft:cobblestone run tellraw @s \"JIGSAWBODYUPPER\""
CMDS="$CMDS;;execute if block 0 97 -1 minecraft:cobblestone run tellraw @s \"JIGSAWBODYLOWER\""
# Vanilla leaves the map-maker's own jigsaw block alone; only the template's are
# replaced.
CMDS="$CMDS;;execute if block 0 100 0 minecraft:jigsaw run tellraw @s \"JIGSAWBUTTONSURVIVES\""

# Run two: the same pool with `keep jigsaws` on, so the template's jigsaw block
# stays standing instead of turning into cobblestone.
CMDS="$CMDS;;setblock 0 100 4 minecraft:jigsaw[orientation=north_up]$JIGSAW_NBT"
CMDS="$CMDS;;!jigsawgenerate 0 100 4 0 keep"
CMDS="$CMDS;;execute if block 0 99 3 minecraft:jigsaw run tellraw @s \"JIGSAWKEPT\""
CMDS="$CMDS;;execute if block 0 98 3 minecraft:cobblestone run tellraw @s \"JIGSAWKEPTBODY\""

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=2 JOIN_COMMAND_SETTLE_SECONDS=1.0 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what the server answered ==="
grep "server says" join.log | grep -E "JIGSAW" | tail -8
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic|incorrect|unknown" | tail -6

if [ $STATUS -ne 0 ]; then
  echo "########## JIGSAW TEST FAILED (the client never settled) ##########"
  tail -20 join.log
  exit 1
fi
# Only the server's own reply counts: join.py echoes the commands it sends, and
# the command text carries the marker too. `-w` is not decoration: without it
# JIGSAWKEPTBODY answers for JIGSAWKEPT, and the keep-jigsaws flag can stop
# working while this still reports a pass.
for marker in JIGSAWANCHORSTATE JIGSAWBODYUPPER JIGSAWBODYLOWER JIGSAWBUTTONSURVIVES JIGSAWKEPT JIGSAWKEPTBODY; do
  if ! grep "server says" join.log | grep -qw "$marker"; then
    echo "########## JIGSAW TEST FAILED ($marker missing) ##########"
    exit 1
  fi
done
echo "########## JIGSAW TEST PASSED ##########"
