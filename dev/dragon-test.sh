#!/bin/bash
# Two things about the ender dragon that a unit test cannot settle.
#
# The first is the fight. Nothing in this script summons the End's dragon: the
# client walks into the End and `EnderDragonFight` does the rest -- it ticks off
# the world, holds the seventeen-chunk arena loaded, builds the exit podium and
# puts a dragon over it. Killing that dragon is what lights the portal and drops
# the egg. Every one of those steps needs a live server with real chunk loading
# behind it, which is exactly what a unit test substitutes away.
#
# The second is the hitboxes.
#
# This is the one thing about multi-part entities that a unit test cannot
# settle. The dragon is eight `EnderDragonPart` boxes, and the client is never
# told any of them exists: it builds all eight itself from the dragon's spawn
# packet and numbers them `dragonId + 1 ..= dragonId + 8`. So every hit a player
# lands on a dragon arrives as an attack addressed to an id the server has no
# live entity for. Before the part lookup existed, `handle_attack` resolved that
# id through the flat live-entity map, missed, and returned -- the hit
# evaporated, and nothing anywhere said so.
#
# So the client here never sends the dragon's own id. It sends `dragonId + 3`,
# the body hitbox, and the test asks the server whether the dragon lost health.
# If it did, the hit went in on a hitbox id and came out on the dragon behind
# it, which is the whole claim.
#
# The dragon is frozen first. Its hover phase still gives it forward speed, and
# two seconds of settle between commands is forty ticks of drift -- enough to
# carry the body hitbox out of the player's three-block attack range and turn a
# working routing into a test that fails for a reason that is not the routing.
#
# Usage: bash dev/dragon-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25601
RUN_DIR="$ROOT/run-dragon"

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

CMDS='gamemode creative'
# One throwaway command first: the very first command of a run can land before
# the chunk around the player is ready.
CMDS="$CMDS;;time set day"
CMDS="$CMDS;;difficulty normal"
# Mob griefing off: a dragon eats every block its head, neck and body pass
# through, and the floor under it is not what is being tested. Steel's game
# rules are named after their registry path, so this is `mob_griefing` and not
# vanilla's `mobGriefing` -- the camel-case spelling is silently rejected.
CMDS="$CMDS;;gamerule mob_griefing false"

# --- the fight ---------------------------------------------------------
# Nothing below summons anything. Standing in the End is the whole input:
# `EnderDragonFight` ticks off the world, holds the arena loaded, and puts a
# dragon there by itself. Killing it is what opens the exit portal and drops
# the egg, and `EndPodiumFeature` has no other caller in vanilla or here.
CMDS="$CMDS;;execute in minecraft:the_end run teleport @s 0 120 0"
# Two throwaway commands: the arena is seventeen chunks across and the fight
# refuses to run until they are loaded, so the dragon is not there instantly.
CMDS="$CMDS;;time set day"
CMDS="$CMDS;;difficulty normal"
CMDS="$CMDS;;execute if entity @e[type=ender_dragon] run tellraw @s \"THEENDMADEADRAGONBYITSELF\""

# The podium the fight builds when the End is first entered, inactive: a
# bedrock pillar with the portal socket left as air. Y comes from the End
# island's own surface at the origin, which the central island makes the same
# in every world; if the End generator moves, re-probe rather than widening.
PODIUM_Y=63
EGG_Y=$((PODIUM_Y + 4))
CMDS="$CMDS;;execute if block 0 $PODIUM_Y 0 minecraft:bedrock run tellraw @s \"PODIUMSTANDS\""
CMDS="$CMDS;;execute if block 1 $PODIUM_Y 0 minecraft:air run tellraw @s \"PORTALSTARTSSHUT\""

CMDS="$CMDS;;kill @e[type=ender_dragon]"
CMDS="$CMDS;;execute if block 1 $PODIUM_Y 0 minecraft:end_portal run tellraw @s \"EXITPORTALOPENED\""
CMDS="$CMDS;;execute if block 0 $EGG_Y 0 minecraft:dragon_egg run tellraw @s \"DRAGONEGGDROPPED\""

# --- the hitboxes ------------------------------------------------------
# Back to the overworld, where there is no fight, for the original claim.
CMDS="$CMDS;;execute in minecraft:overworld run teleport @s 8 100 8"
CMDS="$CMDS;;summon minecraft:ender_dragon 8 100 8"
CMDS="$CMDS;;tick freeze"
CMDS="$CMDS;;execute if entity @e[type=ender_dragon] run tellraw @s \"DRAGONISHERE\""
CMDS="$CMDS;;execute if entity @e[type=ender_dragon,nbt={Health:200.0f}] run tellraw @s \"DRAGONSTARTSATFULLHEALTH\""

# The client has been told about exactly one entity, the dragon. It has never
# been told a hitbox exists, so this id is pure arithmetic -- which is the
# point.
CMDS="$CMDS;;!attack ender_dragon 3"
CMDS="$CMDS;;execute unless entity @e[type=ender_dragon,nbt={Health:200.0f}] run tellraw @s \"HITONAHITBOXREACHEDTHEDRAGON\""

# And the dragon is still alive and still a dragon: a hit on a body hitbox is
# a quarter-damage hit, not a kill.
CMDS="$CMDS;;execute if entity @e[type=ender_dragon] run tellraw @s \"DRAGONSURVIVEDTHEHIT\""

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=3 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what happened ==="
grep "server says" join.log \
  | grep -oE "THEENDMADEADRAGONBYITSELF|PODIUMSTANDS|PORTALSTARTSSHUT|EXITPORTALOPENED|DRAGONEGGDROPPED|DRAGONISHERE|DRAGONSTARTSATFULLHEALTH|HITONAHITBOXREACHEDTHEDRAGON|DRAGONSURVIVEDTHEHIT"
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic" | tail -5

fail() { echo "########## DRAGON TEST FAILED ($1) ##########"; exit 1; }
# `server says` first: join.py echoes the command being run, so grepping the
# bare marker would match the question as well as the answer.
said() { grep "server says" join.log | grep -q "$1"; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }
said THEENDMADEADRAGONBYITSELF    || fail "standing in the End did not produce a dragon"
said PODIUMSTANDS                 || fail "the fight never built the exit podium"
said PORTALSTARTSSHUT             || fail "the podium was already active before the dragon died"
said EXITPORTALOPENED             || fail "killing the dragon did not open the exit portal"
said DRAGONEGGDROPPED             || fail "the first dragon of the world left no egg"
said DRAGONISHERE                 || fail "no dragon was summoned"
said DRAGONSTARTSATFULLHEALTH     || fail "the dragon did not start on two hundred health"
said HITONAHITBOXREACHEDTHEDRAGON || fail "an attack on a hitbox id never reached the dragon"
said DRAGONSURVIVEDTHEHIT         || fail "one hit on a body hitbox killed the dragon"
echo "########## DRAGON TEST PASSED ##########"
