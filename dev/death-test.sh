#!/bin/bash
# Read the line the server writes when a player dies, and count its arguments.
#
# `death.attack.mob` reads "%1$s was slain by %2$s". It used to be built with a
# single argument -- the victim -- so every death by a mob came out with the
# killer missing. Nothing about that is visible to a unit test on the message
# alone: the message has to be built from a real damage source, on a real death,
# and broadcast.
#
# The killer here is the victim, through `/damage ... by @s`. That is a strange
# sentence but it is the reliable one: a selector naming a summoned mob does not
# resolve on this server yet, and the point being measured is the argument
# count, not who the argument is.
#
# Usage: bash dev/death-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

# Respect $CARGO_TARGET_DIR the way `cargo build` itself already does; see
# the same lines in dev/join-test.sh for what hardcoding it used to cost.
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
BIN="$TARGET_DIR/debug/foton"

PORT=25579
RUN_DIR="$ROOT/run-death"

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

CMDS='gamemode survival'
CMDS="$CMDS;;teleport @s 0 100 0"
CMDS="$CMDS;;setblock 0 99 0 minecraft:stone"
CMDS="$CMDS;;damage @s 100 minecraft:mob_attack by @s"
CMDS="$CMDS;;!wait 2"

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=2 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

# The command that caused this carries no marker of its own, so the server's
# broadcast is the only line that mentions the message key.
DEATH_LINE=$(grep "server says" join.log | grep "death.attack.mob" | head -1)
echo "=== the death line ==="
echo "$DEATH_LINE"
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "\[Error\]|panic" | tail -5

fail() { echo "########## DEATH TEST FAILED ($1) ##########"; exit 1; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }
[ -n "$DEATH_LINE" ] || fail "the server never announced the death"

# One `text <name>` per argument the message was built with.
NAMES=$(printf '%s' "$DEATH_LINE" | grep -o "text SmokeTester" | wc -l)
if [ "$NAMES" -lt 2 ]; then
  fail "death.attack.mob was built with $NAMES argument(s); the killer is missing"
fi
echo "########## DEATH TEST PASSED ##########"
