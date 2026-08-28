#!/bin/bash
# Read back the state the server applies to a player, off the wire.
#
# A player is authoritative over themselves. The server holding the right
# number is not the same thing as the player having it: if the value is never
# published, the player neither sees it nor suffers it, and every unit test
# still passes because the server-side assertion is true. So none of the
# questions below are asked of the server. Each one is read out of the packets
# a real client received.
#
# Three things are checked, and each one was broken in a different way:
#
#   * A blast pushing a player. Steel had no explosion packet at all, and that
#     packet is the only channel by which an explosion moves a player --
#     `ClientPacketListener.handleExplosion` ends with
#     `playerKnockback.ifPresent(player::addDeltaMovement)`. Damaging blasts
#     were limping along on `hurtMarked`, which sends a motion packet for a
#     different reason; a wind charge deals no damage, never marks anyone
#     hurt, and so did nothing whatsoever. Both are asked here.
#
#   * The golden hearts of an enchanted golden apple. The grant ran on
#     `LivingEntityBase`, which writes the field every mob keeps its shield
#     in. A player's shield lives in synchronized data instead, because that
#     is what their client reads, and the base cannot reach it.
#
#   * The direction a blow came from. `ClientboundHurtAnimationPacket` carries
#     `hurtDir`, the angle of the blow relative to where the player is facing.
#     Steel sent the player's own yaw, to everyone nearby rather than to the
#     player. The screen still twitched, so only the number tells the two
#     apart -- which is why the check below hits the same player from two
#     opposite sides and asks whether the two angles are half a turn apart.
#     A yaw would give the same answer twice, whatever the yaw happened to be.
#
#   * A game rule only the client acts on. `limitedCrafting` gates the recipe
#     book and `immediateRespawn` decides whether the death screen appears, and
#     both are enforced client-side. Changing one published nothing, so every
#     player kept acting on the old value until they reconnected.
#
#   * A player whose vehicle is destroyed under them. The eject unlinked on
#     `EntityBase`, which skips the player's own `stopRiding` override -- and
#     that override is what sends the packet saying the seat is empty.
#
# Usage: bash dev/client-sync-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25861
RUN_DIR="$ROOT/run-client-sync"

# Any fixed value works; this is the one dev/join-test.sh pins.
WORLD_SEED=${WORLD_SEED:-8675309}

# High above anything the generator makes, so the platform is laid into open
# air. A run whose floor half failed leaves the player falling, and a falling
# player's velocity is not the blast's doing any more -- which reads exactly
# like the bug this test exists to catch.
FLOOR_Y=150
STAND_Y=151

echo "=== Building ==="
if ! cargo build 2>&1 | tail -2; then
  echo "BUILD FAILED"
  exit 1
fi

rm -rf "$RUN_DIR"
mkdir -p "$RUN_DIR/config" || exit 1
cd "$RUN_DIR" || exit 1

if [ -d "$ROOT/run-offline/config" ]; then
  cp -r "$ROOT/run-offline/config/." config/
else
  # No shared offline config to borrow, so let the server write its defaults
  # and edit those. stdin has to come from /dev/null: the console is a TUI, and
  # a background process that reads a terminal is stopped by SIGTTIN instead of
  # running.
  echo "=== Generating an offline config ==="
  nohup "$ROOT/target/debug/steel" > /dev/null 2>&1 < /dev/null &
  GEN_PID=$!
  for _ in $(seq 1 90); do
    [ -f config/config.toml ] && [ -f config/groups.toml ] && break
    sleep 1
  done
  kill "$GEN_PID" 2>/dev/null
  sleep 2
  kill -9 "$GEN_PID" 2>/dev/null
  if [ ! -f config/config.toml ]; then
    echo "SERVER NEVER WROTE A CONFIG"
    exit 1
  fi
fi

