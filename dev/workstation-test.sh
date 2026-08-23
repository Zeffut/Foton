#!/bin/bash
# Open every workstation a player can open.
#
# What these blocks *do* is tested in Rust, where the computation lives -- a
# recipe button and a slot click need container packets the scripted client
# cannot send. What only a real client can answer is whether right-clicking the
# block reaches the behavior at all, which is what this covers.
#
# Usage: bash dev/workstation-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25593
RUN_DIR="$ROOT/run-workstation"

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
CMDS="$CMDS;;setblock 0 99 0 minecraft:stone"
CMDS="$CMDS;;setblock 0 99 2 minecraft:stone"
CMDS="$CMDS;;setblock 0 100 0 minecraft:stonecutter"
CMDS="$CMDS;;setblock 0 100 2 minecraft:grindstone"
CMDS="$CMDS;;setblock 0 99 4 minecraft:stone"
CMDS="$CMDS;;setblock 0 100 4 minecraft:smithing_table"
CMDS="$CMDS;;setblock 0 99 6 minecraft:stone"
CMDS="$CMDS;;setblock 0 100 6 minecraft:lectern"
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

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=2 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what happened ==="
grep -cE "a screen opened" join.log | sed 's/^/screens opened: /'
grep "server says" join.log | grep -oE "BOOKWENTON"
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic|incorrect" | tail -5

fail() { echo "########## WORKSTATION TEST FAILED ($1) ##########"; exit 1; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }
screens=$(grep -c "a screen opened" join.log)
[ "$screens" -eq 4 ] \
  || fail "expected four workstations to open, got $screens"
grep "server says" join.log | grep -q BOOKWENTON \
  || fail "the book never went onto the lectern"
echo "########## WORKSTATION TEST PASSED ##########"
