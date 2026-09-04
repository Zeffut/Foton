#!/bin/bash
# Open every workstation a player can open.
#
# Most of what these blocks *do* is tested in Rust, where the computation
# lives. What only a real client can answer is whether right-clicking the block
# reaches the behavior at all, which is what this covers -- plus, for the
# crafter, the whole chain from a shift-clicked item to a crafted one coming
# back out.
#
# Usage: bash dev/workstation-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

# Respect $CARGO_TARGET_DIR the way `cargo build` itself already does; see
# the same lines in dev/join-test.sh for what hardcoding it used to cost.
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
BIN="$TARGET_DIR/debug/foton"

PORT=25593
RUN_DIR="$ROOT/run-workstation"

echo "=== Building ==="
cargo build 2>&1 | tail -2
# A pipeline's status is its last command's, so `if ! cargo build | tail`
# tested `tail` and never failed. That made the branch below unreachable: a
# broken build fell straight through and the test ran against a stale binary.
if [ "${PIPESTATUS[0]}" -ne 0 ]; then
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

CMDS='gamemode creative'
# One throwaway command first: the very first `setblock` of a run can land
# before the chunk around the player is ready.
CMDS="$CMDS;;time set day"
CMDS="$CMDS;;setblock 0 99 0 minecraft:stone"
CMDS="$CMDS;;setblock 0 99 2 minecraft:stone"
CMDS="$CMDS;;setblock 0 100 0 minecraft:stonecutter"
CMDS="$CMDS;;setblock 0 100 2 minecraft:grindstone"
CMDS="$CMDS;;setblock 0 99 4 minecraft:stone"
CMDS="$CMDS;;setblock 0 100 4 minecraft:smithing_table"
CMDS="$CMDS;;setblock 0 99 6 minecraft:stone"
CMDS="$CMDS;;setblock 0 100 6 minecraft:lectern"
# A bell, with a lever beside it: redstone rings it once on the rising edge
# and flips the block's `powered` state, which is the only part of ringing a
# server can be asked about -- the swing itself is a client animation.
CMDS="$CMDS;;setblock 0 99 8 minecraft:stone"
CMDS="$CMDS;;setblock 0 100 8 minecraft:bell"
CMDS="$CMDS;;setblock 1 100 8 minecraft:redstone_block"
CMDS="$CMDS;;clear @s"
CMDS="$CMDS;;!hotbar 0"

CMDS="$CMDS;;teleport @s 1 100 0"
CMDS="$CMDS;;!useon 0 100 0 east"
CMDS="$CMDS;;!close"

CMDS="$CMDS;;teleport @s 1 100 2"
CMDS="$CMDS;;!useon 0 100 2 east"
CMDS="$CMDS;;!close"

CMDS="$CMDS;;teleport @s 1 100 4"
CMDS="$CMDS;;!useon 0 100 4 east"
CMDS="$CMDS;;!close"

# A lectern only opens once it has a book on it, so this also checks that
# putting one on works: two right-clicks, one screen.
CMDS="$CMDS;;give @s minecraft:writable_book"
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;teleport @s 1 100 6"
CMDS="$CMDS;;!useon 0 100 6 east"
CMDS="$CMDS;;execute if block 0 100 6 minecraft:lectern[has_book=true] run tellraw @s \"BOOKWENTON\""
CMDS="$CMDS;;clear @s"
CMDS="$CMDS;;!useon 0 100 6 east"
CMDS="$CMDS;;!close"

CMDS="$CMDS;;execute if block 0 100 8 minecraft:bell[powered=true] run tellraw @s \"BELLRANG\""

