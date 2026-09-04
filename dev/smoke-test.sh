#!/bin/bash
# Build the server, boot it, speak the Minecraft handshake, then shut it down.
# Catches regressions that compile fine but break the running server.
# Usage: bash dev/smoke-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

# Respect $CARGO_TARGET_DIR the way `cargo build` itself already does; see
# the same lines in dev/join-test.sh for what hardcoding it used to cost.
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
BIN="$TARGET_DIR/debug/foton"

echo "=== Building ==="
cargo build 2>&1 | tail -3
# A pipeline's status is its last command's, so `if ! cargo build | tail`
# tested `tail` and never failed. That made the branch below unreachable: a
# broken build fell straight through and the test ran against a stale binary.
if [ "${PIPESTATUS[0]}" -ne 0 ]; then
  echo "BUILD FAILED"
  exit 1
fi

mkdir -p run && cd run || exit 1
rm -f server.log
echo "=== Booting ==="
nohup "$BIN" > server.log 2>&1 &
PID=$!

STATUS=1
for _ in $(seq 1 120); do
  if ! kill -0 "$PID" 2>/dev/null; then
    echo "SERVER DIED DURING STARTUP"
    tail -30 server.log
    exit 1
  fi
  if ss -ltn 2>/dev/null | grep -q ':25565'; then
    STATUS=0
    break
  fi
  sleep 1
done

if [ $STATUS -ne 0 ]; then
  echo "SERVER NEVER LISTENED ON 25565"
  kill "$PID" 2>/dev/null
  tail -30 server.log
  exit 1
fi

echo "=== Handshake ==="
python3 "$ROOT/dev/ping.py"
RC=$?

echo "=== Shutting down ==="
kill "$PID" 2>/dev/null
sleep 2
kill -0 "$PID" 2>/dev/null && kill -9 "$PID" 2>/dev/null

if [ $RC -eq 0 ]; then
  echo "########## SMOKE TEST PASSED ##########"
else
  echo "########## SMOKE TEST FAILED ##########"
fi
exit $RC
