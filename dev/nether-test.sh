#!/bin/bash
# Walk the test client into the Nether and look at a strider.
#
# dev/join-test.sh only ever sees the overworld, so nothing Nether-only has been
# seen alive: the strider, the zombified piglin, the magma cube. The server
# console is a TUI that only reads a real terminal, so this drives the server
# the way a player would instead -- the client sends chat commands, after
# `groups.toml` is edited so a joining player is an operator.
#
# Usage: bash dev/nether-test.sh [watch-seconds]
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

# Respect $CARGO_TARGET_DIR the way `cargo build` itself already does; see
# the same lines in dev/join-test.sh for what hardcoding it used to cost.
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
BIN="$TARGET_DIR/debug/foton"

PORT=25572
RUN_DIR="$ROOT/run-nether"
WATCH=${1:-20}

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
# Every joining player is an operator, which is what lets the client run the
# commands below. Only this throwaway config is touched.
sed -i 's/^default_groups = .*/default_groups = ["op"]/' "$RUN_DIR/config/groups.toml"
grep -q 'default_groups = \["op"\]' "$RUN_DIR/config/groups.toml" || {
  echo "COULD NOT MAKE THE TEST PLAYER AN OPERATOR"
  exit 1
}

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

# Cross into the Nether, then put striders on the ground next to the player.
# `execute in` is what a player would use; the teleport is the portal's job on
# a real server, and it exercises the same dimension-change path.
export JOIN_COMMANDS='execute in minecraft:the_nether run teleport @s 0 80 0;;summon minecraft:strider 2 80 0;;summon minecraft:strider -2 80 0;;summon minecraft:magma_cube 0 80 2;;summon minecraft:magma_cube 0 80 -2'
JOIN_WATCH_SECONDS=$WATCH python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== join.log ==="
cat join.log
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "summoned|changed world|unknown|incorrect|error|panic" | tail -15
if [ $STATUS -ne 0 ]; then
  echo "########## NETHER TEST FAILED (the client never settled) ##########"
  exit $STATUS
fi

# The point of the run: a real client, in the Nether, told about a strider.
if ! sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -q "changed world to minecraft:the_nether"; then
  echo "########## NETHER TEST FAILED (never crossed dimensions) ##########"
  exit 1
fi
for mob in strider magma_cube; do
  if ! grep -q "$mob x" join.log; then
    echo "########## NETHER TEST FAILED (the client was never told about a $mob) ##########"
    exit 1
  fi
done

echo "########## NETHER TEST PASSED ##########"