sed -i \
  -e 's/^online_mode = .*/online_mode = false/' \
  -e 's/^encryption = .*/encryption = false/' \
  -e 's/^enforce_secure_chat = .*/enforce_secure_chat = false/' \
  -e "s/^server_port = .*/server_port = $PORT/" \
  -e 's/^command_spam_threshold_seconds = .*/command_spam_threshold_seconds = 0/' \
  config/config.toml
sed -i 's/^default_groups = .*/default_groups = ["op"]/' config/groups.toml

# Pin the seed. Unpinned, the terrain moves between runs, and a platform laid
# at a height that happened to be open once lands inside a hill the next time.
# The platform below is well clear of any terrain, but a pinned world is what
# makes two runs of the same build comparable at all.
if grep -q '^seed = ' config/worlds.toml; then
  sed -i "s/^seed = .*/seed = \"$WORLD_SEED\"/" config/worlds.toml
else
  sed -i "/^save_path = /a seed = \"$WORLD_SEED\"" config/worlds.toml
fi
rm -rf saves
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

# One throwaway command first: the very first `setblock` of a run can land
# before the chunk around the player is ready.
CMDS='gamemode creative'
CMDS="$CMDS;;time set day"
CMDS="$CMDS;;gamerule spawn_mobs false"
CMDS="$CMDS;;gamerule natural_health_regeneration false"
# A blast that dug the floor out would leave the player falling, and a falling
# player's velocity is not the blast's doing any more.
CMDS="$CMDS;;gamerule mob_griefing false"

# A floor, laid one block at a time because Steel has no `/fill`. Bedrock so
# nothing here can open a hole under the player.
for x in $(seq -3 3); do
  for z in $(seq -4 4); do
    CMDS="$CMDS;;setblock $x $FLOOR_Y $z minecraft:bedrock"
  done
done
CMDS="$CMDS;;teleport @s 0 $STAND_Y 0 0 0"
# The floor is the ground every reading below stands on, so it is checked
# rather than assumed: a `setblock` that lands before its chunk is ready fails
# quietly, and the player then falls through the whole test.
CMDS="$CMDS;;execute if block 0 $FLOOR_Y 0 minecraft:bedrock run tellraw @s \"THEFLOORISTHERE\""
CMDS="$CMDS;;execute if block 0 $FLOOR_Y 3 minecraft:bedrock run tellraw @s \"THEFLOORREACHESTHETNT\""

# --- a wind charge under the feet ------------------------------------------
#
# The one the owner reported, and the sharpest case in the whole class: a wind
# charge deals no damage at all, so nothing else the server sends could carry
# its push by accident.
CMDS="$CMDS;;gamemode survival"
CMDS="$CMDS;;clear @s"
CMDS="$CMDS;;give @s minecraft:wind_charge 1"
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;!forgetexplosions"
# Pitch 90 is straight down, so the charge bursts against the floor the player
# is standing on.
CMDS="$CMDS;;!useitem 0 90"
CMDS="$CMDS;;!wait 2"
CMDS="$CMDS;;!sawexplosion"
# It must not have hurt them on the way. A wind charge that damaged would be
# reaching the client through the hurt path instead, which is the accident this
# whole test exists to stop relying on.
CMDS="$CMDS;;execute if entity @s[nbt={Health:20.0f}] run tellraw @s \"THEGUSTDIDNOTHURT\""

# --- a charge of TNT --------------------------------------------------------
#
# The ordinary blast, so the fix is not only about the one item that was
# reported. Three blocks to the south, which pushes the player north.
#
# In creative, and that is the point rather than a convenience: a creative
# player takes no damage, so nothing marks them hurt and the motion packet that
# used to carry explosion knockback by accident never fires. Whatever push
# arrives came from the explosion packet and from nowhere else. Vanilla agrees
# that they should still be shoved -- `ServerExplosion.hurtEntities` skips only
# spectators and creative players who are *flying*.
CMDS="$CMDS;;gamemode creative"
CMDS="$CMDS;;!forgetexplosions"
CMDS="$CMDS;;teleport @s 0 $STAND_Y 0 0 0"
CMDS="$CMDS;;summon minecraft:tnt 0 $STAND_Y 3"
CMDS="$CMDS;;!wait 5"
CMDS="$CMDS;;!sawexplosion"
CMDS="$CMDS;;execute if entity @s[nbt={Health:20.0f}] run tellraw @s \"THETNTDIDNOTHURT\""

