#!/bin/bash
# Knock the ceiling out from under a stalactite and watch it come down.
#
# A four-block stalactite hangs from stone at 0 110 0 with a zombie standing
# underneath. Breaking the ceiling has to drop the whole column as falling
# blocks, and the tip carries the weight: vanilla gives only the tip
# `setHurtsEntities`, at one damage per fall distance per block of stalactite
# with a floor of six, so a six-block drop is far past a zombie's twenty.
#
# The second zombie five blocks to the side is the control. It is the same mob,
# on the same floor, at the same time of night, and the only thing different
# about it is that nothing fell on it. If it died too, the stalactite was not
# what killed the first one.
#
# Usage: bash dev/dripstone-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

# Respect $CARGO_TARGET_DIR the way `cargo build` itself already does; see
# the same lines in dev/join-test.sh for what hardcoding it used to cost.
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
BIN="$TARGET_DIR/debug/foton"

PORT=25619
RUN_DIR="$ROOT/run-dripstone"

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

CMDS='gamemode survival'
# 18000 is the middle of the night, so neither zombie burns. A raw tick count
# rather than a name: the markers come from extracted timeline data and this
# test has no business depending on which of them are present.
CMDS="$CMDS;;time set 18000"
CMDS="$CMDS;;teleport @s 0 101 10"

# Floor for both zombies, ceiling for the stalactite.
CMDS="$CMDS;;setblock 0 99 0 minecraft:stone"
CMDS="$CMDS;;setblock 5 99 0 minecraft:stone"
CMDS="$CMDS;;setblock 0 110 0 minecraft:stone"

# The column, hung top down so each block settles its own thickness.
for y in 109 108 107 106; do
  CMDS="$CMDS;;setblock 0 $y 0 minecraft:pointed_dripstone[vertical_direction=down]"
done
CMDS="$CMDS;;execute if block 0 106 0 minecraft:pointed_dripstone[vertical_direction=down] run tellraw @s \"STALACTITEHUNG\""

CMDS="$CMDS;;summon minecraft:zombie 0.5 100.0 0.5 {Tags:[\"under\"],NoAI:1b}"
CMDS="$CMDS;;summon minecraft:zombie 5.5 100.0 0.5 {Tags:[\"aside\"],NoAI:1b}"
CMDS="$CMDS;;execute if entity @e[type=minecraft:zombie,tag=under] run tellraw @s \"ZOMBIEUNDERIT\""
CMDS="$CMDS;;execute if entity @e[type=minecraft:zombie,tag=aside] run tellraw @s \"ZOMBIEASIDE\""

CMDS="$CMDS;;setblock 0 110 0 minecraft:air destroy"
CMDS="$CMDS;;!wait 4"
CMDS="$CMDS;;execute if block 0 106 0 minecraft:air run tellraw @s \"STALACTITELETGO\""
CMDS="$CMDS;;execute if entity @e[type=minecraft:item,nbt={Item:{id:\"minecraft:pointed_dripstone\"}}] run tellraw @s \"STALACTITESHATTERED\""
CMDS="$CMDS;;execute if entity @e[type=minecraft:zombie,tag=aside] run tellraw @s \"ASIDEZOMBIESURVIVED\""
CMDS="$CMDS;;execute unless entity @e[type=minecraft:zombie,tag=under] run tellraw @s \"STALACTITECRUSHEDIT\""

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=2 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what the server answered ==="
grep "server says" join.log | grep -vE "setblock|teleport|gamemode|summon|time" | tail -10
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic|incorrect|unknown" | tail -6

if [ $STATUS -ne 0 ]; then
  echo "########## DRIPSTONE TEST FAILED (the client never settled) ##########"
  tail -20 join.log
  exit 1
fi
for marker in STALACTITEHUNG ZOMBIEUNDERIT ZOMBIEASIDE STALACTITELETGO \
              STALACTITESHATTERED ASIDEZOMBIESURVIVED STALACTITECRUSHEDIT; do
  # Only the server's own reply counts: join.py echoes the commands it sends.
  if ! grep "server says" join.log | grep -q "$marker"; then
    echo "########## DRIPSTONE TEST FAILED ($marker missing) ##########"
    exit 1
  fi
done
echo "########## DRIPSTONE TEST PASSED ##########"
