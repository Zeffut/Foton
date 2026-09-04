#!/bin/bash
# Put a player next to the three blocks that make mobs, and watch them work.
#
# A monster spawner is the one block whose whole job is invisible from a
# command: it has no comparator output and no inventory. What it does have is
# an entity stream, so it is asserted on what the client is told appeared --
# and it is pointed at a magma cube on purpose, because nothing in the
# overworld spawns one, so a sighting is the spawner's doing and nobody else's.
#
# The trial spawner and the vault both carry their state machine in a block
# state property, which `execute if block` reads at any moment. That makes them
# the exact half of this test: `waiting_for_players -> active` and
# `inactive -> active` are read straight off the block, and a `tellraw` behind
# the condition turns each into one line of chat.
#
# The trial spawner half runs in survival on purpose. Vanilla's
# `PlayerDetector.NO_CREATIVE_PLAYERS` is what a trial spawner watches with, so
# a creative player is invisible to it -- a run that did this in creative would
# sit at `waiting_for_players` forever and look like a bug. The vault uses
# `INCLUDING_CREATIVE_PLAYERS` and does not care.
#
# Usage: bash dev/spawner-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

# Respect $CARGO_TARGET_DIR the way `cargo build` itself already does; see
# the same lines in dev/join-test.sh for what hardcoding it used to cost.
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
BIN="$TARGET_DIR/debug/foton"

PORT=25602
RUN_DIR="$ROOT/run-spawner"

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

CMDS='gamemode creative'
# A magma cube refuses to spawn on peaceful, and so does a trial spawner.
CMDS="$CMDS;;difficulty normal"
# One throwaway command first: the very first `setblock` of a run can land
# before the chunk around the player is ready.
CMDS="$CMDS;;time set midnight"
# Something to stand on: the rest of this happens a hundred blocks up.
CMDS="$CMDS;;setblock 0 99 3 minecraft:stone"

# --- the trial spawner ---------------------------------------------------
# Nothing points a fresh trial spawner at a mob, so a spawn egg does it: that
# is the `Spawner` interface, and it drives `overrideEntityToSpawn` too.
CMDS="$CMDS;;setblock 0 100 6 minecraft:trial_spawner"
CMDS="$CMDS;;teleport @s 0 100 3"
CMDS="$CMDS;;give @s minecraft:zombie_spawn_egg 1"
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;!useon 0 100 6 south"
CMDS="$CMDS;;!wait 2"
# It only counts players it could be surprised by, so drop out of creative.
CMDS="$CMDS;;gamemode survival"
CMDS="$CMDS;;!wait 4"
CMDS="$CMDS;;execute if block 0 100 6 minecraft:trial_spawner[trial_spawner_state=active] run tellraw @s {\"text\":\"TRIALSPAWNER_ACTIVE\"}"
CMDS="$CMDS;;gamemode creative"

# --- the monster spawner -------------------------------------------------
# Twenty ticks between attempts and a spawn range of two, so every candidate
# position is inside the pocket of air the player is standing in. The nearby
# cap is vanilla's six, which is what stops the test flooding the world.
SPAWNER_NBT='{SpawnData:{entity:{id:"minecraft:magma_cube"}},Delay:20,MinSpawnDelay:20,MaxSpawnDelay:20,SpawnCount:2,MaxNearbyEntities:6,RequiredPlayerRange:16,SpawnRange:2}'
CMDS="$CMDS;;setblock 0 100 0 minecraft:spawner$SPAWNER_NBT"
CMDS="$CMDS;;!wait 5"

# --- the vault -----------------------------------------------------------
# Four blocks is the activation range, measured block position to block
# position, so standing one block away is inside it and eight blocks away is
# not.
CMDS="$CMDS;;setblock 0 100 -6 minecraft:vault"
CMDS="$CMDS;;teleport @s 0 100 -5"
CMDS="$CMDS;;!wait 3"
CMDS="$CMDS;;execute if block 0 100 -6 minecraft:vault[vault_state=active] run tellraw @s {\"text\":\"VAULT_ACTIVE\"}"
CMDS="$CMDS;;teleport @s 0 100 3"
CMDS="$CMDS;;!wait 3"
CMDS="$CMDS;;execute if block 0 100 -6 minecraft:vault[vault_state=inactive] run tellraw @s {\"text\":\"VAULT_WENT_QUIET\"}"

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=8 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what happened ==="
grep -E "server says|before the commands|spawned around the player" join.log
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic" | tail -5

fail() { echo "########## SPAWNER TEST FAILED ($1) ##########"; exit 1; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }

# Each marker has to arrive as chat. Grepping the whole log would also match
# the echo of the command that asks the question, which is printed whether the
# condition held or not.
said() { grep -q "server says: $1" join.log; }

grep -qE "(before the commands|spawned around the player):.*\bmagma_cube x" join.log \
  || fail "the monster spawner never produced a magma cube"
said TRIALSPAWNER_ACTIVE \
  || fail "the trial spawner never reached its active state"
said VAULT_ACTIVE \
  || fail "the vault never woke up for a player standing next to it"
said VAULT_WENT_QUIET \
  || fail "the vault stayed awake after the player walked away"

echo "########## SPAWNER TEST PASSED ##########"
