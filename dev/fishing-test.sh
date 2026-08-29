#!/bin/bash
# Cast a fishing rod into a pool, watch the bobber float, and reel it back in.
#
# Fishing crosses four layers that have each looked fine on their own: the item
# behavior, the projectile entity, the fluid scan that decides the bobber is in
# water, and the `Player.fishing` slot that makes the second right-click reel in
# rather than cast a second line. This drives all four through a real client.
#
# The catch itself is not asserted here on purpose. A bite arrives 100-600 ticks
# after the cast and the nibble window that makes a reel productive is 20-40
# ticks wide, so a scripted client cannot reel at the right moment without
# flaking. That path is covered deterministically by the unit tests beside
# `fishing_hook.rs`.
#
# Usage: bash dev/fishing-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25603
RUN_DIR="$ROOT/run-fishing"

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
sed -i 's/^default_groups = .*/default_groups = ["op"]/' "$RUN_DIR/config/groups.toml"

cd "$RUN_DIR" || exit 1
nohup "$ROOT/target/debug/foton" > server.log 2>&1 < /dev/null &
PID=$!
cleanup() {
  kill "$PID" 2>/dev/null
  for _ in $(seq 1 30); do kill -0 "$PID" 2>/dev/null || break; sleep 1; done
  kill -9 "$PID" 2>/dev/null
}

for _ in $(seq 1 180); do
  ss -ltn 2>/dev/null | grep -q ":$PORT" && break
  sleep 1
done
if ! ss -ltn 2>/dev/null | grep -q ":$PORT"; then
  echo "SERVER NEVER LISTENED ON $PORT"
  sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | tail -20
  cleanup; exit 1
fi

CMDS='gamemode creative'
# One throwaway command first: the very first `setblock` of a run can land
# before the chunk around the player is ready.
CMDS="$CMDS;;time set day"
# Clear weather, because rain speeds the bite timer up and a thunderstorm would
# make the run less repeatable than it needs to be.
CMDS="$CMDS;;weather clear"

# A three-by-four pool with a stone bottom. The bobber is thrown almost
# straight down and drifts a block or two forward, so the pool starts one block
# ahead of where the line leaves the rod.
for x in 0 1 2; do
  for z in 1 2 3 4; do
    CMDS="$CMDS;;setblock $x 98 $z minecraft:stone"
  done
done
for x in 0 1 2; do
  for z in 1 2 3 4; do
    CMDS="$CMDS;;setblock $x 99 $z minecraft:water"
  done
done
CMDS="$CMDS;;teleport @s 1.5 101 1.5"

# Nothing has been cast yet, so there must be no bobber in the world. Every
# other assertion below reads as a pass if bobbers were lying around already.
CMDS="$CMDS;;execute unless entity @e[type=minecraft:fishing_bobber] run tellraw @s \"NOBOBBERYET\""

# Cast. Pitch 89 rather than 90: at exactly 90 the vanilla throw formula
# divides by a cosine that has already crossed zero in float precision and the
# bobber is flung upwards instead.
CMDS="$CMDS;;clear @s"
CMDS="$CMDS;;give @s minecraft:fishing_rod 1"
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;!useitem 0 89"
CMDS="$CMDS;;!spawned fishing_bobber"
CMDS="$CMDS;;execute if entity @e[type=minecraft:fishing_bobber] run tellraw @s \"BOBBERISOUT\""

# Give it a moment to fly, meet the water and settle, then ask where it is. A
# bobber that fell through the pool or carried on past it fails this.
CMDS="$CMDS;;time set day"
CMDS="$CMDS;;time set day"
CMDS="$CMDS;;execute positioned 1 100 2 if entity @e[type=minecraft:fishing_bobber,distance=..4] run tellraw @s \"BOBBERFLOATSINTHEPOOL\""

# The second right-click reels in rather than casting again.
CMDS="$CMDS;;!useitem 0 89"
CMDS="$CMDS;;execute unless entity @e[type=minecraft:fishing_bobber] run tellraw @s \"BOBBERREELEDIN\""

# ...and the third casts a fresh line, which only works if reeling in cleared
# the player's fishing slot.
CMDS="$CMDS;;!useitem 0 89"
CMDS="$CMDS;;execute if entity @e[type=minecraft:fishing_bobber] run tellraw @s \"SECONDCASTWENTOUT\""

# Losing the rod cuts the line: `shouldStopFishing` discards a bobber whose
# owner is no longer holding a rod in either hand.
CMDS="$CMDS;;clear @s"
CMDS="$CMDS;;time set day"
CMDS="$CMDS;;execute unless entity @e[type=minecraft:fishing_bobber] run tellraw @s \"LOSINGTHERODCUTTHELINE\""

export JOIN_COMMANDS="$CMDS"
python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what happened ==="
grep -E "server says|saw a fishing_bobber" join.log \
  | grep -oE "NOBOBBERYET|BOBBERISOUT|BOBBERFLOATSINTHEPOOL|BOBBERREELEDIN|SECONDCASTWENTOUT|LOSINGTHERODCUTTHELINE|saw a fishing_bobber spawn"
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic|incorrect" | tail -5

fail() { echo "########## FISHING TEST FAILED ($1) ##########"; exit 1; }
said() { grep "server says" join.log | grep -q "$1"; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }
said NOBOBBERYET || fail "a bobber existed before the rod was ever used"
grep -q "the client saw a fishing_bobber spawn" join.log \
  || fail "using the rod spawned no bobber"
said BOBBERISOUT || fail "the cast bobber is not in the world"
said BOBBERFLOATSINTHEPOOL || fail "the bobber never settled in the pool it was cast into"
said BOBBERREELEDIN || fail "the second right-click did not reel the bobber in"
said SECONDCASTWENTOUT || fail "reeling in left the player unable to cast again"
said LOSINGTHERODCUTTHELINE || fail "the bobber outlived the rod that was holding it"
echo "########## FISHING TEST PASSED ##########"
