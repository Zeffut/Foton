#!/bin/bash
# Bone meal on a sapling, with the random ticks switched off.
#
# `SaplingBlock` implemented the whole bonemealable interface and its block
# behavior answered `None` when asked whether it was one, which is the first
# question every caller of bone meal asks. So bone meal on a sapling did
# nothing, and no unit test of the trait could have shown it: the trait worked
# and nothing reached it.
#
# `random_tick_speed` is zero for the whole run, so the only thing that can turn
# a sapling into a tree here is the bone meal in the player's hand. The second
# sapling four blocks away is the proof of that: it is planted at the same
# moment on the same dirt and never touched, and it has to still be a sapling
# at the end.
#
# Everything is built within a chunk or two of the player: only the nine chunks
# around them are loaded, and `setblock` outside that fails without stopping the
# script.
#
# Usage: bash dev/bonemeal-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25639
RUN_DIR="$ROOT/run-bonemeal"

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

CMDS='gamemode survival'
# The registry spells its rules in snake case. Zero here is the whole control:
# nothing but a player's hand may advance anything for the rest of the run.
add "gamerule random_tick_speed 0"
add "time set 6000"
add "setblock 0 99 0 minecraft:stone"
add "teleport @s 0 100 0"
# The teleport crosses a chunk border and only the nine chunks around the
# player are loaded. `setblock` into an unloaded chunk fails quietly and every
# `execute if block` below would then be answered by nothing.
add "!wait 2"

# Dirt for both saplings, and room overhead for a tree.
for x in 0 1 2 3 4 5 6; do
  for z in -2 -1 0 1 2; do
    add "setblock $x 99 $z minecraft:dirt"
  done
done
add "setblock 2 100 0 minecraft:oak_sapling"
add "setblock 6 100 0 minecraft:oak_sapling"
add "execute if block 2 100 0 minecraft:oak_sapling run tellraw @s \"FEDSAPLINGPLANTED\""
add "execute if block 6 100 0 minecraft:oak_sapling run tellraw @s \"CONTROLSAPLINGPLANTED\""

# Bone meal advances a sapling with a 45% roll, and a sapling needs two
# advances to become a tree. Forty tries makes this a test of the wiring rather
# than of the dice: the chance of fewer than two successes is about three in a
# billion.
add "give @s minecraft:bone_meal 64"
add "!hotbar 0"
for _ in $(seq 1 40); do
  add "!useon 2 100 0 up"
done

add "!wait 2"
# An oak is at least four blocks tall, so a log two above where the sapling
# stood is a tree and not a leftover.
add "execute if block 2 102 0 #minecraft:logs run tellraw @s \"BONEMEALGREWATREE\""
add "execute if block 6 100 0 minecraft:oak_sapling run tellraw @s \"CONTROLSTAYEDASAPLING\""

export JOIN_COMMANDS="$CMDS"
JOIN_COMMAND_SETTLE_SECONDS=0.2 JOIN_WATCH_SECONDS=2 \
  python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what the server answered ==="
grep "server says" join.log | grep -vE "setblock|teleport|gamemode|gamerule|give|time" | tail -10
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic|incorrect|unknown" | tail -6

if [ $STATUS -ne 0 ]; then
  echo "########## BONE MEAL TEST FAILED (the client never settled) ##########"
  tail -20 join.log
  exit 1
fi
for marker in FEDSAPLINGPLANTED CONTROLSAPLINGPLANTED BONEMEALGREWATREE \
              CONTROLSTAYEDASAPLING; do
  # Only the server's own reply counts: join.py echoes the commands it sends.
  if ! grep "server says" join.log | grep -q "$marker"; then
    echo "########## BONE MEAL TEST FAILED ($marker missing) ##########"
    exit 1
  fi
done
echo "########## BONE MEAL TEST PASSED ##########"
