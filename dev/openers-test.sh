#!/bin/bash
# Open a trapped chest and check it powers redstone.
#
# The signal is the only thing separating a trapped chest from a chest, and it
# needs a real right-click and a real menu close -- no command opens a
# container. So this also exercises the container opener count end to end,
# which is what the barrel's `open` state and the chest lid sounds ride on.
#
# Usage: bash dev/trapped-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25579
RUN_DIR="$ROOT/run-trapped"

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

# A redstone lamp sits beside each chest, not on it: a solid block on top of a
# chest stops it opening at all, so the lamp has to read the weak signal from
# the side. A plain chest a few blocks over carries an identical lamp and must
# stay dark throughout -- the control that keeps this from passing on some
# unrelated power source. A third trapped chest, buried under stone, must
# refuse to open at all.
CMDS='gamemode creative'
CMDS="$CMDS;;teleport @s 0 101 0"
CMDS="$CMDS;;setblock 0 100 0 minecraft:trapped_chest"
CMDS="$CMDS;;setblock 1 100 0 minecraft:redstone_lamp"
# Dust beside it, on a block of its own: a wire asks the block a different
# question than a lamp does, and a trapped chest that lights a lamp but leaves
# the dust dark is the classic way to get this half right. Dust needs
# something solid underneath or it pops straight off, and a missing wire makes
# every question about it answer the wrong way.
CMDS="$CMDS;;setblock 0 99 -1 minecraft:stone"
CMDS="$CMDS;;setblock 0 100 -1 minecraft:redstone_wire"
CMDS="$CMDS;;setblock 5 100 0 minecraft:chest"
CMDS="$CMDS;;setblock 6 100 0 minecraft:redstone_lamp"
CMDS="$CMDS;;setblock 0 100 4 minecraft:trapped_chest"
CMDS="$CMDS;;setblock 0 101 4 minecraft:stone"
CMDS="$CMDS;;teleport @s 3 100 1"
CMDS="$CMDS;;execute if block 1 100 0 minecraft:redstone_lamp[lit=false] run tellraw @s \"LAMPSTARTSOFF\""
CMDS="$CMDS;;execute if block 0 100 -1 minecraft:redstone_wire[power=0] run tellraw @s \"DUSTSTARTSDARK\""
CMDS="$CMDS;;clear @s"
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;!useon 0 100 0 east"
CMDS="$CMDS;;execute if block 1 100 0 minecraft:redstone_lamp[lit=true] run tellraw @s \"LAMPCAMEON\""
CMDS="$CMDS;;execute if block 0 100 -1 minecraft:redstone_wire[power=1] run tellraw @s \"DUSTGOTPOWER\""
# closing it has to take the signal away again
CMDS="$CMDS;;!close"
CMDS="$CMDS;;execute if block 1 100 0 minecraft:redstone_lamp[lit=false] run tellraw @s \"LAMPWENTOFF\""
CMDS="$CMDS;;execute if block 0 100 -1 minecraft:redstone_wire[power=0] run tellraw @s \"DUSTWENTDARK\""
# a plain chest, opened the same way, powers nothing at all
CMDS="$CMDS;;!useon 5 100 0 west"
CMDS="$CMDS;;execute if block 6 100 0 minecraft:redstone_lamp[lit=false] run tellraw @s \"CONTROLSTAYSOFF\""
CMDS="$CMDS;;!close"
# and one under a solid block does not open, exactly as a chest does not
CMDS="$CMDS;;teleport @s 0 100 2"
CMDS="$CMDS;;!useon 0 100 4 north"
# the other thing the opener count drives: a barrel looks open while somebody
# is in it. Unlike a chest a barrel opens fine with a block on top, so this
# also pins down that the two do not share the blocking rule.
CMDS="$CMDS;;setblock 8 100 0 minecraft:barrel"
CMDS="$CMDS;;teleport @s 7 100 0"
CMDS="$CMDS;;execute if block 8 100 0 minecraft:barrel[open=false] run tellraw @s \"BARRELSTARTSSHUT\""
CMDS="$CMDS;;!useon 8 100 0 west"
CMDS="$CMDS;;execute if block 8 100 0 minecraft:barrel[open=true] run tellraw @s \"BARRELLOOKSOPEN\""
CMDS="$CMDS;;!close"
CMDS="$CMDS;;execute if block 8 100 0 minecraft:barrel[open=false] run tellraw @s \"BARRELSHUTAGAIN\""

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=2 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what happened ==="
grep -E "server says|a screen opened|screen was closed" join.log | grep -vE "setblock|teleport|gamemode|clear" | tail -18
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic|incorrect|unknown" | tail -6

fail() { echo "########## TRAPPED CHEST TEST FAILED ($1) ##########"; exit 1; }
said() { grep "server says" join.log | grep -q "$1"; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }
said LAMPSTARTSOFF   || fail "the lamp was lit before anyone opened anything"
said DUSTSTARTSDARK  || fail "the dust was powered before anyone opened anything"
screens=$(grep -c 'a screen opened' join.log)
[ "$screens" -eq 3 ]   || fail "expected two chests and a barrel to open and the buried chest not to, got $screens"
said LAMPCAMEON      || fail "opening the trapped chest powered nothing"
said DUSTGOTPOWER    || fail "the lamp lit but the dust beside the chest stayed dark"
said LAMPWENTOFF     || fail "closing the trapped chest left the signal on"
said DUSTWENTDARK    || fail "the dust stayed powered after the chest closed"
said CONTROLSTAYSOFF || fail "a plain chest powered redstone too"
said BARRELSTARTSSHUT || fail "the barrel was already open"
said BARRELLOOKSOPEN  || fail "opening the barrel left it looking shut"
said BARRELSHUTAGAIN  || fail "the barrel stayed open after it was closed"
echo "########## TRAPPED CHEST TEST PASSED ##########"
