#!/bin/bash
# Build a conduit the way a player does, and check it earns its keep.
#
# A conduit does nothing until it is sealed in a 3x3x3 pocket of water and
# ringed by prismarine, so the test lays the whole thing out block by block.
# The frame goes down in two halves on purpose:
#
#   twelve blocks -- under vanilla's sixteen, so the conduit must stay dark and
#                    the player standing in water beside it must get nothing.
#   forty-two     -- the full frame, which is both the activation threshold and
#                    the hunting one, so Conduit Power must appear and the
#                    hostile mob in the water must be beaten to death.
#
# The pair is what makes the first check mean anything: the same player, in the
# same puddle, at the same distance, with only the frame changed between them.
#
# The far zombie is the same test in reverse. It is in water and it is a mob the
# conduit would happily pick, and the only thing wrong with it is that it is
# fifteen blocks away rather than one. If it died too, something other than the
# conduit was killing zombies.
#
# Usage: bash dev/conduit-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25618
RUN_DIR="$ROOT/run-conduit"

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
# Laying eighty blocks in a row outruns the anti-spam counter, which sheds only
# one point a game tick.
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

# The conduit sits at 0 100 0.
CX=0; CY=100; CZ=0

add() { CMDS="$CMDS;;$1"; }

CMDS='gamemode survival'
# Midnight so the zombies do not burn, clear so `isInWaterOrRain` can only be
# answered by the water this script puts down.
add "time set midnight"
add "weather clear"
add "teleport @s $CX 100 8"

# A floor under the pocket, or the water falls out of it and the mob with it.
for dx in -1 0 1; do
  for dz in -1 0 1; do
    add "setblock $((CX + dx)) $((CY - 2)) $((CZ + dz)) minecraft:stone"
  done
done

# The 3x3x3 pocket. The middle is the conduit itself, which is waterlogged.
for dx in -1 0 1; do
  for dy in -1 0 1; do
    for dz in -1 0 1; do
      if [ "$dx" -eq 0 ] && [ "$dy" -eq 0 ] && [ "$dz" -eq 0 ]; then
        continue
      fi
      add "setblock $((CX + dx)) $((CY + dy)) $((CZ + dz)) minecraft:water"
    done
  done
done
add "setblock $CX $CY $CZ minecraft:conduit[waterlogged=true]"
add "execute if block $CX $CY $CZ minecraft:conduit run tellraw @s \"CONDUITPLACED\""

# Somewhere for the player to stand with their feet in water.
add "setblock $CX 99 8 minecraft:stone"
add "setblock $CX 100 8 minecraft:water"

# The frame: the three axis-aligned rings of the 5x5 shell. Twelve first, then
# the other thirty.
frame=""
for dx in -2 -1 0 1 2; do
  for dy in -2 -1 0 1 2; do
    for dz in -2 -1 0 1 2; do
      ax=${dx#-}; ay=${dy#-}; az=${dz#-}
      outside=0
      [ "$ax" -gt 1 ] || [ "$ay" -gt 1 ] || [ "$az" -gt 1 ] && outside=1
      on_ring=0
      if [ "$dx" -eq 0 ] && { [ "$ay" -eq 2 ] || [ "$az" -eq 2 ]; }; then on_ring=1; fi
      if [ "$dy" -eq 0 ] && { [ "$ax" -eq 2 ] || [ "$az" -eq 2 ]; }; then on_ring=1; fi
      if [ "$dz" -eq 0 ] && { [ "$ax" -eq 2 ] || [ "$ay" -eq 2 ]; }; then on_ring=1; fi
      if [ "$outside" -eq 1 ] && [ "$on_ring" -eq 1 ]; then
        frame="$frame $((CX + dx)),$((CY + dy)),$((CZ + dz))"
      fi
    done
  done
done
set -- $frame
if [ "$#" -ne 42 ]; then
  echo "########## CONDUIT TEST FAILED (built $# frame slots, expected 42) ##########"
  cleanup; exit 1
fi

placed=0
for slot in $frame; do
  placed=$((placed + 1))
  [ "$placed" -gt 12 ] && continue
  add "setblock ${slot//,/ } minecraft:prismarine"
done

add "!wait 3"
# Twelve is under the sixteen a conduit needs, so nothing at all should reach
# the player. The same check passes below once the frame is finished.
add "execute unless entity @s[nbt={active_effects:[{id:\"minecraft:conduit_power\"}]}] run tellraw @s \"NOPOWERFROMAPARTFRAME\""

placed=0
for slot in $frame; do
  placed=$((placed + 1))
  [ "$placed" -le 12 ] && continue
  add "setblock ${slot//,/ } minecraft:prismarine"
done

add "!wait 3"
add "execute if entity @s[nbt={active_effects:[{id:\"minecraft:conduit_power\"}]}] run tellraw @s \"CONDUITPOWERGRANTED\""

# One zombie in the conduit's own water, one in a puddle far out of reach.
add "setblock 15 98 0 minecraft:stone"
add "setblock 15 99 0 minecraft:water"
add "setblock 15 100 0 minecraft:water"
add "summon minecraft:zombie 1.5 99.0 0.5 {Tags:[\"near\"],NoAI:1b}"
add "summon minecraft:zombie 15.5 99.0 0.5 {Tags:[\"far\"],NoAI:1b}"
add "execute if entity @e[type=minecraft:zombie,tag=near] run tellraw @s \"NEARZOMBIEINTHEWATER\""
add "execute if entity @e[type=minecraft:zombie,tag=far] run tellraw @s \"FARZOMBIEINTHEWATER\""
# Four damage every two seconds against twenty health: six beats is enough with
# room to spare, and Steel drowns nothing, so the water itself cannot do it.
add "!wait 16"
add "execute if entity @e[type=minecraft:zombie,tag=far] run tellraw @s \"FARZOMBIESURVIVED\""
add "execute unless entity @e[type=minecraft:zombie,tag=near] run tellraw @s \"CONDUITKILLEDTHENEARZOMBIE\""

export JOIN_COMMANDS="$CMDS"
# Most of this script is bricklaying; the checks that need the server to have
# caught up ask for it with `!wait`.
JOIN_COMMAND_SETTLE_SECONDS=0.3 JOIN_WATCH_SECONDS=2 \
  python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what the server answered ==="
grep "server says" join.log | grep -vE "setblock|teleport|gamemode|summon|time|weather" | tail -10
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic|incorrect|unknown" | tail -6

if [ $STATUS -ne 0 ]; then
  echo "########## CONDUIT TEST FAILED (the client never settled) ##########"
  tail -20 join.log
  exit 1
fi
for marker in CONDUITPLACED NOPOWERFROMAPARTFRAME CONDUITPOWERGRANTED \
              NEARZOMBIEINTHEWATER FARZOMBIEINTHEWATER FARZOMBIESURVIVED \
              CONDUITKILLEDTHENEARZOMBIE; do
  # Only the server's own reply counts: join.py echoes the commands it sends.
  if ! grep "server says" join.log | grep -q "$marker"; then
    echo "########## CONDUIT TEST FAILED ($marker missing) ##########"
    exit 1
  fi
done
echo "########## CONDUIT TEST PASSED ##########"
