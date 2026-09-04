#!/bin/bash
# Dress an armor stand and hang a painting.
#
# Neither of these leaves a trace a command can read: a stand's gear and a
# painting's variant are entity state, and there is no `/data`. What the client
# does see is the equipment packet and the spawn, so that is what this asserts.
#
# Usage: bash dev/decoration-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

# Respect $CARGO_TARGET_DIR the way `cargo build` itself already does; see
# the same lines in dev/join-test.sh for what hardcoding it used to cost.
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
BIN="$TARGET_DIR/debug/foton"

PORT=25599
RUN_DIR="$ROOT/run-decoration"

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

CMDS='gamemode creative'
# One throwaway command first: the very first `setblock` of a run can land
# before the chunk around the player is ready.
CMDS="$CMDS;;time set day"

# --- the armor stand ---
CMDS="$CMDS;;setblock 0 99 0 minecraft:stone"
CMDS="$CMDS;;clear @s"
CMDS="$CMDS;;give @s minecraft:iron_helmet"
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;teleport @s 2 100 0"
CMDS="$CMDS;;summon armor_stand 0.5 100.0 0.5"
CMDS="$CMDS;;!spawned armor_stand"
# Right-clicking a stand with a helmet puts the helmet on its head.
CMDS="$CMDS;;!useentity armor_stand"
CMDS="$CMDS;;!wait 2"

# --- the painting ---
# A wall to hang it on, two blocks wide and two tall, so the biggest variant
# that fits is bigger than one block. The painting goes on the west face.
for y in 100 101; do
  for z in 4 5; do
    CMDS="$CMDS;;setblock 0 $y $z minecraft:stone"
  done
done
CMDS="$CMDS;;clear @s"
CMDS="$CMDS;;give @s minecraft:painting"
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;teleport @s -2 100 4"
CMDS="$CMDS;;!useon 0 100 4 west"
CMDS="$CMDS;;!wait 2"
CMDS="$CMDS;;!spawned painting"

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=2 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what happened ==="
grep -E "was equipped in|the client saw a (armor_stand|painting)|no (armor_stand|painting) has spawned" join.log
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic" | tail -5

fail() { echo "########## DECORATION TEST FAILED ($1) ##########"; exit 1; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }

# `!spawned` only asks whether the client ever saw one, so it is worthless for
# a type the world makes on its own -- items and falling blocks turn up in every
# run. Nothing generates an armor stand or a painting, so for these two the
# question is a real one.

grep -q "the client saw a armor_stand spawn" join.log \
  || fail "the armor stand never spawned"
grep -q "was equipped in head" join.log \
  || fail "right-clicking the stand with a helmet did not dress it"
grep -q "the client saw a painting spawn" join.log \
  || fail "the painting item hung nothing on the wall"
echo "########## DECORATION TEST PASSED ##########"