# --- the direction the blow came from ---------------------------------------
#
# Two blows from opposite sides, named exactly rather than staged with an
# entity: `/damage ... at` sets the source position the angle is computed from,
# so the two answers are a known half turn apart whatever the player's yaw is.
# Survival on purpose: a creative player is invulnerable, and `/damage` on
# one reports `commands.damage.invulnerable` and hurts nobody.
CMDS="$CMDS;;gamemode survival"
CMDS="$CMDS;;!forgethurt"
CMDS="$CMDS;;teleport @s 0 $STAND_Y 0 0 0"
CMDS="$CMDS;;damage @s 1 minecraft:mob_attack at 0 $STAND_Y 4"
CMDS="$CMDS;;damage @s 1 minecraft:mob_attack at 0 $STAND_Y -4"
CMDS="$CMDS;;!hurtdirections"

# --- the golden hearts ------------------------------------------------------
#
# Eaten, not commanded: Steel has no `/effect`, and eating is the path the
# owner took anyway. Absorption IV is sixteen points, which is eight golden
# hearts, and an enchanted golden apple is always edible so a full stomach
# cannot refuse it.
CMDS="$CMDS;;gamemode survival"
CMDS="$CMDS;;clear @s"
CMDS="$CMDS;;give @s minecraft:enchanted_golden_apple 1"
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;!forgetabsorption"
CMDS="$CMDS;;!useitem 0 0"
CMDS="$CMDS;;!wait 4"
CMDS="$CMDS;;!sawabsorption"

# --- a rule only the client enforces ----------------------------------------
#
# `immediateRespawn` decides whether the death screen appears, and the client
# decides that alone. There is nothing on the server to read: either the game
# event arrived or the player is still acting on the value they joined with.
CMDS="$CMDS;;!forgetgameevents"
CMDS="$CMDS;;gamerule immediate_respawn true"
CMDS="$CMDS;;!sawgameevent immediate_respawn"

# --- a vehicle destroyed under its rider -------------------------------------
#
# Nothing but `ClientboundSetPassengersPacket` tells the rider their seat is
# gone. The boat is killed rather than stepped out of, because that is the path
# that went through the base and skipped the player's own override.
CMDS="$CMDS;;gamemode creative"
CMDS="$CMDS;;teleport @s 0 $STAND_Y 0 0 0"
CMDS="$CMDS;;summon minecraft:oak_boat 0 $STAND_Y 0"
CMDS="$CMDS;;!useentity oak_boat"
CMDS="$CMDS;;execute if entity @s[nbt={RootVehicle:{}}] run tellraw @s BOARDED"
CMDS="$CMDS;;kill @e[type=minecraft:oak_boat]"
CMDS="$CMDS;;!wait 2"

export JOIN_COMMANDS="$CMDS"
python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what the client was told ==="
grep -E "told about [0-9]+ explosions|pushed the client by|no explosion knockback|tilted by|no hurt animation|given .* absorption|no absorption reached|told immediate_respawn|no immediate_respawn|is carrying" join.log
grep "server says" join.log | grep -owE "THEFLOORISTHERE|THEFLOORREACHESTHETNT|THEGUSTDIDNOTHURT|THETNTDIDNOTHURT|BOARDED"
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic|too quickly" | tail -5

fail() { echo "########## CLIENT SYNC TEST FAILED ($1) ##########"; exit 1; }
said() { grep "server says" join.log | grep -qw "$1"; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }

