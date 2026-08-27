#!/bin/bash
# An eyeblossom opening at dusk and closing at dawn.
#
# Both of the block's ticks were literal no-ops, so a flower stayed whichever
# way it was placed for ever. What decides it is the `gameplay/eyeblossom_open`
# environment attribute, which the overworld `day` timeline turns on at 12600
# and off at 23401.
#
# A row of five closed eyeblossoms is planted, the clock is set to the middle of
# the night, and they have to be open afterwards. Then the clock is set to the
# middle of the day and they have to be closed again. The round trip is the
# point: a block that simply turned into an open eyeblossom whatever the hour
# would pass the first half and fail the second.
#
# The row is a row on purpose. A flower that turns wakes every eyeblossom within
# three blocks, so the whole line goes over once any one of them is picked --
# which is also what stops this resting on a random tick landing on one
# particular block.
#
# The random tick speed is raised so the wait is seconds rather than minutes. At
# 500 a given block is picked about once in eight ticks, so over a hundred and
# sixty ticks across five flowers the chance of none being picked is not a
# number anyone will meet.
#
# Everything is built within a chunk or two of the player: only the nine chunks
# around them are loaded, and `setblock` outside that fails without stopping the
# script.
#
# Usage: bash dev/eyeblossom-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25637
RUN_DIR="$ROOT/run-eyeblossom"

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
add "setblock 0 99 8 minecraft:stone"
add "teleport @s 0 100 8"
# The teleport crosses a chunk border and only the nine chunks around the
# player are loaded. `setblock` into an unloaded chunk fails quietly and every
# `execute if block` below would then be answered by nothing.
add "!wait 2"

# The registry spells its rules in snake case; the camel-case names vanilla
# accepts are silently ignored here.
add "gamerule random_tick_speed 500"

for x in 0 1 2 3 4; do
  add "setblock $x 99 0 minecraft:dirt"
  add "setblock $x 100 0 minecraft:closed_eyeblossom"
done
add "execute if block 0 100 0 minecraft:closed_eyeblossom run tellraw @s \"PATCHPLANTEDCLOSED\""
add "execute if block 4 100 0 minecraft:closed_eyeblossom run tellraw @s \"FARENDPLANTEDCLOSED\""

# --- Dusk ---
add "time set 18000"
add "!wait 8"
add "execute if block 0 100 0 minecraft:open_eyeblossom run tellraw @s \"OPENEDATNIGHT\""
add "execute if block 4 100 0 minecraft:open_eyeblossom run tellraw @s \"FARENDOPENEDTOO\""

# --- Dawn ---
# The same row again, the other way. A block that only ever turned into an open
# eyeblossom would have passed everything above and fails here.
add "time set 6000"
add "!wait 8"
add "execute if block 0 100 0 minecraft:closed_eyeblossom run tellraw @s \"CLOSEDBYDAY\""
add "execute if block 4 100 0 minecraft:closed_eyeblossom run tellraw @s \"FARENDCLOSEDTOO\""

export JOIN_COMMANDS="$CMDS"
JOIN_COMMAND_SETTLE_SECONDS=0.5 JOIN_WATCH_SECONDS=2 \
  python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what the server answered ==="
grep "server says" join.log | grep -vE "setblock|teleport|gamemode|gamerule|time" | tail -10
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic|incorrect|unknown" | tail -6

if [ $STATUS -ne 0 ]; then
  echo "########## EYEBLOSSOM TEST FAILED (the client never settled) ##########"
  tail -20 join.log
  exit 1
fi
for marker in PATCHPLANTEDCLOSED FARENDPLANTEDCLOSED OPENEDATNIGHT \
              FARENDOPENEDTOO CLOSEDBYDAY FARENDCLOSEDTOO; do
  # Only the server's own reply counts: join.py echoes the commands it sends.
  if ! grep "server says" join.log | grep -q "$marker"; then
    echo "########## EYEBLOSSOM TEST FAILED ($marker missing) ##########"
    exit 1
  fi
done
echo "########## EYEBLOSSOM TEST PASSED ##########"
