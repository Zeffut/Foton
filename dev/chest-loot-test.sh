#!/bin/bash
# Give a chest the loot table worldgen writes and watch a comparator find loot.
#
# Nothing can read a container's contents from outside -- there is no `data`
# command and no `item` command -- but a comparator can, and its `powered` flag
# is an ordinary block state that `execute if block` reads at any time. So the
# question "did the chest roll its table" is asked as "did the comparator beside
# it light up".
#
# The rig is built in a deliberate order. The chest goes down first, with no
# comparator anywhere near it, and is checked to still be carrying its
# `LootTable` tag: vanilla does not roll on load, it rolls on first access. Only
# then does the comparator arrive, and reading the chest is what spends the
# table -- no player ever opens it. That is the behaviour that matters: a hopper
# draining an untouched dungeon chest gets its loot too.
#
# The control rig is a plain chest. Its comparator must stay dark, otherwise the
# first rig proves nothing.
#
# Usage: bash dev/chest-loot-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

# Respect $CARGO_TARGET_DIR the way `cargo build` itself already does; see
# the same lines in dev/join-test.sh for what hardcoding it used to cost.
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
BIN="$TARGET_DIR/debug/foton"

PORT=25602
RUN_DIR="$ROOT/run-chestloot"

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

TABLE='minecraft:chests/simple_dungeon'

CMDS='gamemode creative'
# One throwaway command first: the very first `setblock` of a run can land
# before the chunk around the player is ready.
CMDS="$CMDS;;time set day"
CMDS="$CMDS;;teleport @s 2 100 0"

# Two rigs four blocks apart: a generated chest at x=0, a plain one at x=4.
for x in 0 4; do
  CMDS="$CMDS;;setblock $x 99 0 minecraft:stone"
  CMDS="$CMDS;;setblock $x 99 1 minecraft:stone"
  CMDS="$CMDS;;setblock $x 99 2 minecraft:stone"
done

# The chest arrives the way a structure leaves it: an empty container carrying
# a loot table and the seed to roll it with.
CMDS="$CMDS;;setblock 0 100 0 minecraft:chest{LootTable:\"$TABLE\",LootTableSeed:1234L}"
CMDS="$CMDS;;setblock 4 100 0 minecraft:chest"
CMDS="$CMDS;;execute if block 0 100 0 minecraft:chest{LootTable:\"$TABLE\"} run tellraw @s \"CHESTARRIVESPACKED\""

# Now the comparator. Its `facing` names the side it reads from, so one south
# of the chest faces north.
for x in 0 4; do
  CMDS="$CMDS;;setblock $x 100 1 minecraft:comparator[facing=north]"
  CMDS="$CMDS;;setblock $x 100 2 minecraft:redstone_wire"
done
# A comparator recomputes on a scheduled tick, so give it a beat.
CMDS="$CMDS;;!wait 2"

CMDS="$CMDS;;execute if block 0 100 1 minecraft:comparator[powered=true] run tellraw @s \"COMPARATORFOUNDLOOT\""
CMDS="$CMDS;;execute unless block 0 100 2 minecraft:redstone_wire[power=0] run tellraw @s \"WIREPOWEREDBYLOOT\""
CMDS="$CMDS;;execute unless block 0 100 0 minecraft:chest{LootTable:\"$TABLE\"} run tellraw @s \"TABLEWASSPENT\""

# The control: an ordinary chest nobody filled reads as nothing.
CMDS="$CMDS;;execute if block 4 100 1 minecraft:comparator[powered=false] run tellraw @s \"PLAINCHESTSTAYSDARK\""
CMDS="$CMDS;;execute if block 4 100 2 minecraft:redstone_wire[power=0] run tellraw @s \"CONTROLWIREDARK\""

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=2 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what happened ==="
grep "server says" join.log \
  | grep -oE "CHESTARRIVESPACKED|COMPARATORFOUNDLOOT|WIREPOWEREDBYLOOT|TABLEWASSPENT|PLAINCHESTSTAYSDARK|CONTROLWIREDARK"
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic" | tail -5

fail() { echo "########## CHEST LOOT TEST FAILED ($1) ##########"; exit 1; }
said() { grep "server says" join.log | grep -q "$1"; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }
said CHESTARRIVESPACKED  || fail "the chest lost its loot table on the way in"
said COMPARATORFOUNDLOOT || fail "the comparator read nothing; the table was never rolled"
said WIREPOWEREDBYLOOT   || fail "the comparator lit but sent no signal"
said TABLEWASSPENT       || fail "the chest kept its table after being rolled"
said PLAINCHESTSTAYSDARK || fail "an empty chest powered its comparator"
said CONTROLWIREDARK     || fail "an empty chest powered redstone"
echo "########## CHEST LOOT TEST PASSED ##########"