# The crafter, end to end, read entirely off block states: a log shift-clicked
# into the grid, a redstone block beside it, and the grid empty again once the
# craft has run. One log is enough because oak planks are a shapeless
# one-ingredient recipe -- a nine-ingredient one could not be loaded by
# shift-clicking, which fills a single slot.
#
# The comparator is the only way to see inside the grid. As with the jukebox,
# its `facing` names the side it reads from, so a comparator south of the
# crafter faces north. One filled slot out of nine reads 1.
CMDS="$CMDS;;setblock 0 99 10 minecraft:stone"
CMDS="$CMDS;;setblock 0 99 11 minecraft:stone"
CMDS="$CMDS;;setblock 0 99 12 minecraft:stone"
CMDS="$CMDS;;setblock 0 100 10 minecraft:crafter"
CMDS="$CMDS;;setblock 0 100 11 minecraft:comparator[facing=north]"
CMDS="$CMDS;;setblock 0 100 12 minecraft:redstone_wire"
CMDS="$CMDS;;clear @s"
CMDS="$CMDS;;give @s minecraft:oak_log 1"
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;teleport @s 2 100 10"
CMDS="$CMDS;;execute if block 0 100 12 minecraft:redstone_wire[power=0] run tellraw @s \"CRAFTERSTARTSEMPTY\""
CMDS="$CMDS;;!useon 0 100 10 east"
# Slot 36 is the first hotbar square, where a give lands.
CMDS="$CMDS;;!shiftclick 36"
CMDS="$CMDS;;!close"
CMDS="$CMDS;;execute if block 0 100 12 minecraft:redstone_wire[power=1] run tellraw @s \"GRIDLOADED\""
CMDS="$CMDS;;teleport @s 6 100 16"
CMDS="$CMDS;;setblock 1 100 10 minecraft:redstone_block"
CMDS="$CMDS;;execute if block 0 100 10 minecraft:crafter[triggered=true] run tellraw @s \"CRAFTERARMED\""
# Two throwaway commands: the craft is scheduled four ticks out.
CMDS="$CMDS;;time set day"
CMDS="$CMDS;;time set day"
CMDS="$CMDS;;execute if block 0 100 12 minecraft:redstone_wire[power=0] run tellraw @s \"GRIDEMPTIED\""

# The loom. What it makes is covered by unit tests -- no command can read a
# banner's pattern layers back, and the result never becomes a block -- so what
# this adds is the packet path: the menu opens, three restricted slots take
# their items by shift-click, and a pattern button press reaches the handler.
CMDS="$CMDS;;setblock 0 99 14 minecraft:stone"
CMDS="$CMDS;;setblock 0 100 14 minecraft:loom"
CMDS="$CMDS;;clear @s"
CMDS="$CMDS;;give @s minecraft:white_banner 1"
CMDS="$CMDS;;give @s minecraft:red_dye 1"
CMDS="$CMDS;;teleport @s 1 100 14"
CMDS="$CMDS;;!useon 0 100 14 east"
# Slots 4 to 39 are the player inventory; 31 and 32 are the first two hotbar
# squares, where the two gives landed.
CMDS="$CMDS;;!shiftclick 31"
CMDS="$CMDS;;!shiftclick 32"
CMDS="$CMDS;;!button 0"
CMDS="$CMDS;;!close"

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=2 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what happened ==="
grep -cE "a screen opened" join.log | sed 's/^/screens opened: /'
grep "server says" join.log | grep -oE "BOOKWENTON|BELLRANG|CRAFTERSTARTSEMPTY|GRIDLOADED|CRAFTERARMED|GRIDEMPTIED"
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic|incorrect" | tail -5

fail() { echo "########## WORKSTATION TEST FAILED ($1) ##########"; exit 1; }
said() { grep "server says" join.log | grep -q "$1"; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }
screens=$(grep -c "a screen opened" join.log)
[ "$screens" -eq 6 ] \
  || fail "expected six workstations to open, got $screens"
said BOOKWENTON         || fail "the book never went onto the lectern"
said BELLRANG           || fail "redstone did not ring the bell"
said CRAFTERSTARTSEMPTY || fail "the comparator read something from an empty crafter"
said GRIDLOADED         || fail "the log never reached the crafter grid"
said CRAFTERARMED       || fail "redstone did not arm the crafter"
said GRIDEMPTIED        || fail "the crafter never ran the recipe"
echo "########## WORKSTATION TEST PASSED ##########"
