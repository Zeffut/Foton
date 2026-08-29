#!/bin/bash
# Boot an offline-mode server and have a real client join the world.
#
# dev/smoke-test.sh only proves the server answers a status ping. This walks the
# login and configuration states and waits for the play login packet, so a break
# anywhere in the join pipeline fails the build instead of shipping.
#
# Usage: bash dev/join-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25566
RUN_DIR="$ROOT/run-offline"

# Any fixed value works; this one is a plains start with water and a village in
# range, which exercises terrain, structures and both spawn paths at once.
WORLD_SEED=${WORLD_SEED:-8675309}

echo "=== Building ==="
if ! cargo build 2>&1 | tail -3; then
  echo "BUILD FAILED"
  exit 1
fi

# A dedicated run directory: the join client cannot authenticate with Mojang, and
# the default config is online mode on the default port.
mkdir -p "$RUN_DIR/config" || exit 1
cd "$RUN_DIR" || exit 1
rm -f server.log

if [ ! -f config/config.toml ]; then
  echo "=== Generating an offline config ==="
  # Let the server write its defaults, then turn off what a scripted client
  # cannot satisfy. stdin has to come from /dev/null: the server reads console
  # commands, and a background process that reads a terminal is stopped by
  # SIGTTIN instead of running.
  nohup "$ROOT/target/debug/foton" > /dev/null 2>&1 < /dev/null &
  GEN_PID=$!
  for _ in $(seq 1 60); do
    [ -f config/config.toml ] && break
    sleep 1
  done
  kill "$GEN_PID" 2>/dev/null
  sleep 2
  kill -0 "$GEN_PID" 2>/dev/null && kill -9 "$GEN_PID" 2>/dev/null
  if [ ! -f config/config.toml ]; then
    echo "SERVER NEVER WROTE A CONFIG"
    exit 1
  fi
fi

sed -i \
  -e 's/^online_mode = .*/online_mode = false/' \
  -e 's/^encryption = .*/encryption = false/' \
  -e 's/^enforce_secure_chat = .*/enforce_secure_chat = false/' \
  -e "s/^server_port = .*/server_port = $PORT/" \
  config/config.toml

# Pin the seed. Without it every run generates different terrain, so what the
# client sees around it -- chunks, structures, the animals a chunk was stocked
# with -- changes from run to run and the test can only say "it did not crash".
# Pinned, two runs of the same build are comparable, and a run that suddenly
# stops showing livestock means the server changed rather than the world did.
if grep -q '^seed = ' config/worlds.toml; then
  sed -i "s/^seed = .*/seed = \"$WORLD_SEED\"/" config/worlds.toml
else
  sed -i "/^save_path = /a seed = \"$WORLD_SEED\"" config/worlds.toml
fi

# Start from a clean world every time, so the test measures the server and not
# whatever a previous run left behind.
#
# One run against a reused world once hung with the log frozen on "Chunk
# scheduling epoch slow" and the client still waiting. That has never
# reproduced: not from a SIGKILL mid-generation, and not from the clean stop,
# hard kill and reload that dev/reload-test.sh now walks through on every run.
# Reloading is covered there; wiping here keeps this test measuring one thing.
rm -rf saves

echo "=== Booting (offline, port $PORT) ==="
nohup "$ROOT/target/debug/foton" > server.log 2>&1 < /dev/null &
PID=$!

STATUS=1
for _ in $(seq 1 120); do
  if ! kill -0 "$PID" 2>/dev/null; then
    echo "SERVER DIED DURING STARTUP"
    tail -30 server.log
    exit 1
  fi
  if ss -ltn 2>/dev/null | grep -q ":$PORT"; then
    STATUS=0
    break
  fi
  sleep 1
done

if [ $STATUS -ne 0 ]; then
  echo "SERVER NEVER LISTENED ON $PORT"
  kill "$PID" 2>/dev/null
  tail -30 server.log
  exit 1
fi

echo "=== Joining ==="
python3 "$ROOT/dev/join.py" "$PORT"
RC=$?

echo "=== Shutting down ==="
kill "$PID" 2>/dev/null
sleep 2
kill -0 "$PID" 2>/dev/null && kill -9 "$PID" 2>/dev/null

if [ $RC -ne 0 ]; then
  echo "--- server log tail ---"
  tail -30 server.log
  echo "########## JOIN TEST FAILED ##########"
  exit $RC
fi

echo "########## JOIN TEST PASSED ##########"
