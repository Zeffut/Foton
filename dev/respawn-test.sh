#!/bin/bash
# Die, press Respawn, and prove the player comes back alive and still connected.
#
# This is the first thing a player does after their first mistake, and nothing
# covered it. `dev/death-test.sh` reads the message the server broadcasts and
# stops there; the death screen's own packet -- `ServerboundClientCommandPacket`
# with `PERFORM_RESPAWN` -- had no test and no way for a script to send it.
#
# The trap the whole test is built around: a server can drop a player by simply
# closing the socket, with no disconnect packet and nothing in its log. Every
# read in `dev/join.py` used to treat that as an ordinary quiet moment, so the
# run stayed green while the player was gone. `!alive` is the assertion that
# tells the two apart, and it runs after the respawn rather than before.
#
# Usage: bash dev/respawn-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25812
RUN_DIR="$ROOT/run-respawn"

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
  # Let the server write its own defaults rather than depending on another test
  # having run first. stdin has to come from /dev/null: the console is a TUI and
  # a background process that reads a terminal is stopped by SIGTTIN.
  echo "=== Generating an offline config ==="
  nohup "$ROOT/target/debug/foton" > /dev/null 2>&1 < /dev/null &
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
  config/config.toml
sed -i 's/^default_groups = .*/default_groups = ["op"]/' config/groups.toml
# The anti-spam counter drains one point per game tick, and this test sends its
# commands faster than that.
if grep -q '^command_spam_threshold_seconds' config/config.toml; then
  sed -i 's/^command_spam_threshold_seconds = .*/command_spam_threshold_seconds = 0/' config/config.toml
fi
rm -rf saves

nohup "$ROOT/target/debug/foton" > server.log 2>&1 < /dev/null &
PID=$!
cleanup() {
  kill "$PID" 2>/dev/null
  for _ in $(seq 1 30); do kill -0 "$PID" 2>/dev/null || break; sleep 1; done
  kill -9 "$PID" 2>/dev/null
}

for _ in $(seq 1 240); do
  ss -ltn 2>/dev/null | grep -q ":$PORT" && break
  sleep 1
done
if ! ss -ltn 2>/dev/null | grep -q ":$PORT"; then
  echo "SERVER NEVER LISTENED ON $PORT"
  sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | tail -20
  cleanup; exit 1
fi

# Survival, because a creative player cannot be killed by damage and a
# spectator never sees a death screen.
CMDS='gamemode survival'
CMDS="$CMDS;;teleport @s 0 100 0"
CMDS="$CMDS;;setblock 0 99 0 minecraft:stone"
CMDS="$CMDS;;!wait 2"
CMDS="$CMDS;;damage @s 10000 minecraft:mob_attack by @s"
CMDS="$CMDS;;!wait 2"
CMDS="$CMDS;;!sawdeathscreen"
CMDS="$CMDS;;!respawn"
CMDS="$CMDS;;!wait 3"
CMDS="$CMDS;;!alive"
CMDS="$CMDS;;!sawrespawn"
# A player who is back in the world answers a command; one the server has let go
# of answers nothing. The marker goes through `tellraw` so the assertion can
# require the server's own echo of it rather than the client's.
CMDS="$CMDS;;tellraw @s {\"text\":\"RESPAWNED_AND_PLAYING\"}"
CMDS="$CMDS;;!wait 2"
CMDS="$CMDS;;!alive"

export JOIN_COMMANDS="$CMDS"
python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== client ==="
tail -30 join.log
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "\[Error\]|\[Warn\]|panic" | tail -10

fail() { echo "########## RESPAWN TEST FAILED ($1) ##########"; exit 1; }

grep -q "the death screen opened" join.log || fail "the player never died"
grep -q "the server dropped the connection" join.log \
  && fail "the server hung up on the player"
grep -q "the client was respawned" join.log || fail "no respawn packet ever arrived"
[ $STATUS -eq 0 ] || fail "the client did not survive the respawn"
grep -q "server says: RESPAWNED_AND_PLAYING" join.log \
  || fail "the respawned player no longer answers commands"

# A respawned player is only back if their health came back too: the server
# sends health 0 with the death screen and full health with the respawn, and a
# respawn that left them dead would keep the death screen up forever.
HEALTH=$(grep -o "health is now [0-9.]*" join.log | tail -1 | awk '{print $4}')
echo "=== final health: $HEALTH ==="
[ -n "$HEALTH" ] || fail "the server never told the client its health"
awk -v h="$HEALTH" 'BEGIN { exit !(h > 0) }' \
  || fail "the player respawned still dead, at $HEALTH health"

echo "########## RESPAWN TEST PASSED ##########"
