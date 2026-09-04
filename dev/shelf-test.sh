#!/bin/bash
# Put an item on a shelf by hand, then take it back off.
#
# A shelf has no menu: the only way an item reaches it is a player clicking one
# of the three slots on its front face, which no command can do. So the swap is
# unverifiable without a real client, and this is the only test that covers it.
#
# The player stays in survival on purpose: in creative the shelf keeps a copy of
# the item in hand, so the hand would look unchanged and prove nothing.
#
# Usage: bash dev/shelf-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

# Respect $CARGO_TARGET_DIR the way `cargo build` itself already does; see
# the same lines in dev/join-test.sh for what hardcoding it used to cost.
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
BIN="$TARGET_DIR/debug/foton"

PORT=25601
RUN_DIR="$ROOT/run-shelf"

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

# A shelf faces north by default, so the clickable face is its north one. The
# client's cursor sits in the middle of that face, which is the middle slot.
CMDS='gamemode survival'
CMDS="$CMDS;;teleport @s 0 100 -2"
CMDS="$CMDS;;setblock 0 99 0 minecraft:stone"
CMDS="$CMDS;;setblock 0 100 0 minecraft:oak_shelf"
CMDS="$CMDS;;execute if block 0 100 0 minecraft:oak_shelf run tellraw @s \"SHELFPLACED\""
CMDS="$CMDS;;give @s minecraft:diamond 1"
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;execute if entity @s[nbt={Inventory:[{id:\"minecraft:diamond\"}]}] run tellraw @s \"DIAMONDINHAND\""
# Click the front face: the diamond goes on the shelf and leaves the hand.
CMDS="$CMDS;;!useon 0 100 0 north"
CMDS="$CMDS;;execute unless entity @s[nbt={Inventory:[{id:\"minecraft:diamond\"}]}] run tellraw @s \"SHELFTOOKIT\""
# Click the same slot empty-handed: the diamond comes back.
CMDS="$CMDS;;!useon 0 100 0 north"
CMDS="$CMDS;;execute if entity @s[nbt={Inventory:[{id:\"minecraft:diamond\"}]}] run tellraw @s \"SHELFGAVEITBACK\""

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=2 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what the server answered ==="
grep "server says" join.log | grep -vE "setblock|teleport|gamemode|give" | tail -8
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic|incorrect|unknown" | tail -6

if [ $STATUS -ne 0 ]; then
  echo "########## SHELF TEST FAILED (the client never settled) ##########"
  tail -20 join.log
  exit 1
fi
for marker in SHELFPLACED DIAMONDINHAND SHELFTOOKIT SHELFGAVEITBACK; do
  # Only the server's own reply counts: join.py echoes the commands it sends.
  if ! grep "server says" join.log | grep -q "$marker"; then
    echo "########## SHELF TEST FAILED ($marker missing) ##########"
    exit 1
  fi
done
echo "########## SHELF TEST PASSED ##########"
