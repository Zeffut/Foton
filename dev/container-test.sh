#!/bin/bash
# Check that placed container blocks really have block entities behind them,
# and that breaking one gives back what was inside it.
#
# All three furnaces went unregistered: the behavior was written, a macro hid
# the struct from the codegen scanner, and nothing said so -- right-clicking one
# did nothing and smelting was unreachable. A unit test cannot see that, because
# the behavior compiles fine; only the running server can.
#
# `new_block_entity` comes from the block's behavior, so a block with NBT data
# behind it is a block whose behavior is registered. That one question covers
# every container, so they are all asked here.
#
# The second half asks the question a player cares about: break a full chest and
# the stock has to land on the floor. Vanilla scatters it from
# `BlockEntity.preRemoveSideEffects`, which only the chunk's own block write
# reaches -- a container whose hook is written but never called looks exactly
# like one that works, right up until someone breaks a full one.
# `/setblock ... destroy` takes that path (`Level.destroyBlock`), the same one a
# pickaxe takes; plain `/setblock` deliberately does not, because vanilla passes
# `UPDATE_SKIP_BLOCK_ENTITY_SIDEEFFECTS` there.
#
# Usage: bash dev/container-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

# Respect $CARGO_TARGET_DIR the way `cargo build` itself already does; see
# the same lines in dev/join-test.sh for what hardcoding it used to cost.
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
BIN="$TARGET_DIR/debug/foton"

PORT=25574
RUN_DIR="$ROOT/run-container"

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

CMDS='teleport @s 0 100 0'
CMDS="$CMDS;;setblock 0 99 0 minecraft:furnace"
CMDS="$CMDS;;setblock 2 99 0 minecraft:smoker"
CMDS="$CMDS;;setblock 4 99 0 minecraft:blast_furnace"
CMDS="$CMDS;;setblock 6 99 0 minecraft:shulker_box"
CMDS="$CMDS;;setblock 8 99 0 minecraft:red_shulker_box"
CMDS="$CMDS;;execute if data block 0 99 0 {} run tellraw @s \"FURNACEHASENTITY\""
CMDS="$CMDS;;execute if data block 2 99 0 {} run tellraw @s \"SMOKERHASENTITY\""
CMDS="$CMDS;;execute if data block 4 99 0 {} run tellraw @s \"BLASTHASENTITY\""
CMDS="$CMDS;;execute if data block 6 99 0 {} run tellraw @s \"SHULKERHASENTITY\""
CMDS="$CMDS;;execute if data block 8 99 0 {} run tellraw @s \"REDSHULKERHASENTITY\""

# A stocked chest and a stocked barrel, on their own platform well away from the
# player so nobody walks over the drops.
CMDS="$CMDS;;setblock 10 99 0 minecraft:stone"
CMDS="$CMDS;;setblock 10 100 0 minecraft:chest{Items:[{Slot:0b,id:\"minecraft:diamond\",count:5}]}"
CMDS="$CMDS;;setblock 12 99 0 minecraft:stone"
CMDS="$CMDS;;setblock 12 100 0 minecraft:barrel{Items:[{Slot:0b,id:\"minecraft:emerald\",count:3}]}"
# The controls: neither gem is lying around yet, so the markers further down can
# only have come out of a container. `distance` is left off on purpose -- an
# item that has just been scattered is still settling, and a radius made the
# answer depend on where it happened to land.
CMDS="$CMDS;;execute if entity @e[type=minecraft:item,nbt={Item:{id:\"minecraft:diamond\"}}] run tellraw @s \"DIAMONDLOOSEEARLY\""
CMDS="$CMDS;;execute if entity @e[type=minecraft:item,nbt={Item:{id:\"minecraft:emerald\"}}] run tellraw @s \"EMERALDLOOSEEARLY\""
CMDS="$CMDS;;setblock 10 100 0 minecraft:air destroy"
CMDS="$CMDS;;setblock 12 100 0 minecraft:air destroy"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute if entity @e[type=minecraft:item,nbt={Item:{id:\"minecraft:chest\"}}] run tellraw @s \"CHESTITSELFDROPPED\""
CMDS="$CMDS;;execute if entity @e[type=minecraft:item,nbt={Item:{id:\"minecraft:diamond\"}}] run tellraw @s \"CHESTGAVEBACKITSDIAMOND\""
CMDS="$CMDS;;execute if entity @e[type=minecraft:item,nbt={Item:{id:\"minecraft:emerald\"}}] run tellraw @s \"BARRELGAVEBACKITSEMERALD\""

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=2 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what the server answered ==="
grep "server says" join.log | tail -10

if [ $STATUS -ne 0 ]; then
  echo "########## CONTAINER TEST FAILED (the client never settled) ##########"
  tail -20 join.log
  exit 1
fi
# Only the server's own reply counts: join.py echoes the commands it sends.
said() { grep "server says" join.log | grep -q "$1"; }
fail() { echo "########## CONTAINER TEST FAILED ($1) ##########"; exit 1; }

for marker in FURNACEHASENTITY SMOKERHASENTITY BLASTHASENTITY \
              SHULKERHASENTITY REDSHULKERHASENTITY; do
  said "$marker" || fail "$marker missing"
done

! said DIAMONDLOOSEEARLY || fail "a diamond was loose before anything was broken"
! said EMERALDLOOSEEARLY || fail "an emerald was loose before anything was broken"
said CHESTITSELFDROPPED       || fail "the chest did not even drop itself"
said CHESTGAVEBACKITSDIAMOND  || fail "breaking a full chest destroyed what was inside it"
said BARRELGAVEBACKITSEMERALD || fail "breaking a full barrel destroyed what was inside it"
echo "########## CONTAINER TEST PASSED ##########"
