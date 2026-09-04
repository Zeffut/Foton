#!/bin/bash
# Respawn from far away, at a wide view distance, over and over.
#
# The panic this covers is a race, so one green run proves nothing: the count is
# the assertion. `RESPAWN_CHURN_ROUNDS` sets it.
#
#   421.34s [Error] Thread 'chunk-worker' has panicked at
#                   foton-core/src/chunk/chunk_generation_task.rs:157
#                   The chunkholder should be created by distance manager
#                   before the generation task is scheduled.
#
# `ChunkGenerationTask::new` demands a live holder for every chunk in a square
# of `worst_case_radius` around its target, but the scheduling epoch only
# guarantees one for the center. A neighbor whose revival was deferred --
# `ChunkMap::update_chunk_level` returns `None` and parks it in
# `deferred_revivals` when `try_revive_from_unloading` loses to an in-flight
# save -- is absent from `chunks` until the next epoch, and the task built in
# this one walks straight into the gap.
#
# Dying far from spawn is what makes it likely: the respawn drops every ticket
# around the death site and raises a new set around spawn in the same breath, so
# a whole ring is unloading while another is being revived. A wide view distance
# multiplies the ring, which is why the owner saw it at 32 and not at 16.
#
# The server dies under the player rather than disconnecting them, so the
# assertion is on the server log as much as on the client: a panicking server
# hangs up without ever sending a disconnect packet, which is why `!alive` in
# dev/join.py had to learn to notice a bare socket close.
#
# Usage: bash dev/respawn-churn-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

# Respect $CARGO_TARGET_DIR the way `cargo build` itself already does; see
# the same lines in dev/join-test.sh for what hardcoding it used to cost.
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
BIN="$TARGET_DIR/debug/foton"

PORT=25814
RUN_DIR="$ROOT/run-respawn-churn"
ROUNDS=${RESPAWN_CHURN_ROUNDS:-6}
VIEW_DISTANCE=${RESPAWN_CHURN_VIEW_DISTANCE:-32}

echo "=== Building ==="
if ! cargo build 2>&1 | tail -2; then
  echo "BUILD FAILED"
  exit 1
fi

rm -rf "$RUN_DIR"
mkdir -p "$RUN_DIR/config" || exit 1
cd "$RUN_DIR" || exit 1

if [ -d "$ROOT/run-offline/config" ]; then
  cp -r "$ROOT/run-offline/config/." config/
else
  echo "=== Generating an offline config ==="
  nohup "$BIN" > /dev/null 2>&1 < /dev/null &
  GEN_PID=$!
  for _ in $(seq 1 120); do
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
  -e "s/^view_distance = .*/view_distance = $VIEW_DISTANCE/" \
  -e "s/^simulation_distance = .*/simulation_distance = $VIEW_DISTANCE/" \
  config/config.toml
sed -i 's/^default_groups = .*/default_groups = ["op"]/' config/groups.toml
if grep -q '^command_spam_threshold_seconds' config/config.toml; then
  sed -i 's/^command_spam_threshold_seconds = .*/command_spam_threshold_seconds = 0/' config/config.toml
fi
rm -rf saves

nohup "$BIN" > server.log 2>&1 < /dev/null &
PID=$!
cleanup() {
  kill "$PID" 2>/dev/null
  for _ in $(seq 1 30); do kill -0 "$PID" 2>/dev/null || break; sleep 1; done
  kill -9 "$PID" 2>/dev/null
}

for _ in $(seq 1 300); do
  ss -ltn 2>/dev/null | grep -q ":$PORT" && break
  sleep 1
done
if ! ss -ltn 2>/dev/null | grep -q ":$PORT"; then
  echo "SERVER NEVER LISTENED ON $PORT"
  sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | tail -20
  cleanup; exit 1
fi

CMDS='gamemode survival'
# Each round walks the player somewhere new, lets the view distance pull that
# whole area in, then kills them so the respawn tears it all down at once while
# raising the spawn area again. A fresh coordinate every round keeps the server
# generating rather than replaying chunks it already has.
# The hole only opens while a chunk is being saved on its way out and asked
# back in before the save lets go: `try_revive_from_unloading` loses the
# compare-exchange, `update_chunk_level` returns `None`, and the position stays
# out of `chunks` until the next epoch. So each round leaves the spawn area just
# long enough for it to start unloading and saving, and then comes straight back
# through a respawn. Waiting longer would let the saves finish and close the
# window, which is why the settles here are deliberately short.
for round in $(seq 1 "$ROUNDS"); do
  far=$((round * 700 + 600))
  CMDS="$CMDS;;teleport @s $far 120 $far"
  CMDS="$CMDS;;!wait 2"
  CMDS="$CMDS;;damage @s 10000 minecraft:mob_attack by @s"
  CMDS="$CMDS;;!respawn"
  CMDS="$CMDS;;!wait 3"
  CMDS="$CMDS;;!alive"
  CMDS="$CMDS;;tellraw @s {\"text\":\"CHURN_ROUND_${round}_SURVIVED\"}"
done

export JOIN_COMMANDS="$CMDS"
JOIN_COMMAND_SETTLE_SECONDS=1.0 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

# The panic kills a worker; the server then stops. Give it a moment to write
# the lines out before reading them.
sleep 3
SERVER_ALIVE=no
kill -0 "$PID" 2>/dev/null && SERVER_ALIVE=yes
cleanup

CLEAN_LOG=$(sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log)

echo "=== client tail ==="
tail -20 join.log
echo "=== server panics ==="
printf '%s\n' "$CLEAN_LOG" | grep -iE "panicked|FATAL ERROR" | head -10
echo "=== server still up after $ROUNDS rounds: $SERVER_ALIVE ==="

fail() { echo "########## RESPAWN CHURN TEST FAILED ($1) ##########"; exit 1; }

printf '%s\n' "$CLEAN_LOG" | grep -qiE "panicked|FATAL ERROR" \
  && fail "the server panicked during a respawn"
[ "$SERVER_ALIVE" = yes ] || fail "the server was gone by the end of the run"
grep -q "the server dropped the connection" join.log \
  && fail "the server hung up on the player"

SURVIVED=$(grep -c "server says: CHURN_ROUND_[0-9]*_SURVIVED" join.log)
echo "=== rounds survived: $SURVIVED of $ROUNDS ==="
[ "$SURVIVED" -eq "$ROUNDS" ] \
  || fail "only $SURVIVED of $ROUNDS respawns left the player playing"
[ $STATUS -eq 0 ] || fail "the client did not survive the run"

echo "########## RESPAWN CHURN TEST PASSED ($ROUNDS rounds) ##########"
