#!/bin/bash
# Boot a Bedrock-enabled server, sharing the Java port, and prove the chain
# end to end: Foton starts and supervises a real Geyser, Geyser really binds
# the same port number the Java listener uses, and a Floodgate identity
# really reaches the world with the configured prefix and survives a
# reconnect with the same UUID.
#
# Two clients drive this, both in dev/bedrock-client.js:
#   - `join`: a simulated Bedrock client (bedrock-protocol, offline mode)
#     aimed at Geyser on the shared port. It cannot reach the world without a
#     real Xbox account -- Geyser's own `validate-bedrock-login` is correctly
#     hardcoded on in production (`foton-bedrock/src/geyser.rs`) and rejects
#     an unauthenticated client before any Java connection to Foton is even
#     attempted. This is not a Foton defect; see the module comment in
#     dev/bedrock-client.js and design/bedrock-stage0-findings.md's Step 4.
#     This part of the test still proves the Bedrock side of the shared port
#     is alive and correctly gated -- when Geyser starts at all (see below).
#   - `floodgate`: a raw Java client that crafts and encrypts a Floodgate
#     handshake itself, using the exact wire format Geyser uses and the real
#     shared key this run generated. This is what a real Geyser, fed a real
#     Xbox-authenticated Bedrock player, sends to Foton on the Java side --
#     so this is what actually exercises identity derivation, the username
#     prefix, and UUID persistence, against real production code, and it
#     needs no Geyser process at all: it talks to Foton's Java port directly.
#
# The server under test runs `online_mode = true, encryption = true` --
# Foton's own generated default, and the production-shaped configuration.
# Every prior run of this script, and every manual run recorded in
# design/bedrock-stage0-findings.md, used `online_mode = false` instead,
# which is not a configuration any public server should run and cannot catch
# a Floodgate regression: the whole point of Floodgate is to carry a verified
# identity on a server that does NOT trust client-claimed usernames. A traced
# reading of foton-login/src/handlers/login.rs says the Floodgate branch in
# `handle_hello` returns before the `self.server.config.encryption` check, so
# online_mode and encryption should never matter to it -- but that was a
# traced conclusion, never an observed one, until this script ran it.
#
# There is deliberately no separate offline-mode run left in this script.
# `resolve_floodgate_login` (foton-login/src/floodgate.rs) and the branch in
# `handle_hello` that calls it never read `online_mode` or `encryption` at
# all, so an offline-mode run exercises nothing on the Floodgate path that
# this online-mode run does not already cover, at roughly double the
# wall-clock cost (each run boots two processes and drives two clients over
# ~1-2 minutes). If that tracing is ever wrong, this run is exactly the one
# that would catch it; a parallel offline run would not add coverage, only
# time. The `join` sub-test's outcome (rejected by Geyser's own Xbox gate)
# is unaffected by this switch either way -- see its own comment below.
#
# What `online_mode = true` does NOT introduce: a live dependency on Mojang's
# session server. The Floodgate handshake short-circuits before
# `mojang_authenticate` is ever called, and the `join` client never reaches
# Foton at all (Geyser's own gate rejects it first, same as before). Foton's
# own Minecraft-services-key fetch at boot happens unconditionally regardless
# of `online_mode`, so that is not a new network dependency introduced here.
#
# Usage: bash dev/bedrock-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

# Respect $CARGO_TARGET_DIR the way `cargo build` itself already does --
# dev/join-test.sh did not, and Stage 0 had to work around it by hand.
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
BIN="$TARGET_DIR/debug/foton"

PORT=25610
RUN_DIR="$ROOT/run-bedrock"
BEDROCK_USERNAME="StageZero"
BEDROCK_XUID="2535428478404012"
PREFIX="."

FAILED=0
note_failure() {
  echo "FAILURE: $1"
  FAILED=1
}

# Foton's console renderer never checks whether stdout is a real terminal --
# server.log (a plain `> server.log` redirect of that stdout) ends up a raw
# terminal transcript: ANSI colour/cursor escapes (`\x1b[...`) interleaved
# with the log text, AND a literal `\r` before most newlines (the redraw of
# the interactive input prompt). That trailing `\r` sits *after* the last
# visible character on a line, so any regex anchored with `$` -- like the
# port-number extraction below -- matches nothing: the true last byte before
# the `\n` is `\r`, not a digit. Confirmed against a real captured
# server.log: `grep -oE '[0-9]+$'` on an unstripped line returns empty.
strip_ansi() {
  sed -E 's/\x1b\[[0-9;]*[A-Za-z]//g' | tr -d '\r'
}

