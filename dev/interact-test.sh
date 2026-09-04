#!/bin/bash
# Right-click a mob and watch what the item on the cursor is allowed to mean.
#
# Two things a command cannot reach, because both start at
# `ServerGamePacketListenerImpl.handleInteract` and go through
# `Player.interactOn` -> `Mob.interact`:
#
#   * shearing a mooshroom, which has to leave a cow behind. A mooshroom that
#     stayed a mooshroom would be `ready_for_shearing` again on the next tick,
#     so one pair of shears would be an unlimited mushroom supply.
#   * a spawn egg used on the mob it makes, which breeds a baby out of it.
#     `Mob.checkAndHandleImportantInteractions` runs before the mob's own
#     `mobInteract`, so this is not something the cow can decline.
#
# Nothing here counts entities, because no selector can. The second cow is
# found by moving one away and asking whether anything is still standing on the
# pad: with no calf that empties it, so the check that matters stays a positive
# one.
#
# Usage: bash dev/interact-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

# Respect $CARGO_TARGET_DIR the way `cargo build` itself already does; see
# the same lines in dev/join-test.sh for what hardcoding it used to cost.
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
BIN="$TARGET_DIR/debug/foton"

PORT=25625
RUN_DIR="$ROOT/run-interact"

echo "=== Building ==="
cargo build 2>&1 | tail -2
# A pipeline's status is its last command's, so `if ! cargo build | tail`
# tested `tail` and never failed. That made the branch below unreachable: a
# broken build fell straight through and the test ran against a stale binary.
if [ "${PIPESTATUS[0]}" -ne 0 ]; then
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
# One throwaway command first: the very first of a run can land before the
# chunk around the player is ready.
CMDS="$CMDS;;time set day"
CMDS="$CMDS;;difficulty easy"

# Frozen throughout. A cow strolls, and a mob that wandered out of arm's reach
# between the summon and the click would fail the test for the wrong reason.
CMDS="$CMDS;;tick freeze"

# --- the mooshroom, on a pad at the origin ---
for x in 0 1 2; do
  CMDS="$CMDS;;setblock $x 99 0 minecraft:stone"
done
CMDS="$CMDS;;teleport @s 0.0 100.0 0.0"
CMDS="$CMDS;;summon minecraft:mooshroom 1.5 100.0 0.0"
# One tick so the client is told the mooshroom exists; `!useentity` needs the id.
CMDS="$CMDS;;tick step 1"
CMDS="$CMDS;;teleport @e[type=minecraft:mooshroom] 1.5 100.0 0.0 0.0 0.0"
CMDS="$CMDS;;execute if entity @e[type=minecraft:mooshroom,distance=..4] run tellraw @s \"AMOOSHROOMISHERE\""
CMDS="$CMDS;;execute unless entity @e[type=minecraft:cow,distance=..6] run tellraw @s \"ANDNOCOWYET\""
CMDS="$CMDS;;give @s minecraft:shears"
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;!useentity mooshroom"
CMDS="$CMDS;;execute if entity @e[type=minecraft:cow,distance=..6] run tellraw @s \"SHEARINGLEFTACOW\""
CMDS="$CMDS;;execute unless entity @e[type=minecraft:mooshroom,distance=..6] run tellraw @s \"THEMOOSHROOMISGONE\""
CMDS="$CMDS;;execute if entity @e[type=minecraft:item,distance=..6] run tellraw @s \"ANDLEFTITSMUSHROOMS\""

# --- the spawn egg, sixteen blocks along so the cow above is out of range ---
for x in 16 17 18; do
  CMDS="$CMDS;;setblock $x 99 0 minecraft:stone"
done
CMDS="$CMDS;;teleport @s 16.0 100.0 0.0"
CMDS="$CMDS;;summon minecraft:cow 17.5 100.0 0.0"
CMDS="$CMDS;;tick step 1"
CMDS="$CMDS;;teleport @e[type=minecraft:cow,distance=..6] 17.5 100.0 0.0 0.0 0.0"
CMDS="$CMDS;;execute if entity @e[type=minecraft:cow,distance=..6] run tellraw @s \"ONECOWTOUSETHEEGGON\""
CMDS="$CMDS;;give @s minecraft:cow_spawn_egg"
CMDS="$CMDS;;!hotbar 1"
CMDS="$CMDS;;!useentity cow"
# There is no counting selector, so the second cow is found by moving one away
# and asking whether anything is still standing there. With no calf that empties
# the pad, which is why the check that follows is a positive one.
CMDS="$CMDS;;teleport @e[type=minecraft:cow,limit=1,sort=nearest,distance=..6] 60.0 100.0 0.0"
CMDS="$CMDS;;execute if entity @e[type=minecraft:cow,distance=..6] run tellraw @s \"THEEGGLEFTASECONDCOW\""

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=3 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what happened ==="
grep "server says" join.log \
  | grep -oE "AMOOSHROOMISHERE|ANDNOCOWYET|SHEARINGLEFTACOW|THEMOOSHROOMISGONE|ANDLEFTITSMUSHROOMS|ONECOWTOUSETHEEGGON|THEEGGLEFTASECONDCOW"
grep -E "right-clicked the (mooshroom|cow)" join.log
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic" | tail -5

fail() { echo "########## INTERACT TEST FAILED ($1) ##########"; exit 1; }
# `server says` first: join.py echoes the command it is running, so grepping
# the bare marker would match the question as well as the answer.
said() { grep "server says" join.log | grep -q "$1"; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }
grep -q "right-clicked the mooshroom" join.log || fail "the shears never reached the mooshroom"
grep -q "right-clicked the cow" join.log        || fail "the egg never reached the cow"

said AMOOSHROOMISHERE    || fail "no mooshroom was summoned"
said ANDNOCOWYET         || fail "a cow was already standing there, so the next check proves nothing"
said SHEARINGLEFTACOW    || fail "shearing the mooshroom left no cow behind"
said THEMOOSHROOMISGONE  || fail "the mooshroom survived its own shearing, and can be sheared again"
said ANDLEFTITSMUSHROOMS || fail "the conversion swallowed the shearing drops"

said ONECOWTOUSETHEEGGON  || fail "no cow was summoned to use the egg on"
said THEEGGLEFTASECONDCOW || fail "the egg left no calf: moving one cow away emptied the pad"

echo "########## INTERACT TEST PASSED ##########"
