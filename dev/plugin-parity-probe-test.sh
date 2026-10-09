#!/usr/bin/env bash
# A Paper-compiled plugin checks, on a live server and a real client, what
# ZeldaCiv-style plugins rely on: canonical worlds, slot mirrors that write
# back (and never over a changed slot), block data interfaces, and recipes
# that keep their result stack.
#
#     bash dev/plugin-parity-probe-test.sh
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
  # The probe player needs creative-style access to spawn entities and place blocks.
  sed -i 's/^default_groups = .*/default_groups = ["op"]/' config/groups.toml

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
    echo "Foton did not start the parity probe" >&2
    exit 1
  fi

  # The crafting table is the client's to use: slots 37-40 are hotbar 0-3, 1-9 the grid, 0 the result.
  commands=(
    '!wait 3'
    'parityprobe'
    'parityprobe craft'
    '!click 37' '!click 1' '!click 38' '!click 2'
    'parityprobe preview radish'
    '!shiftclick 0'
    'parityprobe taken radish'
    '!click 39' '!click 1 1' '!click 2 1' '!click 4 1' '!click 40' '!click 5'
    'parityprobe preview whistle'
    '!shiftclick 0'
    'parityprobe taken whistle'
  )
  JOIN_COMMANDS="$(printf '%s;;' "${commands[@]}")" JOIN_COMMAND_SETTLE_SECONDS=1.5 \
    PYTHONDONTWRITEBYTECODE=1 python3 "$repo/dev/join.py" "$port" \
    > "$scratch/client.log" 2>&1 || {
    tail -n 60 "$scratch/client.log" >&2
    tail -n 80 "$scratch/server.log" >&2
    exit 1
  }
  grep -F '[parity-probe]' "$scratch/server.log"
  for step in all blocks craft taken; do
    grep -qF "[parity-probe] DONE $step" "$scratch/server.log" || {
      tail -n 80 "$scratch/server.log" >&2
      echo "The probe did not get through its '$step' step" >&2
      exit 1
    }
  done
  grep -qF '[parity-probe] DONE taken' "$scratch/server.log" || {
    tail -n 80 "$scratch/server.log" >&2
    echo "The probe did not get through its steps" >&2
    exit 1
  }
  if grep -qF '[parity-probe] FAIL' "$scratch/server.log"; then
    echo "Parity probe found a difference from Paper" >&2
    exit 1
  fi
  echo 'Paper-compiled probe and real client verified worlds, slot mirrors, block data and recipes'
  exit 0
fi

temp_root="$(realpath "${TMPDIR:-/tmp}")"
scratch="$(mktemp -d "$temp_root/foton-parity-probe.XXXXXX")"
cleanup() {
  if [ "${FOTON_PARITY_PROBE_KEEP_SCRATCH:-0}" = 1 ]; then
    echo "Parity probe retained in $scratch" >&2
  else
    case "$scratch" in
      "$temp_root"/foton-parity-probe.*) rm -rf -- "$scratch" ;;
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
  "$repo/dev/fixtures/parity-probe-plugin/ParityProbe.java"
cp "$repo/dev/fixtures/parity-probe-plugin/plugin.yml" "$scratch/classes/"
jar --create --file "$scratch/plugins/ParityProbe.jar" -C "$scratch/classes" .

java_binary="$(readlink -f "$(command -v java)")"
java_home="$(dirname "$(dirname "$java_binary")")"
port="${FOTON_PARITY_PROBE_PORT:-25599}"
command -v unshare >/dev/null || {
  echo 'unshare is required to isolate Foton’s wildcard TCP bind during this offline probe' >&2
  exit 1
}
command -v ip >/dev/null || {
  echo 'iproute2 is required to bring up loopback in the isolated network namespace' >&2
  exit 1
}
unshare -n bash -c 'ip link set lo up && exec bash "$@"' bash \
  "$repo/dev/plugin-parity-probe-test.sh" --isolated "$scratch" "$binary" "$java_home" "$port"