# --- skip cleanly on a machine that legitimately cannot run this ----------

if ! command -v node >/dev/null 2>&1; then
  echo "SKIP: node not found -- cannot run the Bedrock end-to-end test"
  exit 0
fi

JAVA_BIN=$(command -v java || true)
if [ -z "$JAVA_BIN" ]; then
  echo "SKIP: no java on PATH -- cannot run the Bedrock end-to-end test"
  exit 0
fi

JAVA_VERSION_STRING=$("$JAVA_BIN" -version 2>&1 | head -1 | sed -n 's/.*"\([^"]*\)".*/\1/p')
JAVA_FIRST=${JAVA_VERSION_STRING%%.*}
if [ "$JAVA_FIRST" = "1" ]; then
  JAVA_REST=${JAVA_VERSION_STRING#*.}
  JAVA_MAJOR=${JAVA_REST%%.*}
else
  JAVA_MAJOR=$JAVA_FIRST
fi
case "$JAVA_MAJOR" in
  ''|*[!0-9]*)
    echo "SKIP: could not read a Java version from '$JAVA_VERSION_STRING' -- cannot run the Bedrock end-to-end test"
    exit 0
    ;;
esac
if [ "$JAVA_MAJOR" -lt 21 ]; then
  echo "SKIP: Java $JAVA_MAJOR found, Geyser needs 21+ -- cannot run the Bedrock end-to-end test"
  exit 0
fi

BEDROCK_PROTOCOL_AVAILABLE=1
if ! node -e "require.resolve('bedrock-protocol')" >/dev/null 2>&1; then
  BEDROCK_PROTOCOL_AVAILABLE=0
fi

echo "=== Building ==="
if ! cargo build 2>&1 | tail -3; then
  echo "BUILD FAILED"
  exit 1
fi
if [ ! -x "$BIN" ]; then
  echo "BUILD DID NOT PRODUCE $BIN"
  exit 1
fi

mkdir -p "$RUN_DIR/config" || exit 1
cd "$RUN_DIR" || exit 1
rm -f server.log

if [ ! -f config/config.toml ]; then
  echo "=== Generating a default config ==="
  nohup "$BIN" > /dev/null 2>&1 < /dev/null &
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

# Production-shaped: online_mode and encryption both on (Foton's own
# generated default, and the pairing its config validator requires -- see
# "encryption must be true when online_mode is enabled" in
# foton-core/src/config.rs). enforce_secure_chat stays off: it is also off by
# default, and turning it on is an orthogonal, documented operator trade-off
# (foton/src/lib.rs's warn_about_risky_bedrock_config warns about it) that
# has nothing to do with the question this run exists to answer.
sed -i \
  -e 's/^online_mode = .*/online_mode = true/' \
  -e 's/^encryption = .*/encryption = true/' \
  -e 's/^enforce_secure_chat = .*/enforce_secure_chat = false/' \
  -e "s/^server_port = .*/server_port = $PORT/" \
  config/config.toml

if ! grep -q '^online_mode = true' config/config.toml || ! grep -q '^encryption = true' config/config.toml; then
  echo "COULD NOT SET online_mode/encryption TO true IN THE GENERATED CONFIG"
  exit 1
fi

# Enable Bedrock, on the *default* shared port (0 -- follows server_port) and
# the *default* jar (no jar_path override). This is the main test path, per
# the plan: sharing one port number, and Foton fetching its own Geyser, are
# both parts of the claim under test, so nothing here overrides either.
sed -i '/^\[server\.bedrock\]/,/^\[/{s/^enable = false/enable = true/}' config/config.toml

if ! grep -q '^enable = true' <(sed -n '/^\[server\.bedrock\]/,/^\[/p' config/config.toml); then
  echo "COULD NOT ENABLE BEDROCK IN THE GENERATED CONFIG"
  exit 1
fi

