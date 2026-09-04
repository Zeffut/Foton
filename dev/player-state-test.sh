#!/bin/bash
# Log out, restart the server, log back in: the player has to be who they were.
#
# `dev/entity-state-test.sh` asks this of a mob, whose state travels in the
# chunk file. A player's does not: it goes to their own file, which carried a
# health field and nothing else of the living half, so a player who logged out
# poisoned, shielded or hasted logged back in clean.
#
# Health is checked in three places on purpose -- full before the damage, hurt
# after it, hurt again after the restart -- because a reading that always says
# 14 and a reading that always says 20 are both wrong and both look like a pass
# from one side. The effect is asked for before the potion for the same reason.
#
# Usage: bash dev/player-state-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

# Respect $CARGO_TARGET_DIR the way `cargo build` itself already does; see
# the same lines in dev/join-test.sh for what hardcoding it used to cost.
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
BIN="$TARGET_DIR/debug/foton"

PORT=25732
RUN_DIR="$ROOT/run-player-state"

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

# A clean stop, because that is what flushes the player file the second boot
# reads. The client disconnects on its own before this, which is the write.
stop_server() {
  kill "$PID" 2>/dev/null
  for _ in $(seq 1 60); do kill -0 "$PID" 2>/dev/null || break; sleep 1; done
  kill -0 "$PID" 2>/dev/null && kill -9 "$PID" 2>/dev/null
  sleep 2
}

# Survival, because a creative player shrugs off `/damage`, and the player
# stays at the world spawn: a teleport onto a one-block perch is a long fall
# away from a rig that kills its own subject.
SETUP='gamemode survival'
SETUP="$SETUP;;difficulty normal"
SETUP="$SETUP;;gamerule spawn_mobs false"
# Regeneration hands the health back between the damage and the question, so a
# health reading comes out 20 whatever the save did.
SETUP="$SETUP;;gamerule natural_health_regeneration false"
SETUP="$SETUP;;gamerule advance_time false"
SETUP="$SETUP;;time set midnight"
SETUP="$SETUP;;weather clear"
SETUP="$SETUP;;!wait 2"

ask() {
  echo ";;execute if entity @s[nbt=$1] run tellraw @s {\"text\":\"$2\"}"
}

CHECKS=$(ask '{Health:14.0f}' PL_HURT)
CHECKS="$CHECKS$(ask '{Health:20.0f}' PL_FULL)"
CHECKS="$CHECKS$(ask '{active_effects:[{id:"minecraft:fire_resistance"}]}' PL_EFFECT)"

# The same two questions before anything has happened, under their own names:
# a reading stuck on one answer fails here rather than passing later.
BEFORE=$(ask '{Health:20.0f}' PL_BEFORE_FULL)
BEFORE="$BEFORE$(ask '{Health:14.0f}' PL_BEFORE_HURT)"
BEFORE="$BEFORE$(ask '{active_effects:[{id:"minecraft:fire_resistance"}]}' PL_BEFORE_EFFECT)"

# ---------------------------------------------------------------- first boot
echo "=== First boot: hurts the player, doses them, and lets them log out ==="
start_server first || exit 1

CMDS="$SETUP$BEFORE"
CMDS="$CMDS;;damage @s 6 minecraft:generic"
# Thrown rather than drunk: Foton has no drinking lifecycle yet, and a splash
# potion at the thrower's own feet doses them at full strength. Fire resistance
# because it neither heals nor shields, so it cannot move the health above.
CMDS="$CMDS;;give @s minecraft:splash_potion[minecraft:potion_contents={potion:\"minecraft:long_fire_resistance\"}]"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;!useitem 0 90"
CMDS="$CMDS;;!wait 3"
CMDS="$CMDS$CHECKS"

JOIN_COMMANDS="$CMDS" python3 "$ROOT/dev/join.py" "$PORT" > join-first.log 2>&1
FIRST_STATUS=$?
stop_server

# --------------------------------------------------------------- second boot
echo "=== Second boot: the same player logs back in ==="
start_server second || exit 1

# No damage and no potion this time: everything below has to come from the
# player file.
JOIN_COMMANDS="$SETUP$CHECKS" python3 "$ROOT/dev/join.py" "$PORT" > join-second.log 2>&1
SECOND_STATUS=$?
stop_server

echo "=== first boot ==="
grep -oE "server says: PL_[A-Z_]+" join-first.log | sort -u
echo "=== second boot ==="
grep -oE "server says: PL_[A-Z_]+" join-second.log | sort -u
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server-first.log server-second.log \
  | grep -iE "\[Error\]|panic" | tail -5

fail() { echo "########## PLAYER STATE TEST FAILED ($1) ##########"; exit 1; }
# Only the server's own reply counts: join.py echoes the commands it sends, and
# a bare marker would match that echo whether the condition held or not.
said() { grep -q "server says: $2" "join-$1.log"; }

[ $FIRST_STATUS -eq 0 ] || { tail -20 join-first.log; fail "the client never settled on the first boot"; }
[ $SECOND_STATUS -eq 0 ] || { tail -20 join-second.log; fail "the client never settled on the second boot"; }

said first PL_BEFORE_FULL   || fail "the player did not start at full health; the rig is broken"
said first PL_BEFORE_HURT   && fail "an untouched player read 14 health, so the reading is stuck"
said first PL_BEFORE_EFFECT && fail "the player had fire resistance before the potion"

said first PL_HURT   || fail "the player did not read 14 health right after being hurt"
said first PL_FULL   && fail "the player still read 20 health right after being hurt"
said first PL_EFFECT || fail "the splash potion never reached the player; the rig is broken"

said second PL_HURT   || fail "the player came back at a different health"
said second PL_FULL   && fail "the player came back at full health"
said second PL_EFFECT || fail "the player's potion effect did not survive the restart"

echo "########## PLAYER STATE TEST PASSED ##########"
