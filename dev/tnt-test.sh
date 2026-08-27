#!/bin/bash
# The two ways TNT catches that are not a lever: a burning arrow, and the TNT
# next door.
#
# The chain is the harder one to prove, because one stick of TNT already blows
# a row of TNT blocks apart on its own -- the hole looks the same either way.
# What tells them apart is the witness zombie ten and a half blocks from the
# first stick. A blast reaches twice its radius, so four is eight, and nothing
# at ten and a half takes a scratch from the first one. It only dies if the row
# lit itself and walked the blast out to it.
#
# The second zombie, on a shelf thirty blocks overhead, shows a zombie survives
# the run at all.
#
# There is no check here for `dropFromExplosion`. TNT that is blown up both
# stops dropping and lights itself, and the stick it lights blows the drop away
# a second later, so the two are not separable from inside a running world. It
# rests on the vanilla source.
#
# Everything is built within a chunk or two of the player: only the nine chunks
# around them are loaded, and `setblock` outside that fails without stopping the
# script. The player is in creative so the blast can go off next to them.
#
# Usage: bash dev/tnt-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25620
RUN_DIR="$ROOT/run-tnt"

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
sed -i 's/^command_spam_threshold_seconds = .*/command_spam_threshold_seconds = 0/' \
  "$RUN_DIR/config/config.toml"
grep -q '^command_spam_threshold_seconds' "$RUN_DIR/config/config.toml" ||
  echo 'command_spam_threshold_seconds = 0' >> "$RUN_DIR/config/config.toml"

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

add() { CMDS="$CMDS;;$1"; }

CMDS='gamemode creative'
add "time set 18000"
add "setblock 0 99 -3 minecraft:stone"
add "teleport @s 0 100 -3"
# The teleport crosses a chunk border and only the nine chunks around the
# player are loaded. `setblock` into an unloaded chunk fails quietly, and the
# `if block ... air` further down would then be answered by the nothing that was
# never built. Wait for the client to be given its chunks before building.
add "!wait 2"

# --- A burning arrow ---
# `Fire` and `Motion` both come straight off the entity NBT, so an arrow can be
# put in flight already alight without a bow or an enchantment in the world.
add "setblock 0 99 -8 minecraft:stone"
add "setblock 0 100 -8 minecraft:tnt"
add "execute if block 0 100 -8 minecraft:tnt run tellraw @s \"ARROWTARGETPLACED\""
add "summon minecraft:arrow 0.5 100.5 -10.0 {Fire:200s,Motion:[0.0,0.0,0.8]}"
add "!wait 1"
add "execute if block 0 100 -8 minecraft:air run tellraw @s \"ARROWTOOKTHEBLOCK\""
add "execute if entity @e[type=minecraft:tnt] run tellraw @s \"ARROWLITTHETNT\""
# Let it go off before anything below counts primed TNT again.
add "!wait 6"

# --- The chain ---
for x in 0 1 2 3 4 5 6 7 8; do
  add "setblock $x 99 0 minecraft:stone"
  add "setblock $x 100 0 minecraft:tnt"
done
add "execute if block 8 100 0 minecraft:tnt run tellraw @s \"ROWBUILT\""

add "setblock 10 99 0 minecraft:stone"
# The control goes thirty blocks straight up rather than out along the row.
# Primed TNT is thrown about by the blasts that light it, and one flung far
# enough down the row can reach a control that only stood aside; nothing reaches
# thirty blocks of air overhead.
add "setblock 0 129 -3 minecraft:stone"
add "summon minecraft:zombie 10.5 100.0 0.5 {Tags:[\"witness\"],NoAI:1b}"
add "summon minecraft:zombie 0.5 130.0 -2.5 {Tags:[\"far\"],NoAI:1b}"
add "execute if entity @e[type=minecraft:zombie,tag=witness] run tellraw @s \"WITNESSREADY\""
add "execute if entity @e[type=minecraft:zombie,tag=far] run tellraw @s \"CONTROLREADY\""

# Flint and steel rather than redstone: it is the path a player takes and the
# one the block already had, so nothing here rests on a signal reaching a block.
add "give @s minecraft:flint_and_steel 1"
add "!hotbar 0"
add "!useon 0 100 0 up"
add "!wait 1"
add "execute if entity @e[type=minecraft:tnt] run tellraw @s \"FIRSTSTICKLIT\""
add "!wait 10"
add "execute if block 0 100 0 minecraft:air run tellraw @s \"FIRSTSTICKGONE\""
add "execute if block 8 100 0 minecraft:air run tellraw @s \"ROWWENTUP\""
add "execute if entity @e[type=minecraft:zombie,tag=far] run tellraw @s \"CONTROLSURVIVED\""
add "execute unless entity @e[type=minecraft:zombie,tag=witness] run tellraw @s \"CHAINREACHEDTHEWITNESS\""

export JOIN_COMMANDS="$CMDS"
JOIN_COMMAND_SETTLE_SECONDS=0.5 JOIN_WATCH_SECONDS=2 \
  python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what the server answered ==="
grep "server says" join.log | grep -vE "setblock|teleport|gamemode|summon|time|give" | tail -14
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic|incorrect|unknown" | tail -6

if [ $STATUS -ne 0 ]; then
  echo "########## TNT TEST FAILED (the client never settled) ##########"
  tail -20 join.log
  exit 1
fi
for marker in ARROWTARGETPLACED ARROWTOOKTHEBLOCK ARROWLITTHETNT ROWBUILT \
              WITNESSREADY CONTROLREADY FIRSTSTICKLIT FIRSTSTICKGONE ROWWENTUP \
              CONTROLSURVIVED CHAINREACHEDTHEWITNESS; do
  # Only the server's own reply counts: join.py echoes the commands it sends.
  if ! grep "server says" join.log | grep -q "$marker"; then
    echo "########## TNT TEST FAILED ($marker missing) ##########"
    exit 1
  fi
done
echo "########## TNT TEST PASSED ##########"
