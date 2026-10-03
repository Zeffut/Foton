#!/usr/bin/env bash
# Exercise BEEHIVE through a Paper-compiled Java listener on a real Foton server.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$repo/dev/paper-api-test-lib.sh"
command -v java >/dev/null
command -v javac >/dev/null
command -v jar >/dev/null

temp_root="$(realpath "${TMPDIR:-/tmp}")"
isolated_mode=false
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
  isolated_mode=true
  scratch="$2"
  binary="$3"
  java_home="$4"
  port="$5"
else
  scratch="$(mktemp -d "$temp_root/foton-beehive-plugin.XXXXXX")"
fi
server_pid=""
cleanup() {
  if [ -n "$server_pid" ] && kill -0 "$server_pid" 2>/dev/null; then
    kill "$server_pid" 2>/dev/null || true
    for _ in $(seq 1 30); do
      kill -0 "$server_pid" 2>/dev/null || break
      sleep 1
    done
    kill -9 "$server_pid" 2>/dev/null || true
  fi
  if [ "$isolated_mode" = true ]; then
    return
  fi
  if [ "${FOTON_BEEHIVE_KEEP_SCRATCH:-0}" = 1 ]; then
    echo "Beehive probe logs retained in $scratch" >&2
  else
    case "$scratch" in
      "$temp_root"/foton-beehive-plugin.*) rm -rf -- "$scratch" ;;
    esac
  fi
}
trap cleanup EXIT

if [ "${1:-}" != "--isolated" ]; then
  paper_api_jar="$(paper_api_prepare "${FOTON_PAPER_API_JAR:-}" "$scratch")"
  cd "$repo"
  bash dev/build-plugin-api.sh > "$scratch/api-build.log" 2>&1 || {
    tail -n 40 "$scratch/api-build.log" >&2
    exit 1
  }
  cargo build --quiet -p foton

  target_dir="${CARGO_TARGET_DIR:-$repo/target}"
  binary="$target_dir/debug/foton"
  [ -x "$binary" ] || { echo "Foton binary was not built: $binary" >&2; exit 1; }
  mkdir -p "$scratch/classes" "$scratch/plugins" "$scratch/run/config"
  javac --release 21 -cp "$paper_api_jar:$repo/plugin-api/lib/*" \
    -d "$scratch/classes" \
    "$repo/dev/fixtures/beehive-spawn-plugin/BeehiveSpawnProbe.java"
  cp "$repo/dev/fixtures/beehive-spawn-plugin/plugin.yml" "$scratch/classes/"
  jar --create --file "$scratch/plugins/BeehiveSpawnProbe.jar" -C "$scratch/classes" .

  java_binary="$(readlink -f "$(command -v java)")"
  java_home="$(dirname "$(dirname "$java_binary")")"
  port="${FOTON_BEEHIVE_TEST_PORT:-25597}"
  command -v unshare >/dev/null || {
    echo 'unshare is required to isolate the beehive probe server' >&2
    exit 1
  }
  command -v ip >/dev/null || {
    echo 'iproute2 is required to bring up isolated loopback' >&2
    exit 1
  }
  unshare -n bash -c 'ip link set lo up && exec bash "$@"' bash \
    "$repo/dev/beehive-plugin-test.sh" --isolated "$scratch" "$binary" "$java_home" "$port"
  exit 0
fi
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
  echo "Foton did not generate its configuration" >&2
  exit 1
}
sed -i \
  -e 's/^online_mode = .*/online_mode = false/' \
  -e 's/^encryption = .*/encryption = false/' \
  -e 's/^enforce_secure_chat = .*/enforce_secure_chat = false/' \
  -e "s/^server_port = .*/server_port = $port/" \
  config/config.toml
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
  echo "Foton did not start the beehive probe" >&2
  exit 1
fi

# The stored nectar bee is already past its occupation timer. The plugin
# cancels its first attempts; after the zero-honey assertion, a command allows
# its release. This exercises the Native UUID lookup while the bee is pending.
waiting_bee='minecraft:beehive[facing=east]{bees:[{entity_data:{id:"minecraft:bee",HasNectar:1b},ticks_in_hive:0,min_ticks_in_hive:2400}]}'
due_bee='minecraft:beehive[facing=east]{bees:[{entity_data:{id:"minecraft:bee",HasNectar:1b},ticks_in_hive:2402,min_ticks_in_hive:2400}]}'
export JOIN_COMMANDS="gamemode creative;;teleport @s 0 100 0;;time set day;;weather clear;;setblock 2 100 0 $waiting_bee;;execute if data block 2 100 0 bees[0] run tellraw @s \"BEEHIVE_BEE_STORED\";;setblock 2 100 0 minecraft:air;;setblock 2 100 0 $due_bee;;execute if block 2 100 0 minecraft:beehive[honey_level=0] run tellraw @s \"BEEHIVE_HONEY_STILL_ZERO\";;allowbeerelease;;execute if block 2 100 0 minecraft:beehive[honey_level=1] run tellraw @s \"BEEHIVE_HONEY_RAISED\";;execute if block 2 100 0 minecraft:beehive[honey_level=2] run tellraw @s \"BEEHIVE_HONEY_RAISED\""
JOIN_WATCH_SECONDS=1 JOIN_COMMAND_SETTLE_SECONDS=3 \
  python3 "$repo/dev/join.py" "$port" > "$scratch/join.log" 2>&1 || {
    tail -n 60 "$scratch/join.log" >&2
    tail -n 60 "$scratch/server.log" >&2
    exit 1
  }

for marker in \
  '[beehive-probe] BEEHIVE_TYPED_PENDING_CANCELLED' \
  '[beehive-probe] BEEHIVE_RELEASE_ALLOWED' \
  '[beehive-probe] BEEHIVE_TYPED_PENDING_ALLOWED'; do
  grep -qF "$marker" "$scratch/server.log" || {
    grep -iE 'beehive|beehive-probe|failed|error' "$scratch/server.log" | tail -n 45 >&2 || true
    grep -E 'server says|BEEHIVE|JOIN STATUS' "$scratch/join.log" | tail -n 30 >&2 || true
    echo "Missing Java callback marker: $marker" >&2
    exit 1
  }
done
if grep -qE 'WRONG_ENTITY_TYPE|PENDING_BEE_NOT_RESOLVED' "$scratch/server.log"; then
  tail -n 60 "$scratch/server.log" >&2
  echo "Beehive callback did not expose a live typed bee" >&2
  exit 1
fi
for marker in BEEHIVE_BEE_STORED BEEHIVE_HONEY_STILL_ZERO BEEHIVE_HONEY_RAISED; do
  grep -qF "$marker" "$scratch/join.log" || {
    tail -n 60 "$scratch/join.log" >&2
    echo "Missing client observation: $marker" >&2
    exit 1
  }
done
echo "Paper-compiled BEEHIVE listener saw a pending Bee; cancellation retained honey, release delivered it"
