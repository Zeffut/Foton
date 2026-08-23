#!/bin/bash
# Put a flower in a pot by hand, then take it back out.
#
# A flower pot has no block entity: potting swaps the empty pot for a different
# block entirely, and unpotting swaps back. So the check is what block is
# standing there afterwards, which only a running server can answer.
#
# Usage: bash dev/flowerpot-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25577
RUN_DIR="$ROOT/run-flowerpot"

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
CMDS="$CMDS;;teleport @s 0 100 0"
CMDS="$CMDS;;setblock 0 99 0 minecraft:stone"
CMDS="$CMDS;;setblock 0 100 0 minecraft:flower_pot"
CMDS="$CMDS;;execute if block 0 100 0 minecraft:flower_pot run tellraw @s \"POTISEMPTY\""
# Put a dandelion in it.
CMDS="$CMDS;;give @s minecraft:dandelion 1"
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;!useon 0 100 0 up"
CMDS="$CMDS;;execute if block 0 100 0 minecraft:potted_dandelion run tellraw @s \"POTHOLDSFLOWER\""
# Take it back out with an empty hand.
CMDS="$CMDS;;clear @s"
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;!useon 0 100 0 up"
CMDS="$CMDS;;execute if block 0 100 0 minecraft:flower_pot run tellraw @s \"POTISEMPTYAGAIN\""
CMDS="$CMDS;;execute if entity @s[nbt={Inventory:[{id:\"minecraft:dandelion\"}]}] run tellraw @s \"FLOWERCAMEBACK\""

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=2 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what the server answered ==="
grep "server says" join.log | grep -vE "setblock|teleport|gamemode" | tail -8
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic|incorrect|unknown" | tail -6

if [ $STATUS -ne 0 ]; then
  echo "########## FLOWER POT TEST FAILED (the client never settled) ##########"
  tail -20 join.log
  exit 1
fi
for marker in POTISEMPTY POTHOLDSFLOWER POTISEMPTYAGAIN; do
  # Only the server's own reply counts: join.py echoes the commands it sends.
  if ! grep "server says" join.log | grep -q "$marker"; then
    echo "########## FLOWER POT TEST FAILED ($marker missing) ##########"
    exit 1
  fi
done
echo "########## FLOWER POT TEST PASSED ##########"
