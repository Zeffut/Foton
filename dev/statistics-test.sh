#!/bin/bash
# Count things with a real client on the wire and read the counters back.
#
# A statistic is server-side state no command reports, so `ClientboundAwardStats`
# is the only thing that says one moved -- and the client only gets it when it
# asks, which is what opening the statistics screen does.
#
# Both halves of a statistic travel as registry ids, so the ids this test greps
# for are looked up in the same extracted json the server generated its registry
# from. A hand-written id would be a second source of truth and would drift.
#
# Usage: bash dev/statistics-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25633
RUN_DIR="$ROOT/run-statistics"

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
# The anti-spam counter drains one point per game tick, and this test sends a
# burst of commands back to back.
sed -i 's/^command_spam_threshold_seconds = .*/command_spam_threshold_seconds = 0/' \
  "$RUN_DIR/config/config.toml"

# The two registry ids a statistic is made of, out of the extracted data.
stat_type_id() {
  python3 -c "import json,sys; d=json.load(open('$ROOT/steel-registry/build_assets/stat_types.json')); print(next(e['id'] for e in d if e['key']=='minecraft:'+sys.argv[1]))" "$1"
}
custom_stat_id() {
  python3 -c "import json,sys; d=json.load(open('$ROOT/steel-registry/build_assets/custom_stats.json')); print(next(e['id'] for e in d if e['key']=='minecraft:'+sys.argv[1]))" "$1"
}

CUSTOM=$(stat_type_id custom)
KILLED=$(stat_type_id killed)
MOB_KILLS=$(custom_stat_id mob_kills)
PLAYER_KILLS=$(custom_stat_id player_kills)
DEATHS=$(custom_stat_id deaths)
PLAY_TIME=$(custom_stat_id play_time)

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
CMDS="$CMDS;;difficulty normal"
CMDS="$CMDS;;gamerule spawn_mobs false"
CMDS="$CMDS;;gamerule advance_time false"
CMDS="$CMDS;;gamerule immediate_respawn true"
CMDS="$CMDS;;time set midnight"
# A statistics packet before anything has been counted. Whatever it holds, it
# must not hold a kill or a death: that is what makes the counts below mean
# "this run did it" rather than "it was already there".
CMDS="$CMDS;;!requeststats"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;tellraw @s {\"text\":\"STATMARKERONE\"}"
# `by @s` credits the kill to the player, which is what the counters key on.
CMDS="$CMDS;;summon minecraft:zombie ~ ~ ~4"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;damage @e[type=zombie,limit=1] 100 minecraft:generic by @s"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;kill @s"
CMDS="$CMDS;;!wait 2"
CMDS="$CMDS;;tellraw @s {\"text\":\"STATMARKERTWO\"}"
CMDS="$CMDS;;!requeststats"
CMDS="$CMDS;;!wait 2"

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=3 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what happened ==="
grep -E "statistic|server says" join.log | tail -30
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic" | tail -6

fail() { echo "########## STATISTICS TEST FAILED ($1) ##########"; exit 1; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }

MARKER_ONE=$(grep -n -- "server says: STATMARKERONE" join.log | head -1 | cut -d: -f1)
MARKER_TWO=$(grep -n -- "server says: STATMARKERTWO" join.log | head -1 | cut -d: -f1)
[ -n "$MARKER_ONE" ] && [ -n "$MARKER_TWO" ] \
  || fail "the ordering markers never came back, so nothing below can be trusted"

# 1. The screen is answered at all, and the tick counters are running: play_time
#    rises every tick a player is in the world, so a server that counted nothing
#    would send an empty packet or none.
FIRST_PLAY_TIME=$(grep -n -- "statistic $CUSTOM:$PLAY_TIME = " join.log | head -1 | cut -d: -f1)
[ -n "$FIRST_PLAY_TIME" ] || fail "play_time was never counted, so the tick counters are not running"
[ "$FIRST_PLAY_TIME" -lt "$MARKER_ONE" ] \
  || fail "the first statistics request went unanswered"

# 2. Before anything was killed, nothing had been killed. Without this the
#    counts below would pass on a server that handed out a kill on login.
BEFORE=$(sed -n "1,${MARKER_ONE}p" join.log)
echo "$BEFORE" | grep -q "statistic $CUSTOM:$MOB_KILLS = " \
  && fail "a mob kill was counted before anything was killed"
echo "$BEFORE" | grep -q "statistic $CUSTOM:$DEATHS = " \
  && fail "a death was counted before anything died"

# 3. Killing a zombie counts one mob kill and one zombie, and dying counts one
#    death. The counts are exact: a counter that fired twice per event fails.
AFTER=$(sed -n "${MARKER_TWO},\$p" join.log)
echo "$AFTER" | grep -q "statistic $CUSTOM:$MOB_KILLS = 1$" \
  || fail "killing a zombie did not count exactly one mob kill"
echo "$AFTER" | grep -q "statistic $CUSTOM:$DEATHS = 1$" \
  || fail "dying did not count exactly one death"
ZOMBIE=$(python3 -c "import json;d=json.load(open('$ROOT/steel-registry/build_assets/entities.json'));print([e['id'] for e in d if e['name']=='zombie'][0])" 2>/dev/null)
if [ -n "$ZOMBIE" ]; then
  echo "$AFTER" | grep -q "statistic $KILLED:$ZOMBIE = 1$" \
    || fail "killing a zombie did not count against the zombie entity type"
fi

# 4. A zombie is not a player. The two counters live in the same registry and
#    differ only by their value id, so a counter that ignored which one it was
#    told to raise would trip here.
echo "$AFTER" | grep -q "statistic $CUSTOM:$PLAYER_KILLS = " \
  && fail "killing a zombie counted as a player kill"

echo "########## STATISTICS TEST PASSED ##########"
