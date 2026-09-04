#!/bin/bash
# Stop the server and start it again: a mob has to still be as hurt as it was.
#
# `dev/mob-persist-test.sh` asks whether a mob's own type-specific state came
# back. This asks about the half every living entity shares -- health, potion
# effects, absorption, attribute modifiers -- which the chunk saver wrote
# nowhere and read from nowhere, so every mob in the world came back at full
# health with nothing on it.
#
# Health is not summoned here, it is *inflicted*: `/damage` on the first boot
# makes it live runtime state rather than a value handed to the loader, so a
# save path that only echoes back what it was given cannot pass. The effects,
# the absorption and the attribute modifier are summoned, and a matching
# control mob is summoned without them, so a selector that matches anything
# fails out loud on the first boot instead of passing on the second.
#
# Usage: bash dev/entity-state-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

# Respect $CARGO_TARGET_DIR the way `cargo build` itself already does; see
# the same lines in dev/join-test.sh for what hardcoding it used to cost.
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
BIN="$TARGET_DIR/debug/foton"

PORT=25731
RUN_DIR="$ROOT/run-entity-state"

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
# A scripted client fires commands far faster than a person, and the throttle
# decays per game tick -- so a busy server turns a normal rig into a kick.
sed -i 's/^command_spam_threshold_seconds = .*/command_spam_threshold_seconds = 0/' \
  "$RUN_DIR/config/config.toml"
sed -i 's/^default_groups = .*/default_groups = ["op"]/' "$RUN_DIR/config/groups.toml"

cd "$RUN_DIR" || exit 1

wait_for_port() {
  for _ in $(seq 1 180); do
    ss -ltn 2>/dev/null | grep -q ":$PORT" && return 0
    sleep 1
  done
  return 1
}

PID=
start_server() {
  # stdin from /dev/null: the server reads console commands, and a background
  # process that reads a terminal is stopped by SIGTTIN instead of running.
  nohup "$BIN" > "server-$1.log" 2>&1 < /dev/null &
  PID=$!
  if ! wait_for_port; then
    echo "SERVER NEVER LISTENED ON $PORT ($1)"
    sed 's/\x1b\[[0-9;]*[A-Za-z]//g' "server-$1.log" | tail -20
    kill -9 "$PID" 2>/dev/null
    return 1
  fi
}

# A clean stop, because that is what flushes the chunks the second boot reads.
stop_server() {
  kill "$PID" 2>/dev/null
  for _ in $(seq 1 60); do kill -0 "$PID" 2>/dev/null || break; sleep 1; done
  kill -0 "$PID" 2>/dev/null && kill -9 "$PID" 2>/dev/null
  sleep 2
}

SETUP='gamemode creative'
SETUP="$SETUP;;difficulty normal"
# Natural spawns would pollute the selectors, which reads as "the save lost it".
SETUP="$SETUP;;gamerule spawn_mobs false"
# Regeneration hands health back between the damage and the question, so a
# health reading comes out 20 whatever the save did.
SETUP="$SETUP;;gamerule natural_health_regeneration false"
# Undead burn in daylight, and these stand in open sky. Midnight plus a frozen
# clock keeps "the mob is gone" from ever meaning "the sun came up".
SETUP="$SETUP;;gamerule advance_time false"
SETUP="$SETUP;;time set midnight"
SETUP="$SETUP;;weather clear"
# One throwaway command first: the very first command of a run can land before
# the chunk around the player is ready.
SETUP="$SETUP;;setblock 0 149 0 minecraft:stone"
SETUP="$SETUP;;teleport @s 0 150 0"
SETUP="$SETUP;;!wait 2"

# Everything is asked for by tag, and every mob is `NoGravity` so it stays in
# the chunk it was summoned in rather than falling out of the test.
ask() {
  echo ";;execute if entity @e[tag=$1,nbt=$2,distance=..30] run tellraw @s {\"text\":\"$3\"}"
}
alive() {
  echo ";;execute if entity @e[tag=$1,distance=..30] run tellraw @s {\"text\":\"$2\"}"
}

