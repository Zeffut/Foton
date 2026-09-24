#!/usr/bin/env bash
# Acceptance test for direct, Paper-style ViaVersion + ViaBackwards support.
# It downloads the unmodified official plugins, boots Foton's plugin host, and
# joins with a schema-driven Minecraft 1.21.11 client all the way into PLAY.
set -Eeuo pipefail

cd "$(dirname "$0")/.."
ROOT=$(pwd)
export PATH="$HOME/.cargo/bin:$PATH"

VIA_VERSION=5.11.0
CLIENT_PACKAGE_VERSION=1.68.0
VIA_VERSION_SHA256=18d19e90fc9467d68128c076630ae8700449c901402a3ef421837ce006bc8cae
VIA_BACKWARDS_SHA256=b21983d561e3f92df257683f0133ab6c68ec68175e8acfd82c6231723bf83587
VIA_VERSION_URL="https://github.com/ViaVersion/ViaVersion/releases/download/${VIA_VERSION}/ViaVersion-${VIA_VERSION}.jar"
VIA_BACKWARDS_URL="https://github.com/ViaVersion/ViaBackwards/releases/download/${VIA_VERSION}/ViaBackwards-${VIA_VERSION}.jar"

TARGET_DIR=${CARGO_TARGET_DIR:-"$ROOT/target"}
BIN="$TARGET_DIR/debug/foton"
PORT=${FOTON_VIA_TEST_PORT:-25586}
CLIENT_TIMEOUT_MS=${FOTON_VIA_CLIENT_TIMEOUT_MS:-60000}

scratch=""
server_pid=""

cleanup() {
  local status=$?
  trap - EXIT INT TERM
  if [[ -n "$server_pid" ]] && kill -0 "$server_pid" 2>/dev/null; then
    kill "$server_pid" 2>/dev/null || true
    for _ in $(seq 1 20); do
      kill -0 "$server_pid" 2>/dev/null || break
      sleep 0.1
    done
    kill -0 "$server_pid" 2>/dev/null && kill -9 "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
  fi
  if [[ -n "$scratch" && -d "$scratch" ]]; then
    rm -rf -- "$scratch"
  fi
  exit "$status"
}
trap cleanup EXIT INT TERM

for command in cargo curl sha256sum node npm java javac python3; do
  command -v "$command" >/dev/null 2>&1 || {
    echo "$command is required for the Via compatibility test" >&2
    exit 1
  }
done

if [[ -n ${FOTON_JAVA_HOME:-} ]]; then
  java_home=$FOTON_JAVA_HOME
elif [[ -n ${JAVA_HOME:-} ]]; then
  java_home=$JAVA_HOME
else
  java_bin=$(readlink -f "$(command -v java)")
  java_home=$(dirname "$(dirname "$java_bin")")
fi

[[ -d "$java_home" ]] || {
  echo "resolved Java home does not exist: $java_home" >&2
  exit 1
}

scratch=$(mktemp -d "${TMPDIR:-/tmp}/foton-via-e2e.XXXXXX")
run_dir="$scratch/run"
plugins_dir="$run_dir/plugins"
client_dir="$scratch/node-client"
mkdir -p "$run_dir/config" "$plugins_dir" "$client_dir"

download_verified() {
  local url=$1
  local expected=$2
  local destination=$3
  curl --fail --location --silent --show-error --retry 3 \
    --proto '=https' --tlsv1.2 --output "$destination" "$url"
  printf '%s  %s\n' "$expected" "$destination" | sha256sum --check --status || {
    echo "SHA-256 mismatch for official asset $url" >&2
    exit 1
  }
}

echo "=== Fetching official ViaVersion ${VIA_VERSION} plugins ==="
download_verified "$VIA_VERSION_URL" "$VIA_VERSION_SHA256" \
  "$plugins_dir/ViaVersion-${VIA_VERSION}.jar"
download_verified "$VIA_BACKWARDS_URL" "$VIA_BACKWARDS_SHA256" \
  "$plugins_dir/ViaBackwards-${VIA_VERSION}.jar"

echo "=== Installing isolated Minecraft 1.21.11 client ==="
npm install --prefix "$client_dir" --no-save --ignore-scripts --no-audit --no-fund \
  "minecraft-protocol@${CLIENT_PACKAGE_VERSION}" >/dev/null
NODE_PATH="$client_dir/node_modules" node - <<'NODE'
const protocolPackage = require('minecraft-protocol/package.json')
const data = require('minecraft-data')('1.21.11')
if (protocolPackage.version !== '1.68.0') {
  throw new Error(`expected minecraft-protocol 1.68.0, installed ${protocolPackage.version}`)
}
if (!data || data.version.minecraftVersion !== '1.21.11') {
  throw new Error('minecraft-protocol dependency has no exact 1.21.11 schema')
}
console.log(`client schema ready: Minecraft ${data.version.minecraftVersion}, protocol ${data.version.version}`)
NODE

echo "=== Building Foton and its plugin API ==="
cargo build
bash dev/build-plugin-api.sh
[[ -f "$ROOT/plugin-api/build/foton-plugin-api.jar" ]] || {
  echo "plugin API build did not produce foton-plugin-api.jar" >&2
  exit 1
}
[[ -d "$ROOT/plugin-api/lib" ]] || {
  echo "plugin API runtime libraries are missing" >&2
  exit 1
}

