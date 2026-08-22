#!/bin/bash
# Boot, save, shut down, and boot again on the same world.
#
# dev/join-test.sh always starts from an empty save directory, so it only ever
# proves the server can build a world -- never that it can pick one back up.
# Those are different code paths: the second boot reads region files, entity
# storage and level data that the first one wrote, and a break in any of them
# loses somebody's world rather than failing a build.
#
# Both shutdown kinds are covered. A clean stop is the ordinary case. A SIGKILL
# is the interesting one: it leaves whatever was mid-write on disk, which is
# what a crash or a killed container does, and it is the case an old note in
# join-test.sh suspected of hanging the next boot.
#
# Usage: bash dev/reload-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25567
RUN_DIR="$ROOT/run-reload"
WORLD_SEED=${WORLD_SEED:-8675309}

echo "=== Building ==="
if ! cargo build 2>&1 | tail -3; then
  echo "BUILD FAILED"
  exit 1
fi

# A config the scripted client can actually connect to, borrowed from the join
# test's if one is already there so this does not regenerate defaults.
rm -rf "$RUN_DIR"
mkdir -p "$RUN_DIR" || exit 1
if [ -d "$ROOT/run-offline/config" ]; then
  cp -r "$ROOT/run-offline/config" "$RUN_DIR/config"
else
  echo "RUN dev/join-test.sh FIRST so a config exists"
  exit 1
fi

sed -i "s/^server_port = .*/server_port = $PORT/" "$RUN_DIR/config/config.toml"
if grep -q '^seed = ' "$RUN_DIR/config/worlds.toml"; then
  sed -i "s/^seed = .*/seed = \"$WORLD_SEED\"/" "$RUN_DIR/config/worlds.toml"
else
  sed -i "/^save_path = /a seed = \"$WORLD_SEED\"" "$RUN_DIR/config/worlds.toml"
fi

cd "$RUN_DIR" || exit 1

wait_for_port() {
  for _ in $(seq 1 120); do
    ss -ltn 2>/dev/null | grep -q ":$PORT" && return 0
    sleep 1
  done
  return 1
}

# $1 = log suffix, $2 = "clean" or "kill"
boot_and_join() {
  local label=$1
  local shutdown=$2

  # stdin from /dev/null: the server reads console commands, and a background
  # process that reads a terminal is stopped by SIGTTIN instead of running.
  nohup "$ROOT/target/debug/steel" > "server-$label.log" 2>&1 < /dev/null &
  local pid=$!

  if ! wait_for_port; then
    echo "SERVER NEVER LISTENED ON $PORT ($label)"
    tail -30 "server-$label.log"
    kill -9 "$pid" 2>/dev/null
    return 1
  fi

  if ! python3 "$ROOT/dev/join.py" "$PORT" > "join-$label.log" 2>&1; then
    echo "JOIN FAILED ($label)"
    tail -20 "join-$label.log"
    kill -9 "$pid" 2>/dev/null
    return 1
  fi
  echo "  joined on the $label boot"

  if [ "$shutdown" = "kill" ]; then
    kill -9 "$pid" 2>/dev/null
  else
    kill "$pid" 2>/dev/null
    for _ in $(seq 1 30); do
      kill -0 "$pid" 2>/dev/null || break
      sleep 1
    done
    kill -0 "$pid" 2>/dev/null && kill -9 "$pid" 2>/dev/null
  fi
  sleep 2
  return 0
}

echo "=== First boot: builds the world, then stops cleanly ==="
boot_and_join first clean || exit 1
saved=$(grep -o 'Saved [0-9]* chunks' "server-first.log" | tail -1)
echo "  ${saved:-saved nothing}"
if [ ! -d saves ]; then
  echo "NO SAVE DIRECTORY WAS WRITTEN"
  exit 1
fi

echo "=== Second boot: reads that world back ==="
boot_and_join second kill || exit 1

echo "=== Third boot: reads a world left behind by a hard kill ==="
boot_and_join third clean || exit 1

echo "########## RELOAD TEST PASSED ##########"