# Each mob says it is there before it is asked what it is, so a missing answer
# below reads as "this state was lost" rather than "this mob was lost".
CHECKS=$(alive es_hurt ES_HURT_ALIVE)
CHECKS="$CHECKS$(alive es_plain ES_PLAIN_ALIVE)"
CHECKS="$CHECKS$(alive es_state ES_STATE_ALIVE)"

# 20 - 6 = 14, and nothing gives a mob health back with regeneration off.
CHECKS="$CHECKS$(ask es_hurt '{Health:14.0f}' ES_HEALTH)"
# The fingerprint of the bug, printed rather than only asserted: a mob that
# came back at full health says so out loud.
CHECKS="$CHECKS$(ask es_hurt '{Health:20.0f}' ES_HEALED)"
# The control: an untouched zombie of the same type is at 20, which is what
# proves the reading above can tell one number from the other.
CHECKS="$CHECKS$(ask es_plain '{Health:20.0f}' ES_PLAIN_FULL)"
CHECKS="$CHECKS$(ask es_plain '{Health:14.0f}' ES_PLAIN_HURT)"

CHECKS="$CHECKS$(ask es_state '{AbsorptionAmount:6.0f}' ES_ABSORPTION)"
CHECKS="$CHECKS$(ask es_state '{active_effects:[{id:"minecraft:strength"}]}' ES_EFFECT)"
CHECKS="$CHECKS$(ask es_state '{active_effects:[{amplifier:3b}]}' ES_EFFECT_AMPLIFIER)"
CHECKS="$CHECKS$(ask es_state '{attributes:[{id:"minecraft:max_health",base:30.0d}]}' ES_ATTRIBUTE_BASE)"
CHECKS="$CHECKS$(ask es_state '{attributes:[{modifiers:[{id:"foton:test_speed"}]}]}' ES_ATTRIBUTE_MODIFIER)"
CHECKS="$CHECKS$(ask es_state '{equipment:{head:{id:"minecraft:diamond_helmet"}}}' ES_EQUIPMENT)"
# The control again, on the state mob's keys: a plain zombie must answer none
# of them.
CHECKS="$CHECKS$(ask es_plain '{AbsorptionAmount:6.0f}' ES_PLAIN_ABSORPTION)"
CHECKS="$CHECKS$(ask es_plain '{active_effects:[{id:"minecraft:strength"}]}' ES_PLAIN_EFFECT)"
CHECKS="$CHECKS$(ask es_plain '{attributes:[{modifiers:[{id:"foton:test_speed"}]}]}' ES_PLAIN_MODIFIER)"
CHECKS="$CHECKS$(ask es_plain '{equipment:{head:{id:"minecraft:diamond_helmet"}}}' ES_PLAIN_EQUIPMENT)"

STATE_NBT='{NoAI:1b,NoGravity:1b,PersistenceRequired:1b,Tags:["es_state"],'
STATE_NBT="$STATE_NBT"'AbsorptionAmount:6.0f,'
STATE_NBT="$STATE_NBT"'equipment:{head:{id:"minecraft:diamond_helmet",count:1}},'
STATE_NBT="$STATE_NBT"'active_effects:[{id:"minecraft:strength",amplifier:3b,duration:100000,show_particles:0b}],'
STATE_NBT="$STATE_NBT"'attributes:[{id:"minecraft:max_health",base:30.0d},'
STATE_NBT="$STATE_NBT"'{id:"minecraft:movement_speed",base:0.23d,modifiers:[{id:"foton:test_speed",amount:0.5d,operation:"add_value"}]}]}'

# ---------------------------------------------------------------- first boot
echo "=== First boot: hurts a mob, summons a loaded one, stops cleanly ==="
start_server first || exit 1

