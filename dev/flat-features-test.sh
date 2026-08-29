#!/bin/bash
# Boot superflats that decorate, and check what vanilla says should be standing.
#
# `features = true` and `lakes = true` used to be rejected by the generator
# config validator, and a rejected generator config aborts the whole server
# before it opens its port -- every other dimension included. So the first thing
# either run proves is that the server starts at all.
#
# Run one asks for features and lakes on the classic four-layer stack. That
# stack is exactly four blocks tall, so nothing is ever above the grass block
# unless a feature put it there. Grass patches are clumpy, so this counts how
# many of a spread of columns were decorated rather than betting on one.
#
# Run two is the deterministic half. A layer that does not block motion is taken
# out of the layer stack and put back by an inline `FILL_LAYER` feature at the
# last decoration step -- vanilla's `adjustGenerationSettings` does this whether
# or not features are on. So a preset topped with short grass and no features
# has to come out with short grass over *every* column, and nothing else can put
# it there.
#
# Usage: bash dev/flat-features-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

echo "=== Building ==="
if ! cargo build 2>&1 | tail -2; then
  echo "BUILD FAILED"
  exit 1
fi

if [ ! -d "$ROOT/run-offline/config" ]; then
  echo "RUN dev/join-test.sh FIRST so a config exists"
  exit 1
fi

PID=""
cleanup() {
  [ -n "$PID" ] || return
  kill "$PID" 2>/dev/null
  for _ in $(seq 1 30); do kill -0 "$PID" 2>/dev/null || break; sleep 1; done
  kill -9 "$PID" 2>/dev/null
  PID=""
}

# start_server <run-dir> <port> <generator config toml>
start_server() {
  local run_dir="$1" port="$2" generator_config="$3"
  rm -rf "$run_dir"
  mkdir -p "$run_dir" || exit 1
  cp -r "$ROOT/run-offline/config" "$run_dir/config"
  sed -i "s/^server_port = .*/server_port = $port/" "$run_dir/config/config.toml"
  sed -i 's/^command_spam_threshold_seconds = .*/command_spam_threshold_seconds = 0/' "$run_dir/config/config.toml"
  sed -i 's/^default_groups = .*/default_groups = ["op"]/' "$run_dir/config/groups.toml"
  sed -i \
    -e "s|^generator = \"minecraft:overworld\"\$|generator = \"minecraft:flat\"\nconfig = { $generator_config }|" \
    "$run_dir/config/worlds.toml"

  cd "$run_dir" || exit 1
  nohup "$ROOT/target/debug/foton" > server.log 2>&1 < /dev/null &
  PID=$!
  for _ in $(seq 1 180); do
    ss -ltn 2>/dev/null | grep -q ":$port" && return 0
    sleep 1
  done
  echo "SERVER NEVER LISTENED ON $port (a rejected generator config aborts startup)"
  sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | tail -20
  cleanup
  exit 1
}

fail() {
  echo "########## FLAT FEATURES TEST FAILED ($1) ##########"
  exit 1
}

# --- Run one: the classic stack, with features and lakes -------------------
# One above the grass block of the default four-layer superflat in a dimension
# whose floor is -64.
SURFACE_Y=-60
start_server "$ROOT/run-flat-features" 25625 "features = true, lakes = true"

CMDS='gamemode creative'
CMDS="$CMDS;;teleport @s 8 -55 8"
CMDS="$CMDS;;!wait 3"
# The grass block layer is the top of the stack, so this says the stack itself
# generated; without it a missing surface would read as a missing feature.
CMDS="$CMDS;;execute if block 8 -61 8 minecraft:grass_block run tellraw @s \"FLATSURFACE\""
for x in 0 3 6 9 12 15; do
  for z in 0 3 6 9 12 15; do
    CMDS="$CMDS;;execute unless block $x $SURFACE_Y $z minecraft:air run tellraw @s \"FLATDECORATED\""
  done
done

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=2 JOIN_COMMAND_SETTLE_SECONDS=0.3 python3 "$ROOT/dev/join.py" 25625 > join.log 2>&1
STATUS=$?
cleanup

DECORATED=$(grep "server says" join.log | grep -cw FLATDECORATED)
echo "=== features run ==="
echo "decorated columns: $DECORATED of 36"
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic" | tail -4

[ $STATUS -eq 0 ] || fail "the client never settled on the features run"
grep "server says" join.log | grep -qw FLATSURFACE || fail "the superflat layers are missing"
[ "$DECORATED" -ge 3 ] || fail "only $DECORATED of 36 columns decorated"

# --- Run two: a non-opaque top layer, with no features ---------------------
LAYERS='layers = [{ block = "minecraft:bedrock", height = 1 }, { block = "minecraft:dirt", height = 2 }, { block = "minecraft:grass_block", height = 1 }, { block = "minecraft:short_grass", height = 1 }]'
start_server "$ROOT/run-flat-fill-layer" 25626 "$LAYERS"

CMDS='gamemode creative'
CMDS="$CMDS;;teleport @s 8 -55 8"
CMDS="$CMDS;;!wait 3"
CMDS="$CMDS;;execute if block 8 -61 8 minecraft:grass_block run tellraw @s \"FILLSURFACE\""
for x in 0 5 10 15; do
  for z in 0 5 10 15; do
    CMDS="$CMDS;;execute if block $x -60 $z minecraft:short_grass run tellraw @s \"FILLGRASS\""
  done
done

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=2 JOIN_COMMAND_SETTLE_SECONDS=0.3 python3 "$ROOT/dev/join.py" 25626 > join.log 2>&1
STATUS=$?
cleanup

FILLED=$(grep "server says" join.log | grep -cw FILLGRASS)
echo "=== fill layer run ==="
echo "filled columns: $FILLED of 16"
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic" | tail -4

[ $STATUS -eq 0 ] || fail "the client never settled on the fill layer run"
grep "server says" join.log | grep -qw FILLSURFACE || fail "the superflat layers are missing"
[ "$FILLED" -eq 16 ] || fail "$FILLED of 16 columns got the short grass layer back"

echo "########## FLAT FEATURES TEST PASSED ##########"
