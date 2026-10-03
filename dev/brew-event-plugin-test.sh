#!/usr/bin/env bash
# Execute a Paper-compiled listener against a real Foton brewing stand.
set -euo pipefail
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$repo/dev/paper-api-test-lib.sh"
temp_root="$(realpath "${TMPDIR:-/tmp}")"
scratch="$(mktemp -d "$temp_root/foton-brew-event.XXXXXX")"
server_pid=""
cleanup() {
  if [ -n "$server_pid" ] && kill -0 "$server_pid" 2>/dev/null; then
    kill "$server_pid" 2>/dev/null || true
    for _ in $(seq 1 20); do
      kill -0 "$server_pid" 2>/dev/null || break
      sleep 1
    done
    kill -9 "$server_pid" 2>/dev/null || true
  fi
  if [ "${FOTON_BREW_KEEP_SCRATCH:-0}" = 1 ]; then
    echo "Brewing probe logs retained in $scratch" >&2
  else
    case "$scratch" in "$temp_root"/foton-brew-event.*) rm -rf -- "$scratch" ;; esac
  fi
}
trap cleanup EXIT
paper_api="$(paper_api_prepare "${FOTON_PAPER_API_JAR:-}" "$scratch")"
cd "$repo"
mkdir -p "$scratch/classes" "$scratch/plugins" "$scratch/run"
javac --release 21 -cp "$paper_api:$repo/plugin-api/lib/*" -d "$scratch/classes" \
  "$repo/dev/fixtures/brew-event-plugin/BrewEventProbe.java"
cp "$repo/dev/fixtures/brew-event-plugin/plugin.yml" "$scratch/classes/"
jar --create --file "$scratch/plugins/BrewEventProbe.jar" -C "$scratch/classes" .
if [ "${1:-}" = --compile-only ]; then
  echo "BrewEvent probe compiled against pinned Paper 1.21.11 API"
  exit 0
fi
bash dev/build-plugin-api.sh > "$scratch/api-build.log" 2>&1 || {
  tail -n 40 "$scratch/api-build.log" >&2
  exit 1
}
cargo build --quiet -p foton
binary="${CARGO_TARGET_DIR:-$repo/target}/debug/foton"
[ -x "$binary" ] || { echo "Foton binary missing: $binary" >&2; exit 1; }
java_binary="$(readlink -f "$(command -v java)")"
java_home="$(dirname "$(dirname "$java_binary")")"
port="${FOTON_BREW_TEST_PORT:-25596}"
cd "$scratch/run"
"$binary" > "$scratch/config-generation.log" 2>&1 < /dev/null &
generation_pid=$!
for _ in $(seq 1 60); do
  [ -f config/config.toml ] && break
  sleep 1
done
kill "$generation_pid" 2>/dev/null || true
wait "$generation_pid" 2>/dev/null || true
[ -f config/config.toml ] || { tail -n 40 "$scratch/config-generation.log" >&2; exit 1; }
sed -i -e 's/^online_mode = .*/online_mode = false/' \
  -e 's/^encryption = .*/encryption = false/' \
  -e 's/^enforce_secure_chat = .*/enforce_secure_chat = false/' \
  -e "s/^server_port = .*/server_port = $port/" config/config.toml
sed -i 's/^default_groups = .*/default_groups = ["op"]/' config/groups.toml
FOTON_JAVA_HOME="$java_home" FOTON_PLUGIN_DIRECTORY="$scratch/plugins" \
FOTON_PLUGIN_API_JAR="$repo/plugin-api/build/foton-plugin-api.jar" \
FOTON_PLUGIN_LIBRARY_DIRECTORY="$repo/plugin-api/lib" \
  "$binary" > "$scratch/server.log" 2>&1 < /dev/null &
server_pid=$!
for _ in $(seq 1 120); do
  python3 "$repo/dev/wait-tcp.py" 127.0.0.1 "$port" 1 "$server_pid" >/dev/null 2>&1 && break
  kill -0 "$server_pid" 2>/dev/null || break
  sleep 1
done
python3 "$repo/dev/wait-tcp.py" 127.0.0.1 "$port" 1 "$server_pid" >/dev/null 2>&1 || {
  tail -n 60 "$scratch/server.log" >&2
  exit 1
}
ready_stand='minecraft:brewing_stand{BrewTime:1s,Fuel:7b,Items:[{Slot:0b,id:"minecraft:potion",count:1,components:{"minecraft:potion_contents":{potion:"minecraft:water"}}},{Slot:1b,id:"minecraft:potion",count:1,components:{"minecraft:potion_contents":{potion:"minecraft:water"}}},{Slot:2b,id:"minecraft:potion",count:1,components:{"minecraft:potion_contents":{potion:"minecraft:water"}}},{Slot:3b,id:"minecraft:nether_wart",count:1}]}'
export JOIN_COMMANDS="gamemode creative;;teleport @s 0 100 0;;setblock 2 100 0 $ready_stand;;brewprobe allow;;data merge block 2 100 0 {BrewTime:1s};;brewprobe verify"
JOIN_WATCH_SECONDS=1 JOIN_COMMAND_SETTLE_SECONDS=2 \
  timeout 180s python3 "$repo/dev/join.py" "$port" > "$scratch/join.log" 2>&1 || {
    tail -n 60 "$scratch/join.log" >&2
    tail -n 60 "$scratch/server.log" >&2
    exit 1
  }
for marker in ENABLED CANCELLED_CALLBACK CANCELLED_INPUTS_RETAINED MODIFIED_CALLBACK MODIFIED_RESULTS_PERSISTED; do
  grep -qF "[brew-probe] $marker" "$scratch/server.log" || {
    tail -n 80 "$scratch/server.log" >&2
    tail -n 60 "$scratch/join.log" >&2
    echo "Missing brewing probe marker: $marker" >&2
    exit 1
  }
done
echo "Paper-compiled BrewEvent listener: live inventory, cancellation, changed results and PDC writeback passed"
