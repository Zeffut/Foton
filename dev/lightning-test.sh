#!/bin/bash
# Strike a lightning rod and a weathered copper block with a bolt.
#
# Scrubbing copper is permanent: `LightningBolt` walks the struck block back to
# its first weathering stage, so an oxidized block becomes a plain one and stays
# that way. The random walk that cleans its neighbours is deliberately not
# asserted -- it is random by design.
#
# The rod is harder to watch. It holds `powered` for eight ticks, which is a
# fifth of a second, and a command sent afterwards always arrives too late to
# see it -- the first version of this test read `powered=false` and looked like
# a bug. So the pulse is caught with a witness that keeps a record: a TNT block
# beside the rod. Redstone lights it, the block becomes a primed entity, and the
# hole it leaves is still there whenever the next command lands.
#
# Usage: bash dev/lightning-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

# Respect $CARGO_TARGET_DIR the way `cargo build` itself already does; see
# the same lines in dev/join-test.sh for what hardcoding it used to cost.
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
BIN="$TARGET_DIR/debug/foton"

PORT=25599
RUN_DIR="$ROOT/run-lightning"

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
CMDS="$CMDS;;difficulty normal"

# The copper goes first and far away, so the rod's blast cannot reach it.
CMDS="$CMDS;;setblock 0 99 16 minecraft:stone"
CMDS="$CMDS;;setblock 0 100 16 minecraft:oxidized_copper"
CMDS="$CMDS;;teleport @s 8 100 16"
CMDS="$CMDS;;execute if block 0 100 16 minecraft:oxidized_copper run tellraw @s \"COPPERSTARTSOXIDIZED\""
# `LightningBolt.strike_position` is the block one hair below the bolt's feet,
# so a bolt at y=101 strikes the block at y=100.
CMDS="$CMDS;;summon minecraft:lightning_bolt 0.5 101 16.5"
CMDS="$CMDS;;execute if block 0 100 16 minecraft:copper_block run tellraw @s \"COPPERSCRUBBEDCLEAN\""

# A rod with a stick of TNT beside it. The TNT is the witness: it cannot
# un-light itself, so the hole outlives the eight-tick pulse.
CMDS="$CMDS;;setblock 0 99 0 minecraft:stone"
CMDS="$CMDS;;setblock 0 100 0 minecraft:lightning_rod[facing=up,powered=false]"
CMDS="$CMDS;;setblock 1 100 0 minecraft:tnt"
CMDS="$CMDS;;teleport @s 24 100 0"
CMDS="$CMDS;;execute if block 0 100 0 minecraft:lightning_rod[powered=false] run tellraw @s \"RODSTARTSCOLD\""
CMDS="$CMDS;;execute if block 1 100 0 minecraft:tnt run tellraw @s \"TNTSTARTSPLACED\""

CMDS="$CMDS;;summon minecraft:lightning_bolt 0.5 101 0.5"
CMDS="$CMDS;;execute unless block 1 100 0 minecraft:tnt run tellraw @s \"RODLITTHETNT\""
# And the pulse is momentary, not a latch: the rod is cold again afterwards.
CMDS="$CMDS;;execute if block 0 100 0 minecraft:lightning_rod[powered=false] run tellraw @s \"RODWENTCOLDAGAIN\""

# The control: a rod nobody struck never lights the TNT next to it.
CMDS="$CMDS;;setblock 0 100 32 minecraft:lightning_rod[facing=up,powered=false]"
CMDS="$CMDS;;setblock 1 100 32 minecraft:tnt"
CMDS="$CMDS;;execute if block 1 100 32 minecraft:tnt run tellraw @s \"UNSTRUCKRODLITNOTHING\""

# `Pig.thunderHit`: a struck pig becomes a zombified piglin. Two pigs on two
# perches sixteen blocks apart, one struck and one not, so the conversion has to
# be the bolt rather than anything the summon itself did. The perches are up at
# y=100 where the selector radius cannot reach a pig the world generated.
CMDS="$CMDS;;setblock 0 99 -64 minecraft:stone"
CMDS="$CMDS;;setblock 0 99 -48 minecraft:stone"
CMDS="$CMDS;;teleport @s 8 100 -56"
CMDS="$CMDS;;summon minecraft:pig 0.5 100 -63.5"
CMDS="$CMDS;;summon minecraft:pig 0.5 100 -47.5"
CMDS="$CMDS;;execute positioned 0.5 100 -63.5 if entity @e[type=minecraft:pig,distance=..3] run tellraw @s \"PIGSTARTSAPIG\""

CMDS="$CMDS;;summon minecraft:lightning_bolt 0.5 101 -63.5"
CMDS="$CMDS;;execute positioned 0.5 100 -63.5 if entity @e[type=minecraft:zombified_piglin,distance=..5] run tellraw @s \"STRUCKPIGZOMBIFIED\""
CMDS="$CMDS;;execute positioned 0.5 100 -47.5 if entity @e[type=minecraft:pig,distance=..3] run tellraw @s \"UNSTRUCKPIGSTAYSAPIG\""

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=3 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what happened ==="
grep "server says" join.log \
  | grep -oE "COPPERSTARTSOXIDIZED|COPPERSCRUBBEDCLEAN|RODSTARTSCOLD|TNTSTARTSPLACED|RODLITTHETNT|RODWENTCOLDAGAIN|UNSTRUCKRODLITNOTHING|PIGSTARTSAPIG|STRUCKPIGZOMBIFIED|UNSTRUCKPIGSTAYSAPIG"
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic" | tail -5

fail() { echo "########## LIGHTNING TEST FAILED ($1) ##########"; exit 1; }
said() { grep "server says" join.log | grep -q "$1"; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }
said COPPERSTARTSOXIDIZED   || fail "the copper was not oxidized to begin with"
said COPPERSCRUBBEDCLEAN    || fail "a bolt did not scrub the oxidized copper clean"
said RODSTARTSCOLD          || fail "the rod was already powered before any strike"
said TNTSTARTSPLACED        || fail "the witness TNT was never placed"
said RODLITTHETNT           || fail "a bolt on the rod sent no redstone pulse"
said RODWENTCOLDAGAIN       || fail "the rod never stopped being powered"
said UNSTRUCKRODLITNOTHING  || fail "a rod nobody struck lit the TNT anyway"
said PIGSTARTSAPIG          || fail "the pig never arrived on its perch"
said STRUCKPIGZOMBIFIED     || fail "a bolt did not turn the pig into a zombified piglin"
said UNSTRUCKPIGSTAYSAPIG   || fail "the pig nobody struck zombified anyway"
echo "########## LIGHTNING TEST PASSED ##########"
