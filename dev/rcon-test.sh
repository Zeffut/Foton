#!/bin/bash
# Drive the server the way a remote administrator does, and read the answers.
#
# Rcon is the one interface with no player attached: whatever a command prints
# has to come back down the same socket or the operator is working blind. That
# is what this asserts -- not that the port accepts a connection, but that a
# `seed` comes back with the seed in it and a typo comes back as the error.
#
# The password is checked from both sides. A wrong one and an empty one both
# have to be refused with vanilla's request id of -1, and a command sent before
# any password at all has to be refused too, or the port is an open shell.
#
# Usage: bash dev/rcon-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25704
RCON_PORT=25705
RCON_PASSWORD=steel-rcon-test
RUN_DIR="$ROOT/run-rcon"

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
sed -i 's/^command_spam_threshold_seconds = .*/command_spam_threshold_seconds = 0/' \
  "$RUN_DIR/config/config.toml"
# A TOML table header is an absolute path, so appending this at the end of the
# file puts it under [server] wherever it lands. Strip any [server.rcon] the
# reference config already carries first: a freshly generated config has one
# now that the server speaks RCON, and two of them is a `duplicate key` the
# server refuses to start on.
python3 "$ROOT/dev/strip-rcon-section.py" "$RUN_DIR/config/config.toml"
cat >> "$RUN_DIR/config/config.toml" <<TOML

[server.rcon]
enable = true
port = $RCON_PORT
password = "$RCON_PASSWORD"
TOML

cd "$RUN_DIR" || exit 1
nohup "$ROOT/target/debug/steel" > server.log 2>&1 < /dev/null &
PID=$!
cleanup() {
  kill "$PID" 2>/dev/null
  for _ in $(seq 1 30); do kill -0 "$PID" 2>/dev/null || break; sleep 1; done
  kill -9 "$PID" 2>/dev/null
}

for _ in $(seq 1 180); do
  ss -ltn 2>/dev/null | grep -q ":$RCON_PORT" && break
  sleep 1
done
if ! ss -ltn 2>/dev/null | grep -q ":$RCON_PORT"; then
  echo "SERVER NEVER LISTENED ON RCON PORT $RCON_PORT"
  sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | tail -20
  cleanup; exit 1
fi

python3 "$ROOT/dev/rcon.py" "$RCON_PORT" "$RCON_PASSWORD" > rcon.log 2>&1
STATUS=$?

cleanup

echo "=== transcript ==="
cat rcon.log
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic|rcon" | tail -10

fail() { echo "########## RCON TEST FAILED ($1) ##########"; exit 1; }

[ $STATUS -eq 0 ] || fail "the rcon client never finished"
grep -q RCON_DONE rcon.log || fail "the rcon client never finished"

# `id=-1` is how vanilla says no, and it is what a client reads to know.
event() { grep -m1 "^RCON_EVENT $1 " rcon.log; }

case "$(event preauth)" in
  *"id=-1 kind=2"*) ;;
  *) fail "a command sent before the password was not refused" ;;
esac
case "$(event badpass)" in
  *"id=-1 kind=2"*) ;;
  *) fail "the wrong password was not refused" ;;
esac
case "$(event emptypass)" in
  *"id=-1 kind=2"*) ;;
  *) fail "an empty password was not refused" ;;
esac
case "$(event auth)" in
  *"id=4711 kind=2"*) ;;
  *) fail "the right password did not authenticate" ;;
esac

# The point of the whole exercise: output comes back.
case "$(event seed)" in
  *8675309*) ;;
  *) fail "/seed came back without the seed in it" ;;
esac
# A failure is output too. An empty body here is the old behaviour, where
# every message was dropped and the client could not tell a typo from a
# success.
case "$(event bogus)" in
  *definitelynotacommand*) ;;
  *) fail "a rejected command came back with nothing in it" ;;
esac
# Still answering after the first command, on the same connection.
case "$(event list)" in
  *"id=23 kind=0"*) ;;
  *) fail "the second command on a connection went unanswered" ;;
esac
if [ -z "$(event list | sed -e "s/.*body='//" -e "s/'$//")" ]; then
  fail "/list came back empty"
fi

echo "########## RCON TEST PASSED ##########"