CMDS="$SETUP"
CMDS="$CMDS;;summon minecraft:zombie 0 150 2 {NoAI:1b,NoGravity:1b,PersistenceRequired:1b,Tags:[\"es_hurt\"]}"
CMDS="$CMDS;;summon minecraft:zombie 0 150 4 {NoAI:1b,NoGravity:1b,PersistenceRequired:1b,Tags:[\"es_plain\"]}"
CMDS="$CMDS;;summon minecraft:zombie 0 150 6 $STATE_NBT"
CMDS="$CMDS;;!wait 2"
# `limit=1`: /damage takes a single entity, and a selector that could
# match more is refused outright rather than damaging the first match.
CMDS="$CMDS;;damage @e[tag=es_hurt,distance=..30,limit=1] 6 minecraft:generic"
CMDS="$CMDS;;!wait 2"
CMDS="$CMDS$CHECKS"

JOIN_COMMANDS="$CMDS" python3 "$ROOT/dev/join.py" "$PORT" > join-first.log 2>&1
FIRST_STATUS=$?
stop_server

# --------------------------------------------------------------- second boot
echo "=== Second boot: reads them back off disk ==="
start_server second || exit 1

# No summons and no damage this time: everything below has to come from the
# region files.
JOIN_COMMANDS="$SETUP$CHECKS" python3 "$ROOT/dev/join.py" "$PORT" > join-second.log 2>&1
SECOND_STATUS=$?
stop_server

echo "=== first boot ==="
grep -oE "server says: ES_[A-Z_]+" join-first.log | sort -u
echo "=== second boot ==="
grep -oE "server says: ES_[A-Z_]+" join-second.log | sort -u
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server-first.log server-second.log \
  | grep -iE "\[Error\]|panic" | tail -5

fail() { echo "########## ENTITY STATE TEST FAILED ($1) ##########"; exit 1; }
# Only the server's own reply counts: join.py echoes the commands it sends, and
# a bare marker would match that echo whether the condition held or not.
said() { grep -q "server says: $2" "join-$1.log"; }

[ $FIRST_STATUS -eq 0 ] || { tail -20 join-first.log; fail "the client never settled on the first boot"; }
[ $SECOND_STATUS -eq 0 ] || { tail -20 join-second.log; fail "the client never settled on the second boot"; }

ALIVE="ES_HURT_ALIVE ES_PLAIN_ALIVE ES_STATE_ALIVE"
STATE="ES_HEALTH ES_ABSORPTION ES_EFFECT ES_EFFECT_AMPLIFIER ES_ATTRIBUTE_BASE ES_ATTRIBUTE_MODIFIER ES_EQUIPMENT"

for marker in $ALIVE $STATE ES_PLAIN_FULL; do
  said first "$marker" || fail "$marker was not true even before the restart; the rig is broken"
done
said first ES_HEALED           && fail "the damaged zombie read 20 health right after being hurt"
said first ES_PLAIN_HURT       && fail "an undamaged zombie read 14 health, so the selector matches anything"
said first ES_PLAIN_ABSORPTION && fail "a zombie summoned with no NBT had absorption anyway"
said first ES_PLAIN_EFFECT     && fail "a zombie summoned with no NBT had an effect anyway"
said first ES_PLAIN_MODIFIER   && fail "a zombie summoned with no NBT had an attribute modifier anyway"
said first ES_PLAIN_EQUIPMENT  && fail "a zombie summoned with no NBT wore a diamond helmet anyway"

for marker in $ALIVE; do
  said second "$marker" || fail "$marker: the mob itself did not survive the restart"
done

said second ES_HEALTH             || fail "the hurt zombie did not keep its health over the restart"
said second ES_HEALED             && fail "the hurt zombie came back at full health"
said second ES_ABSORPTION         || fail "AbsorptionAmount did not survive the restart"
said second ES_EFFECT             || fail "the potion effect did not survive the restart"
said second ES_EFFECT_AMPLIFIER   || fail "the effect came back at the wrong amplifier"
said second ES_ATTRIBUTE_BASE     || fail "the attribute base value did not survive the restart"
said second ES_ATTRIBUTE_MODIFIER || fail "the attribute modifier did not survive the restart"
said second ES_EQUIPMENT          || fail "the mob's helmet did not survive the restart"
said second ES_PLAIN_FULL         || fail "the control zombie did not come back at full health"
said second ES_PLAIN_EFFECT       && fail "the control zombie came back with an effect, so the selector matches anything"

echo "########## ENTITY STATE TEST PASSED ##########"
