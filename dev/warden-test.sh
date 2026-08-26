#!/bin/bash
# Walk past a warden on a real server and get killed for it.
#
# The warden is the one mob whose whole animal is its brain: it has no goals at
# all, and everything it knows about the world arrives through a vibration
# listener. That makes it the longest chain in the game, and only a running
# server holds all of it at once -- position packets become movement, movement
# emits `step`, the chunk's game-event registry hands that to the warden's
# `VibrationSystem.Listener`, the vibration travels, `AngerManagement` records
# who caused it, the brain turns that anger into a roar and the roar into an
# attack target, and the attack kills the player.
#
# The four markers are the four places that chain can break and still look fine
# from the inside:
#
#   * the client is told a warden exists at all -- the entity factory and the
#     spawn packet;
#   * the warden is in the world a moment later -- it did not remove itself on
#     its first tick, which is what a missing dig cooldown would do;
#   * the client is given darkness -- `Warden.applyDarknessAround`, which the
#     warden pulses out every six seconds whether or not it has noticed anybody;
#   * the player dies to a mob -- and nothing else in this world can kill them.
#
# The player is in survival on purpose. A warden ignores anybody in creative or
# spectator (vanilla's `EntitySelector.NO_CREATIVE_OR_SPECTATOR`), so a creative
# player can walk laps around one and it will never hear a thing.
#
# Every assertion is a thing that was said, never the absence of one. Nothing is
# asked after the death: a dead player's commands go nowhere.
#
# Usage: bash dev/warden-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25618
RUN_DIR="$ROOT/run-warden"

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
nohup "$ROOT/target/debug/steel" > server.log 2>&1 < /dev/null &
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

CMDS='op SmokeTester'
CMDS="$CMDS;;gamemode survival"
# A warden is a monster, and a monster refuses to exist on peaceful.
CMDS="$CMDS;;difficulty normal"
CMDS="$CMDS;;time set noon"

# --- the floor ------------------------------------------------------------
# A hundred blocks up, where whatever the world generated is somebody else's
# problem. Only as big as the walk and the warden's path across it: every
# `setblock` costs the run two seconds.
for X in $(seq -5 5); do
  for Z in $(seq -1 9); do
    CMDS="$CMDS;;setblock $X 99 $Z minecraft:stone"
  done
done

# --- the warden -----------------------------------------------------------
# Eight blocks off the walk: inside the sixteen it can hear, and far enough that
# it has to come looking rather than starting on top of the player.
CMDS="$CMDS;;teleport @s -4 100 0"
CMDS="$CMDS;;!wait 2"
CMDS="$CMDS;;summon minecraft:warden 0 100 8"
CMDS="$CMDS;;!wait 3"
CMDS="$CMDS;;!spawned warden"
CMDS="$CMDS;;execute if entity @e[type=minecraft:warden] run tellraw @s \"WARDEN_ALIVE\""

# --- the walk -------------------------------------------------------------
# Thirty-two quarter-block strides carry the player across the warden's front,
# emitting a `step` every time the distance walked crosses the next whole block.
CMDS="$CMDS;;!walk -4 100 0 0.25 0 32"
# Long enough for the anger to climb past the roar, for the roar's eighty-four
# ticks to run, and for the warden to cross the eight blocks and swing.
CMDS="$CMDS;;!wait 14"

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=2 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what happened ==="
grep -E "server says: WARDEN|walked .* strides|saw a warden|effect darkness|death\.attack" join.log
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic" | tail -5

fail() { echo "########## WARDEN TEST FAILED ($1) ##########"; exit 1; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }

said() { grep -q "$1" join.log; }

said "the client saw a warden spawn" \
  || fail "no warden reached the client, so the entity never spawned"
said "server says: WARDEN_ALIVE" \
  || fail "the warden is not in the world a moment after it was summoned"
said "got the effect darkness" \
  || fail "no darkness reached the player, so the warden's pulse never ran"
said "death.attack.mob" \
  || fail "the player survived, so the walk never became a warden coming for them"

echo "########## WARDEN TEST PASSED ##########"
