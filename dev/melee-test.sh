#!/bin/bash
# Swing at a mob and watch what reaches the client.
#
# A critical hit is invisible from outside: the extra damage is folded into the
# same hurt the target would have taken anyway, and no command reports one. The
# only thing that leaves the server is a `ClientboundAnimatePacket`, which is
# what `!sawanimation crit` reads.
#
# Both halves are asserted. A crit needs `fallDistance > 0` at the moment of
# the swing, so `!hop` sends the arc of a real jump and swings on the way down
# inside one directive -- the two-second settle between two commands is long
# enough for the server to put the player back on their feet, and a player on
# their feet never crits. A swing taken standing still must not report one.
#
# Usage: bash dev/melee-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

# Respect $CARGO_TARGET_DIR the way `cargo build` itself already does; see
# the same lines in dev/join-test.sh for what hardcoding it used to cost.
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
BIN="$TARGET_DIR/debug/foton"

PORT=25623
RUN_DIR="$ROOT/run-melee"

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
sed -i 's/^command_spam_threshold_seconds = .*/command_spam_threshold_seconds = 0/' "$RUN_DIR/config/config.toml"
sed -i 's/^default_groups = .*/default_groups = ["op"]/' "$RUN_DIR/config/groups.toml"

cd "$RUN_DIR" || exit 1
nohup "$BIN" > server.log 2>&1 < /dev/null &
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
for x in -2 -1 0 1 2; do
  for z in -2 -1 0 1 2; do
    CMDS="$CMDS;;setblock $x 99 $z minecraft:stone"
  done
done
CMDS="$CMDS;;teleport @s 0 100 0"
# A pig that wandered in on its own would be hit instead of the one summoned,
# and one that wandered off would be out of reach.
CMDS="$CMDS;;gamerule spawn_mobs false"
CMDS="$CMDS;;kill @e[type=minecraft:pig]"

CMDS="$CMDS;;clear @s"
CMDS="$CMDS;;give @s minecraft:iron_sword 1"
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;summon minecraft:pig 0 100 2"
CMDS="$CMDS;;execute if entity @e[type=minecraft:pig] run tellraw @s \"APIGSTANDSTHERE\""

# A swing with both feet on the ground is never critical.
CMDS="$CMDS;;teleport @e[type=minecraft:pig] 0 100 2"
CMDS="$CMDS;;!forgetanimations"
CMDS="$CMDS;;!attack pig"
CMDS="$CMDS;;!sawanimation crit"
CMDS="$CMDS;;!forgetanimations"

# And the swing lands: enough of them kill the pig outright.
for _ in $(seq 1 4); do
  CMDS="$CMDS;;teleport @e[type=minecraft:pig] 0 100 2"
  CMDS="$CMDS;;!attack pig"
done
CMDS="$CMDS;;execute unless entity @e[type=minecraft:pig] run tellraw @s \"THEPIGDIED\""

# A swing taken on the way down is. The pig is a fresh one: the first died,
# and `!attack` swings at the last of its kind the client was told about.
CMDS="$CMDS;;summon minecraft:pig 0 100 2"
CMDS="$CMDS;;execute if entity @e[type=minecraft:pig] run tellraw @s \"ASECONDPIGSTANDSTHERE\""
CMDS="$CMDS;;teleport @e[type=minecraft:pig] 0 100 2"
CMDS="$CMDS;;!forgetanimations"
CMDS="$CMDS;;!hop 0 100 0 pig"
CMDS="$CMDS;;!sawanimation crit"

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=2 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what happened ==="
grep -E "server says|crit" join.log \
  | grep -oE "APIGSTANDSTHERE|THEPIGDIED|ASECONDPIGSTANDSTHERE|the client saw a crit|no crit reached the client"
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic|incorrect" | tail -5

fail() { echo "########## MELEE TEST FAILED ($1) ##########"; exit 1; }
said() { grep "server says" join.log | grep -q "$1"; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }
said APIGSTANDSTHERE || fail "nothing was summoned to hit"
grep -q "no crit reached the client" join.log \
  || fail "a swing taken standing still was reported as a critical hit"
said THEPIGDIED || fail "four sword swings did not kill a pig"
said ASECONDPIGSTANDSTHERE || fail "nothing was summoned for the falling swing"
grep -q "the client saw a crit" join.log \
  || fail "a swing taken on the way down was not critical"
echo "########## MELEE TEST PASSED ##########"
