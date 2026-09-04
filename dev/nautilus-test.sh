#!/bin/bash
# Put both nautiluses in a real server, in real water, and watch a real client
# receive them.
#
# The unit tests drive the brain in a test world, which proves the AI runs but
# says nothing about the two things only a live server answers: whether
# `/summon` reaches the generated factory at all, and whether the synched data
# a nautilus sends -- the dash flag, and the zombie one's coral variant, which
# is a registry reference -- encodes into a packet a client can read. A client
# that cannot read it disconnects, and that is what this catches.
#
# Usage: bash dev/nautilus-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

# Respect $CARGO_TARGET_DIR the way `cargo build` itself already does; see
# the same lines in dev/join-test.sh for what hardcoding it used to cost.
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
BIN="$TARGET_DIR/debug/foton"

PORT=25594
RUN_DIR="$ROOT/run-nautilus"

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

# A walled tank, laid one block at a time because Foton has no `/fill` yet. The
# walls are not decoration: unwalled water flows away within a tick or two and
# leaves a beached nautilus, which drowns in fifteen seconds and would fail this
# for the wrong reason.
CMDS='gamemode creative'
for x in $(seq -1 3); do
  for z in $(seq -1 3); do
    CMDS="$CMDS;;setblock $x 97 $z minecraft:stone"
    if [ "$x" = "-1" ] || [ "$x" = "3" ] || [ "$z" = "-1" ] || [ "$z" = "3" ]; then
      CMDS="$CMDS;;setblock $x 98 $z minecraft:stone"
      CMDS="$CMDS;;setblock $x 99 $z minecraft:stone"
    else
      CMDS="$CMDS;;setblock $x 98 $z minecraft:water"
      CMDS="$CMDS;;setblock $x 99 $z minecraft:water"
    fi
  done
done

CMDS="$CMDS;;teleport @s 1 100 -1"
CMDS="$CMDS;;summon minecraft:nautilus 1 98 1"
CMDS="$CMDS;;summon minecraft:zombie_nautilus 0 98 1"

# Both still there after the brains have had a while: a mob whose tick panics
# is removed, and one that never got its water drowns.
CMDS="$CMDS;;execute positioned 1 98 1 if entity @e[type=minecraft:nautilus,distance=..4] run tellraw @s \"NAUTILUSISHERE\""
CMDS="$CMDS;;execute positioned 1 98 1 if entity @e[type=minecraft:zombie_nautilus,distance=..4] run tellraw @s \"ZOMBIENAUTILUSISHERE\""

# A pufferfish is the whole of `#minecraft:nautilus_taming_items`, so this is
# the one right-click that reaches `tryToTame`. The roll is one in three, so
# nothing here asserts it worked -- only that the path runs and the mob lives.
CMDS="$CMDS;;give @s minecraft:pufferfish 4"
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;!useentity nautilus"
CMDS="$CMDS;;execute positioned 1 98 1 if entity @e[type=minecraft:nautilus,distance=..4] run tellraw @s \"NAUTILUSSURVIVEDTHEFEED\""

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=6 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== join.log ==="
grep -E "server says|before the commands|spawned around|right-clicked|JOIN" join.log | tail -14
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic|unknown|incorrect" | tail -8

fail() { echo "########## NAUTILUS TEST FAILED ($1) ##########"; exit 1; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }
grep -q "server says: NAUTILUSISHERE" join.log || fail "no nautilus in the tank"
grep -q "server says: ZOMBIENAUTILUSISHERE" join.log || fail "no zombie nautilus in the tank"
grep -q "server says: NAUTILUSSURVIVEDTHEFEED" join.log || fail "feeding a nautilus removed it"
# The nautilus half of this can also be satisfied by a natural spawn -- the
# pinned seed has ocean in range and `SpawnPlacements` registers the nautilus --
# which is fine, because what is being checked is that *a* nautilus encoded into
# a packet the client read. Nothing registers the zombie one for natural
# spawning, so its half is exactly the mob summoned above.
grep -qE "(before the commands|spawned around the player):.*\bnautilus x" join.log || fail "the client never received a nautilus"
grep -qE "(before the commands|spawned around the player):.*\bzombie_nautilus x" join.log || fail "the client never received a zombie nautilus"
if sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -qi "panic"; then
  fail "the server panicked"
fi
echo "########## NAUTILUS TEST PASSED ##########"
