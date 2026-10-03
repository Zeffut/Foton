#!/usr/bin/env bash
# Paper-compiled plugin and real MC client exercise live custom-payload channels.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$repo/dev/paper-api-test-lib.sh"
if [ "${1:-}" = "--isolated" ]; then
  [ "$#" -eq 5 ] || { echo 'Invalid isolated probe arguments' >&2; exit 1; }
  current_netns="$(readlink /proc/self/ns/net)"
  parent_netns="$(readlink "/proc/$PPID/ns/net")"
  initial_netns="$(readlink /proc/1/ns/net)"
  links="$(ip -o link show)"
  loopback="$(ip -o link show lo)"
  if [ "$current_netns" = "$parent_netns" ] || [ "$current_netns" = "$initial_netns" ] ||
    [ "$links" != "$loopback" ] || [[ "$loopback" != *'<LOOPBACK,UP'* ]]; then
    echo 'Refusing to start probe outside an isolated, loopback-only network namespace' >&2
    exit 1
  fi
  scratch="$2"
  binary="$3"
  java_home="$4"
  port="$5"
  server_pid=""
  stop_server() {
    if [ -n "$server_pid" ] && kill -0 "$server_pid" 2>/dev/null; then
      kill "$server_pid" 2>/dev/null || true
      for _ in $(seq 1 30); do
        kill -0 "$server_pid" 2>/dev/null || break
        sleep 1
      done
      kill -9 "$server_pid" 2>/dev/null || true
    fi
  }
  trap stop_server EXIT
  cd "$scratch/run"
  "$binary" > "$scratch/config-generation.log" 2>&1 < /dev/null &
  generation_pid=$!
  for _ in $(seq 1 60); do
    [ -f config/config.toml ] && break
    sleep 1
  done
  kill "$generation_pid" 2>/dev/null || true
  wait "$generation_pid" 2>/dev/null || true
  [ -f config/config.toml ] || {
    tail -n 40 "$scratch/config-generation.log" >&2
    echo "Foton did not generate configuration" >&2
    exit 1
  }
  sed -i \
    -e 's/^online_mode = .*/online_mode = false/' \
    -e 's/^encryption = .*/encryption = false/' \
    -e 's/^enforce_secure_chat = .*/enforce_secure_chat = false/' \
    -e "s/^server_port = .*/server_port = $port/" \
    config/config.toml

  FOTON_JAVA_HOME="$java_home" \
  FOTON_PLUGIN_DIRECTORY="$scratch/plugins" \
  FOTON_PLUGIN_API_JAR="$repo/plugin-api/build/foton-plugin-api.jar" \
  FOTON_PLUGIN_LIBRARY_DIRECTORY="$repo/plugin-api/lib" \
    "$binary" > "$scratch/server.log" 2>&1 < /dev/null &
  server_pid=$!
  for _ in $(seq 1 120); do
    python3 "$repo/dev/wait-tcp.py" 127.0.0.1 "$port" 1 "$server_pid" >/dev/null 2>&1 && break
    kill -0 "$server_pid" 2>/dev/null || break
    sleep 1
  done
  if ! python3 "$repo/dev/wait-tcp.py" 127.0.0.1 "$port" 1 "$server_pid" >/dev/null 2>&1; then
    tail -n 60 "$scratch/server.log" >&2
    echo "Foton did not start plugin channel probe" >&2
    exit 1
  fi

  PYTHONDONTWRITEBYTECODE=1 python3 "$repo/dev/plugin-channel-client.py" "$port" \
    > "$scratch/client.log" 2>&1 || {
    tail -n 60 "$scratch/client.log" >&2
    tail -n 80 "$scratch/server.log" >&2
    exit 1
  }
  for marker in ENABLED COMMAND_SENT REGISTERED_AND_SENT INCOMING_RECEIVED UNREGISTERED_AND_SUPPRESSED; do
    grep -qF "[channel-probe] $marker" "$scratch/server.log" || {
      tail -n 60 "$scratch/client.log" >&2
      tail -n 80 "$scratch/server.log" >&2
      echo "Missing plugin channel callback: $marker" >&2
      exit 1
    }
  done
  grep -qF 'CHANNEL PROBE: REGISTER -> payload -> UNREGISTER passed' "$scratch/client.log"
  echo 'Paper-compiled plugin and real client verified REGISTER, payload, and UNREGISTER'
  exit 0
fi

temp_root="$(realpath "${TMPDIR:-/tmp}")"
scratch="$(mktemp -d "$temp_root/foton-plugin-channels.XXXXXX")"
cleanup() {
  if [ "${FOTON_PLUGIN_CHANNEL_KEEP_SCRATCH:-0}" = 1 ]; then
    echo "Plugin channel probe retained in $scratch" >&2
  else
    case "$scratch" in
      "$temp_root"/foton-plugin-channels.*) rm -rf -- "$scratch" ;;
    esac
  fi
}
trap cleanup EXIT

paper_api_jar="$(paper_api_prepare "${FOTON_PAPER_API_JAR:-}" "$scratch")"
cd "$repo"
bash dev/build-plugin-api.sh > "$scratch/api-build.log" 2>&1 || {
  tail -n 40 "$scratch/api-build.log" >&2
  exit 1
}
cargo build --quiet -p foton

target_dir="${CARGO_TARGET_DIR:-$repo/target}"
binary="$target_dir/debug/foton"
[ -x "$binary" ] || { echo "Foton binary not built: $binary" >&2; exit 1; }
mkdir -p "$scratch/classes" "$scratch/plugins" "$scratch/run/config"
javac --release 21 -cp "$paper_api_jar:$repo/plugin-api/lib/*" \
  -d "$scratch/classes" \
  "$repo/dev/fixtures/plugin-channel-plugin/PluginChannelProbe.java"
cp "$repo/dev/fixtures/plugin-channel-plugin/plugin.yml" "$scratch/classes/"
jar --create --file "$scratch/plugins/PluginChannelProbe.jar" -C "$scratch/classes" .

java_binary="$(readlink -f "$(command -v java)")"
java_home="$(dirname "$(dirname "$java_binary")")"
port="${FOTON_PLUGIN_CHANNEL_TEST_PORT:-25598}"
command -v unshare >/dev/null || {
  echo 'unshare is required to isolate Foton’s wildcard TCP bind during this offline probe' >&2
  exit 1
}
command -v ip >/dev/null || {
  echo 'iproute2 is required to bring up loopback in the isolated network namespace' >&2
  exit 1
}
unshare -n bash -c 'ip link set lo up && exec bash "$@"' bash \
  "$repo/dev/plugin-channel-test.sh" --isolated "$scratch" "$binary" "$java_home" "$port"