PID=""
cleanup() {
  # Both processes: SIGTERM to Foton first, so its own supervisor gets the
  # chance to stop Geyser itself (its shutdown path already waits up to
  # TERMINATE_GRACE for exactly that). Geyser's pid is captured *before* that,
  # in case Foton has to be force-killed and never gets to run its own
  # shutdown path -- a hard failure here must not orphan a JVM still holding
  # the port. The jar's argument is relative (a separate, real finding --
  # see the report), so a path-anchored pkill would not match it; a name
  # match on the jar itself is the reliable fallback.
  local geyser_pid=""
  if [ -n "$PID" ] && kill -0 "$PID" 2>/dev/null; then
    geyser_pid=$(pgrep -P "$PID" 2>/dev/null | head -1)
    kill "$PID" 2>/dev/null
    for _ in $(seq 1 15); do
      kill -0 "$PID" 2>/dev/null || break
      sleep 1
    done
    kill -0 "$PID" 2>/dev/null && kill -9 "$PID" 2>/dev/null
  fi
  if [ -n "$geyser_pid" ] && kill -0 "$geyser_pid" 2>/dev/null; then
    kill -9 "$geyser_pid" 2>/dev/null
  fi
  pkill -f "bedrock/Geyser-Standalone.jar" 2>/dev/null
}
trap cleanup EXIT

echo "=== Booting (Bedrock enabled, shared port $PORT, online_mode=true, encryption=true) ==="
rm -rf saves bedrock
nohup "$BIN" > server.log 2>&1 < /dev/null &
PID=$!

STATUS=1
for _ in $(seq 1 120); do
  if ! kill -0 "$PID" 2>/dev/null; then
    echo "SERVER DIED DURING STARTUP"
    tail -40 server.log
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
  tail -40 server.log
  exit 1
fi

echo "=== Waiting for Geyser to start, or to give up (relayed through Foton's own log) ==="
GEYSER_STARTED=0
for _ in $(seq 1 90); do
  if grep -q "Started Geyser on UDP port" server.log 2>/dev/null; then
    GEYSER_STARTED=1
    break
  fi
  # The supervisor gives up after MAX_CONSECUTIVE_FAILURES retries -- no
  # point waiting out the rest of the timeout once it has.
  if grep -q "not restarting it again" server.log 2>/dev/null; then
    break
  fi
  if ! kill -0 "$PID" 2>/dev/null; then
    break
  fi
  sleep 1
done

if [ "$GEYSER_STARTED" -eq 1 ]; then
  GEYSER_STARTUP_LINE=$(grep "Started Geyser on UDP port" server.log | strip_ansi | tail -1)
  echo "  $GEYSER_STARTUP_LINE"
else
  note_failure "Geyser never started (log relay is working -- it shows Geyser crash-looping and giving up, or nothing at all)"
  echo "--- geyser-tagged lines from server.log ---"
  grep -i "geyser\|jarfile" server.log | tail -20
fi

# --- Assertion 1: the shared port (only meaningful once Geyser is up) -----

if [ "$GEYSER_STARTED" -eq 1 ]; then
  echo "=== Assertion: Geyser bound the same port number as the Java listener ==="
  GEYSER_UDP_PORT=$(echo "$GEYSER_STARTUP_LINE" | grep -oE '[0-9]+$')
  if [ "$GEYSER_UDP_PORT" != "$PORT" ]; then
    note_failure "Geyser bound UDP $GEYSER_UDP_PORT, not the shared port $PORT"
  elif ! ss -ltn 2>/dev/null | grep -q ":$PORT"; then
    note_failure "TCP (Java) is not listening on $PORT according to ss"
  elif ! ss -lun 2>/dev/null | grep -q ":$PORT"; then
    note_failure "UDP (Bedrock) is not listening on $PORT according to ss -- the shared-port claim does not hold"
  else
    echo "  confirmed: TCP and UDP both bound on port $PORT (Java and Bedrock/Geyser sharing one port number)"
  fi

  if [ "$BEDROCK_PROTOCOL_AVAILABLE" -eq 1 ]; then
    echo "=== Driving a simulated Bedrock client at Geyser on the shared port ==="
    JOIN_OUTPUT=$(node "$ROOT/dev/bedrock-client.js" join "$PORT" "$BEDROCK_USERNAME" 2>&1)
    JOIN_RC=$?
    echo "$JOIN_OUTPUT" | sed 's/^/  /'
    if echo "$JOIN_OUTPUT" | grep -q '^JOINED'; then
      echo "  a synthetic (non-Xbox) Bedrock client joined -- unexpected given the hardcoded"
      echo "  validate-bedrock-login: true, but not a failure of this test either way"
    elif [ $JOIN_RC -eq 0 ]; then
      note_failure "the bedrock-protocol client reported success but never printed JOINED"
    else
      echo "  rejected, as expected without a real Xbox account (Geyser's own login gate)"
    fi
  else
    echo "=== Skipping the simulated Bedrock-protocol attempt: 'bedrock-protocol' is not installed for node ==="
    echo "    (the shared-port assertion above already does not depend on it)"
  fi
