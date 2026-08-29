#!/bin/bash
# Ask a running server to find a biome, and a biome its dimension cannot hold.
#
# `/locate biome` reads the generator's noise rather than the world's blocks, so
# nothing about it shows up in a unit test of the search itself: what this proves
# is that the command is wired, that a plains overworld answers with a position,
# and that a nether-only biome comes back as not found instead of scanning
# forty thousand columns and timing the client out.
#
# Usage: bash dev/locate-biome-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25623
RUN_DIR="$ROOT/run-locate-biome"

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
nohup "$ROOT/target/debug/foton" > server.log 2>&1 < /dev/null &
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

# The spawn is a plains start, so plains is both reachable and near. The nether
# biome is the negative: the overworld biome source cannot produce it, and
# vanilla answers from `possibleBiomes()` without sampling anything.
CMDS='locate biome minecraft:plains'
CMDS="$CMDS;;!wait 3"
CMDS="$CMDS;;locate biome minecraft:crimson_forest"

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=2 JOIN_COMMAND_SETTLE_SECONDS=5.0 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what the server answered ==="
grep "server says" join.log | grep -E "locate" | tail -6
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic" | tail -6

if [ $STATUS -ne 0 ]; then
  echo "########## LOCATE BIOME TEST FAILED (the client never settled) ##########"
  tail -20 join.log
  exit 1
fi
if ! grep "server says" join.log | grep -q "commands.locate.biome.success"; then
  echo "########## LOCATE BIOME TEST FAILED (plains was not found) ##########"
  exit 1
fi
if ! grep "server says" join.log | grep -q "commands.locate.biome.not_found"; then
  echo "########## LOCATE BIOME TEST FAILED (a nether biome was reported in the overworld) ##########"
  exit 1
fi
echo "########## LOCATE BIOME TEST PASSED ##########"