cp "$ROOT/package-content/config.toml" "$run_dir/config/config.toml"
cp "$ROOT/package-content/worlds.toml" "$run_dir/config/worlds.toml"
cp "$ROOT/package-content/groups.toml" "$run_dir/config/groups.toml"
cp "$ROOT/package-content/favicon.png" "$run_dir/config/favicon.png"
sed -i \
  -e 's/^online_mode = .*/online_mode = false/' \
  -e 's/^encryption = .*/encryption = false/' \
  -e 's/^enforce_secure_chat = .*/enforce_secure_chat = false/' \
  -e "s/^server_port = .*/server_port = $PORT/" \
  -e 's/^view_distance = .*/view_distance = 4/' \
  -e 's/^simulation_distance = .*/simulation_distance = 4/' \
  "$run_dir/config/config.toml"

echo "=== Booting Foton with official Via plugins on port $PORT ==="
(
  cd "$run_dir"
  FOTON_PLUGIN_DIRECTORY="$plugins_dir" \
  FOTON_JAVA_HOME="$java_home" \
  FOTON_PLUGIN_API_JAR="$ROOT/plugin-api/build/foton-plugin-api.jar" \
  FOTON_PLUGIN_LIBRARY_DIRECTORY="$ROOT/plugin-api/lib" \
    nohup "$BIN" > server.log 2>&1 < /dev/null &
  echo $! > server.pid
)
server_pid=$(<"$run_dir/server.pid")

listening=false
for _ in $(seq 1 120); do
  if ! kill -0 "$server_pid" 2>/dev/null; then
    echo "Foton died while starting the Via plugins" >&2
    tail -80 "$run_dir/server.log" >&2
    exit 1
  fi
  if node -e '
    const socket = require("net").connect(Number(process.argv[1]), "127.0.0.1")
    socket.once("connect", () => { socket.destroy(); process.exit(0) })
    socket.once("error", () => process.exit(1))
    setTimeout(() => process.exit(1), 500).unref()
  ' "$PORT"; then
    listening=true
    break
  fi
  sleep 1
done

if [[ "$listening" != true ]]; then
  echo "Foton never listened on port $PORT" >&2
  tail -80 "$run_dir/server.log" >&2
  exit 1
fi

echo "=== Joining through ViaBackwards as Minecraft 1.21.11 ==="
set +e
NODE_PATH="$client_dir/node_modules" node "$ROOT/dev/via-client.js" \
  127.0.0.1 "$PORT" "$CLIENT_TIMEOUT_MS" > "$scratch/client.log" 2>&1
client_status=$?
set -e

echo "=== Joining natively as Minecraft 26.2 ==="
set +e
python3 "$ROOT/dev/join.py" "$PORT" > "$scratch/native-client.log" 2>&1
native_status=$?
set -e

# Compare stable, plain-text records while retaining the untouched server log
# for failure diagnostics. This strips CRLF and CSI ANSI escape sequences.
normalized_server_log="$scratch/server.normalized.log"
tr -d '\r' < "$run_dir/server.log" \
  | sed -E $'s/\033\\[[0-?]*[ -\/]*[@-~]//g' > "$normalized_server_log"

log_status=0
via_version_record=$(grep -F -x -n -m 1 \
  "[host] enabled ViaVersion v${VIA_VERSION}" "$normalized_server_log" || true)
via_backwards_record=$(grep -F -x -n -m 1 \
  "[host] enabled ViaBackwards v${VIA_VERSION}" "$normalized_server_log" || true)
if [[ -z "$via_version_record" ]]; then
  echo "ViaVersion ${VIA_VERSION} did not reach enabled state" >&2
  log_status=1
fi
if [[ -z "$via_backwards_record" ]]; then
  echo "ViaBackwards ${VIA_VERSION} did not reach enabled state" >&2
  log_status=1
fi
if [[ -n "$via_version_record" && -n "$via_backwards_record" ]]; then
  via_version_line=${via_version_record%%:*}
  via_backwards_line=${via_backwards_record%%:*}
  if (( via_version_line >= via_backwards_line )); then
    echo "ViaBackwards enabled before ViaVersion" >&2
    log_status=1
  fi
fi

forbidden='\[host\] (ViaVersion|ViaBackwards) failed|DecoderException|EncoderException|IllegalReferenceCountException|ReferenceCountUtil|refCnt|refcount|(^|[^[:alnum:]_])(Exception|Error)(:| in thread)'
if grep -E -i -q "$forbidden" "$normalized_server_log"; then
  echo "Via/codec/reference-count exception found in server.log" >&2
  grep -E -i -n "$forbidden" "$normalized_server_log" >&2 || true
  log_status=1
fi

if ! kill -0 "$server_pid" 2>/dev/null; then
  echo "Foton stopped before the translated client completed" >&2
  log_status=1
fi

if [[ $client_status -ne 0 || $native_status -ne 0 || $log_status -ne 0 ]]; then
  echo "--- Via client log ---" >&2
  cat "$scratch/client.log" >&2
  echo "--- Native 26.2 client log ---" >&2
  cat "$scratch/native-client.log" >&2
  echo "--- Foton log tail ---" >&2
  tail -100 "$run_dir/server.log" >&2
  echo "########## VIA COMPATIBILITY TEST FAILED ##########" >&2
  exit 1
fi

cat "$scratch/client.log"
cat "$scratch/native-client.log"
echo "########## VIA COMPATIBILITY TEST PASSED ##########"