else
  echo "=== Skipping the shared-port and simulated-client checks: Geyser never started ==="
fi

# --- Assertion 2: identity, via a real Floodgate handshake ----------------
#
# This talks to Foton directly, never through Geyser, so it exercises
# Foton's own Floodgate login branch and the real shared key it generated
# regardless of whether Geyser itself managed to start.

echo "=== Sending a real Floodgate handshake (first connection) ==="
KEY_PATH="$RUN_DIR/bedrock/key.pem"
for _ in $(seq 1 30); do
  [ -f "$KEY_PATH" ] && break
  sleep 1
done
if [ ! -f "$KEY_PATH" ]; then
  note_failure "the shared key never appeared at $KEY_PATH"
elif ! FIRST_OUTPUT=$(node "$ROOT/dev/bedrock-client.js" floodgate "$PORT" "$KEY_PATH" "$BEDROCK_USERNAME" "$BEDROCK_XUID" 2>&1) || ! echo "$FIRST_OUTPUT" | grep -q '^JOINED'; then
  echo "$FIRST_OUTPUT" | sed 's/^/  /'
  note_failure "the first Floodgate join failed"
  tail -40 server.log
else
  echo "$FIRST_OUTPUT" | sed 's/^/  /'
  FIRST_UUID=$(echo "$FIRST_OUTPUT" | awk '/^JOINED/ {print $2}')
  FIRST_NAME=$(echo "$FIRST_OUTPUT" | awk '/^JOINED/ {print $3}')

  echo "=== Assertion: the joined name carries the configured prefix ==="
  EXPECTED_NAME="$PREFIX$BEDROCK_USERNAME"
  if [ "$FIRST_NAME" != "$EXPECTED_NAME" ]; then
    note_failure "expected name '$EXPECTED_NAME', got '$FIRST_NAME'"
  else
    echo "  name: $FIRST_NAME (uuid $FIRST_UUID)"
  fi

  if ! grep -q "$FIRST_NAME joined the game" server.log; then
    note_failure "server.log never recorded '$FIRST_NAME joined the game'"
    tail -40 server.log
  else
    echo "  confirmed in server.log: '$FIRST_NAME joined the game'"

    # The first connection's own client already closed its socket, but the
    # server's own cleanup (removing the player, freeing the UUID for a new
    # admission) runs asynchronously to that. Reconnecting before it lands
    # would race an "already connected" kick, not the persistence claim.
    echo "=== Waiting for the first connection to fully release the player slot ==="
    RELEASED=1
    for _ in $(seq 1 30); do
      if grep -q "Player $FIRST_UUID removed" server.log 2>/dev/null; then
        RELEASED=0
        break
      fi
      sleep 1
    done
    if [ $RELEASED -ne 0 ]; then
      note_failure "server.log never recorded 'Player $FIRST_UUID removed'"
      tail -40 server.log
    else
      echo "=== Sending the same Floodgate identity again (second connection) ==="
      if ! SECOND_OUTPUT=$(node "$ROOT/dev/bedrock-client.js" floodgate "$PORT" "$KEY_PATH" "$BEDROCK_USERNAME" "$BEDROCK_XUID" 2>&1) || ! echo "$SECOND_OUTPUT" | grep -q '^JOINED'; then
        echo "$SECOND_OUTPUT" | sed 's/^/  /'
        note_failure "the second Floodgate join failed"
        tail -40 server.log
      else
        echo "$SECOND_OUTPUT" | sed 's/^/  /'
        SECOND_UUID=$(echo "$SECOND_OUTPUT" | awk '/^JOINED/ {print $2}')
        SECOND_NAME=$(echo "$SECOND_OUTPUT" | awk '/^JOINED/ {print $3}')

        echo "=== Assertion: reconnecting with the same identity yields the same UUID ==="
        if [ "$SECOND_UUID" != "$FIRST_UUID" ]; then
          note_failure "UUID did not persist: first join $FIRST_UUID, second join $SECOND_UUID"
        elif [ "$SECOND_NAME" != "$EXPECTED_NAME" ]; then
          note_failure "expected name '$EXPECTED_NAME' on reconnect, got '$SECOND_NAME'"
        else
          echo "  confirmed: both connections resolved to $FIRST_UUID ($FIRST_NAME)"
        fi
      fi
    fi
  fi
fi

if [ "$FAILED" -ne 0 ]; then
  echo "########## BEDROCK TEST FAILED ##########"
  exit 1
fi
echo "########## BEDROCK TEST PASSED ##########"
