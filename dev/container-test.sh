#!/bin/bash
# Check that placed container blocks really have block entities behind them.
#
# All three furnaces went unregistered: the behavior was written, a macro hid
# the struct from the codegen scanner, and nothing said so -- right-clicking one
# did nothing and smelting was unreachable. A unit test cannot see that, because
# the behavior compiles fine; only the running server can.
#
# `new_block_entity` comes from the block's behavior, so a block with NBT data
# behind it is a block whose behavior is registered. That one question covers
# every container, so they are all asked here.
#
# Usage: bash dev/container-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25574
RUN_DIR="$ROOT/run-container"

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

CMDS='teleport @s 0 100 0'
CMDS="$CMDS;;setblock 0 99 0 minecraft:furnace"
CMDS="$CMDS;;setblock 2 99 0 minecraft:smoker"
CMDS="$CMDS;;setblock 4 99 0 minecraft:blast_furnace"
CMDS="$CMDS;;setblock 6 99 0 minecraft:shulker_box"
CMDS="$CMDS;;setblock 8 99 0 minecraft:red_shulker_box"
CMDS="$CMDS;;execute if data block 0 99 0 {} run tellraw @s \"FURNACEHASENTITY\""
CMDS="$CMDS;;execute if data block 2 99 0 {} run tellraw @s \"SMOKERHASENTITY\""
CMDS="$CMDS;;execute if data block 4 99 0 {} run tellraw @s \"BLASTHASENTITY\""
CMDS="$CMDS;;execute if data block 6 99 0 {} run tellraw @s \"SHULKERHASENTITY\""
CMDS="$CMDS;;execute if data block 8 99 0 {} run tellraw @s \"REDSHULKERHASENTITY\""

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=2 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what the server answered ==="
grep "server says" join.log | tail -10

if [ $STATUS -ne 0 ]; then
  echo "########## CONTAINER TEST FAILED (the client never settled) ##########"
  tail -20 join.log
  exit 1
fi
for marker in FURNACEHASENTITY SMOKERHASENTITY BLASTHASENTITY \
              SHULKERHASENTITY REDSHULKERHASENTITY; do
  # Only the server's own reply counts: join.py echoes the commands it sends.
  if ! grep "server says" join.log | grep -q "$marker"; then
    echo "########## CONTAINER TEST FAILED ($marker missing) ##########"
    exit 1
  fi
done
echo "########## CONTAINER TEST PASSED ##########"