said THEFLOORISTHERE || fail "the platform under the player was never laid"
said THEFLOORREACHESTHETNT || fail "the platform does not reach where the TNT is summoned"

# The two explosion readings are taken in order, so each `!sawexplosion` block
# is matched against the burst it belongs to.
GUST_AT=$(grep -n "told about .* explosions" join.log | sed -n 1p | cut -d: -f1)
TNT_AT=$(grep -n "told about .* explosions" join.log | sed -n 2p | cut -d: -f1)
[ -n "$GUST_AT" ] || fail "the wind charge reading never ran"
[ -n "$TNT_AT" ] || fail "the TNT reading never ran"

# `!sawexplosion` prints its count first and then one line per push, so each
# reading is the count line and the handful that follow it.
GUST_BLOCK=$(sed -n "$GUST_AT,$((GUST_AT + 5))p" join.log)
TNT_BLOCK=$(sed -n "$TNT_AT,$((TNT_AT + 5))p" join.log)

echo "$GUST_BLOCK" | grep -qE "told about [1-9][0-9]* explosions" \
  || fail "the client was never told a wind charge burst"
GUST_PUSH=$(echo "$GUST_BLOCK" | grep -m1 -oE "pushed the client by [-0-9.]+ [-0-9.]+ [-0-9.]+" | awk '{print $6}')
[ -n "$GUST_PUSH" ] || fail "a wind charge under the feet handed the client no push at all"
awk -v y="$GUST_PUSH" 'BEGIN { exit !(y > 0.3) }' \
  || fail "the wind charge lifted the client by only $GUST_PUSH"
said THEGUSTDIDNOTHURT || fail "the wind charge hurt the player, so its push is not the packet's doing"

echo "$TNT_BLOCK" | grep -qE "told about [1-9][0-9]* explosions" \
  || fail "the client was never told a TNT charge went off"
TNT_PUSH=$(echo "$TNT_BLOCK" | grep -m1 -oE "pushed the client by [-0-9.]+ [-0-9.]+ [-0-9.]+" | awk '{print $7}')
[ -n "$TNT_PUSH" ] || fail "a TNT blast handed the client no push at all"
said THETNTDIDNOTHURT || fail "the TNT hurt the creative player, so its push is not the packet's doing"
awk -v z="$TNT_PUSH" 'BEGIN { exit !(z < -0.1) }' \
  || fail "TNT three blocks south pushed the client north by only $TNT_PUSH"

# The hurt direction, read as the gap between two opposite blows.
ANGLES=$(grep -m1 -oE "the client was tilted by .*" join.log | cut -d' ' -f6-)
[ -n "$ANGLES" ] || fail "no hurt animation ever reached the client"
set -- $ANGLES
[ $# -ge 2 ] || fail "only $# hurt animations arrived, so the two sides cannot be compared"
awk -v a="$1" -v b="$2" 'BEGIN {
  gap = a - b; if (gap < 0) gap = -gap
  while (gap >= 360) gap -= 360
  if (gap > 180) gap = 360 - gap
  exit !(gap > 170)
}' || fail "two blows from opposite sides tilted the screen by $1 and $2, which is not half a turn apart"

# The golden hearts.
grep -q "given 16.00 absorption" join.log \
  || fail "an enchanted golden apple put no golden hearts on the wire"

# The rule the client enforces alone.
grep -q "told immediate_respawn 1.0" join.log \
  || fail "changing immediateRespawn told the client nothing, so it still shows the old death screen"

# The seat that was destroyed under the rider. The boat has to have been
# boarded first, or an empty-seat packet would prove nothing.
said BOARDED || fail "the player never boarded the boat, so the eject proves nothing"
BOAT_AT=$(grep -n "server says: BOARDED" join.log | tail -1 | cut -d: -f1)
tail -n "+$BOAT_AT" join.log | grep -q "is carrying nobody" \
  || fail "a boat destroyed under its rider never told them their seat was gone"

echo "########## CLIENT SYNC TEST PASSED ##########"
