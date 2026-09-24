#!/bin/bash
# Boot Foton with real plugins, walk a client through the world, and report
# what the plugins made of it.
#
# A plugin that compiles against the API and loads in the host has proved very
# little: the questions that matter are whether its onEnable finishes, whether
# its listeners are called, and whether it throws when they are. This answers
# them against a running server, with PacketProbe beside the plugins under test
# so the packet pipeline is checked by something that writes down what it saw.
#
# Usage: bash dev/plugin-compat-test.sh [plugin.jar ...]
#
# Needs dev/build-plugin-api.sh and dev/build-packetevents.sh to have run, and
# a JDK 21+ at $FOTON_JAVA_HOME (default: the one `javac` belongs to).

export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
BIN="$TARGET_DIR/debug/foton"
PORT=${PORT:-25567}
RUN_DIR="${RUN_DIR:-$ROOT/run-plugins}"
API_JAR="$ROOT/plugin-api/build/foton-plugin-api.jar"
JAVA_HOME_DIR="${FOTON_JAVA_HOME:-$(dirname "$(dirname "$(readlink -f "$(command -v javac)")")")}"
PROBE_SRC="$ROOT/plugin-api/packetevents/check/probe"

[ -f "$API_JAR" ] || { echo "no API jar; run dev/build-plugin-api.sh"; exit 1; }
[ -f "$ROOT/plugin-api/build/bundled/packetevents.jar" ] || { echo "no packetevents.jar; run dev/build-packetevents.sh"; exit 1; }
bash "$ROOT/dev/fetch-plugin-runtime-libs.sh" || exit 1

echo "=== Building ==="
cargo build -p foton 2>&1 | tail -3
if [ "${PIPESTATUS[0]}" -ne 0 ]; then echo "BUILD FAILED"; exit 1; fi

echo "=== Building PacketProbe ==="
PROBE_OUT="$ROOT/plugin-api/build/probe"
rm -rf "$PROBE_OUT" && mkdir -p "$PROBE_OUT/classes"
CP="$ROOT/plugin-api/build/bundled/packetevents.jar:$API_JAR"
for jar in "$ROOT/plugin-api/lib/"*.jar; do CP="$CP:$jar"; done
javac -nowarn -d "$PROBE_OUT/classes" -cp "$CP" $(find "$PROBE_SRC" -name '*.java') || exit 1
cp "$PROBE_SRC/plugin.yml" "$PROBE_OUT/classes/"
jar --create --file "$PROBE_OUT/PacketProbe.jar" -C "$PROBE_OUT/classes" .

mkdir -p "$RUN_DIR/config" || exit 1
cd "$RUN_DIR" || exit 1
rm -rf plugins saves server.log
mkdir -p plugins
cp "$PROBE_OUT/PacketProbe.jar" plugins/
for jar in "$@"; do cp "$jar" plugins/ || exit 1; done

if [ ! -f config/config.toml ]; then
  nohup "$BIN" > /dev/null 2>&1 < /dev/null &
  GEN_PID=$!
  for _ in $(seq 1 60); do [ -f config/config.toml ] && break; sleep 1; done
  kill "$GEN_PID" 2>/dev/null; sleep 2; kill -9 "$GEN_PID" 2>/dev/null
  [ -f config/config.toml ] || { echo "SERVER NEVER WROTE A CONFIG"; exit 1; }
fi
sed -i \
  -e 's/^online_mode = .*/online_mode = false/' \
  -e 's/^encryption = .*/encryption = false/' \
  -e 's/^enforce_secure_chat = .*/enforce_secure_chat = false/' \
  -e "s/^server_port = .*/server_port = $PORT/" \
  config/config.toml

echo "=== Booting with $(ls plugins | tr '\n' ' ')==="
FOTON_PLUGIN_DIRECTORY="$RUN_DIR/plugins" FOTON_JAVA_HOME="$JAVA_HOME_DIR" FOTON_PLUGIN_API_JAR="$API_JAR" \
  nohup "$BIN" > server.log 2>&1 < /dev/null &
PID=$!
UP=1
for _ in $(seq 1 180); do
  kill -0 "$PID" 2>/dev/null || { echo "SERVER DIED DURING STARTUP"; tail -60 server.log; exit 1; }
  if python3 "$ROOT/dev/wait-tcp.py" 127.0.0.1 "$PORT" 1 "$PID" >/dev/null 2>&1; then UP=0; break; fi
  sleep 1
done
[ $UP -eq 0 ] || { echo "SERVER NEVER LISTENED"; kill "$PID"; tail -60 server.log; exit 1; }

echo "=== Joining ==="
JOIN_WATCH_SECONDS=${JOIN_WATCH_SECONDS:-5} \
JOIN_COMMANDS=${JOIN_COMMANDS:-"!walk 0.5 -60 0.5 0.2 0 10"} \
  python3 "$ROOT/dev/join.py" "$PORT"
RC=$?

echo "=== Stopping ==="
kill -TERM "$PID" 2>/dev/null
for _ in $(seq 1 30); do kill -0 "$PID" 2>/dev/null || break; sleep 1; done
kill -9 "$PID" 2>/dev/null

echo "=== Plugins ==="
grep -E "\[host\]" server.log
echo "=== Probe report ==="
cat plugins/PacketProbe/report.txt 2>/dev/null || echo "(no report)"
echo "=== Stack traces from plugin code ==="
grep -nE "^\s+at (fr\.|foton\.|com\.github\.retrooper|io\.github\.retrooper)" server.log | head -40
grep -cE "Exception|Error" server.log

exit $RC
