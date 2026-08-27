#!/bin/bash
# Stop the server and start it again: the mobs have to be what they were.
#
# Twenty of Steel's mobs had no `save_additional` at all, or one that skipped
# the shared `Mob` half, and `save_additional` is the *whole* of what the chunk
# saver writes for a type. So a restart quietly reset every one of them: a
# charged creeper came back ordinary, a size-4 slime came back tiny, a baby
# zombie grew up, and `CanPickUpLoot`, `PersistenceRequired`, `NoAI`,
# `LeftHanded` and `DeathLootTable` were thrown away for the lot.
#
# `dev/reload-test.sh` already boots a world twice, but it only asks whether the
# server comes back up -- it never asks what came back up with it. This summons
# mobs in a known state, stops the server, starts it on the same world, and asks
# the second boot's world about them.
#
# Every value below is deliberately not the default, so nothing here can pass on
# a mob that spawned that way; the plain zombie beside the baby one is the
# control that says so out loud. And every mob is asked about once on the first
# boot too, because a summon that silently did nothing would otherwise look
# exactly like a save that silently dropped everything.
#
# Usage: bash dev/mob-persist-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25712
RUN_DIR="$ROOT/run-mob-persist"

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
  nohup "$ROOT/target/debug/steel" > "server-$1.log" 2>&1 < /dev/null &
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

# The world the mobs are put in, shared by both boots.
SETUP='gamemode creative'
SETUP="$SETUP;;difficulty normal"
# Peaceful would delete them and natural spawns would pollute the selectors,
# which both read as "the save lost them".
SETUP="$SETUP;;gamerule spawn_mobs false"
# Undead burn in daylight, and these stand in open sky. Left at noon the
# zombies died partway through the very first boot's own checks, which looks
# exactly like a save that dropped them. Midnight plus a frozen clock is what
# keeps "the mob is gone" from ever meaning "the sun came up".
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
CHECKS=$(alive mp_zombie MP_ZOMBIE_ALIVE)
CHECKS="$CHECKS$(alive mp_plain MP_PLAIN_ALIVE)"
CHECKS="$CHECKS$(alive mp_slime MP_SLIME_ALIVE)"
CHECKS="$CHECKS$(alive mp_creeper MP_CREEPER_ALIVE)"
CHECKS="$CHECKS$(alive mp_blaze MP_BLAZE_ALIVE)"

CHECKS="$CHECKS$(ask mp_zombie '{IsBaby:1b}' MP_BABY)"
CHECKS="$CHECKS$(ask mp_zombie '{CanPickUpLoot:1b}' MP_PICKUP)"
CHECKS="$CHECKS$(ask mp_zombie '{PersistenceRequired:1b}' MP_PERSIST)"
CHECKS="$CHECKS$(ask mp_zombie '{LeftHanded:1b}' MP_LEFTY)"
CHECKS="$CHECKS$(ask mp_zombie '{NoAI:1b}' MP_NOAI)"
CHECKS="$CHECKS$(ask mp_slime '{Size:3}' MP_SIZE)"
CHECKS="$CHECKS$(ask mp_creeper '{powered:1b}' MP_CHARGED)"
CHECKS="$CHECKS$(ask mp_creeper '{Fuse:45s}' MP_FUSE)"
CHECKS="$CHECKS$(ask mp_creeper '{ExplosionRadius:7b}' MP_RADIUS)"
CHECKS="$CHECKS$(ask mp_blaze '{DeathLootTable:"minecraft:entities/creeper"}' MP_LOOTTABLE)"
# The control: a zombie summoned with none of this must not answer any of it.
CHECKS="$CHECKS$(ask mp_plain '{IsBaby:1b}' MP_PLAIN_BABY)"
CHECKS="$CHECKS$(ask mp_plain '{CanPickUpLoot:1b}' MP_PLAIN_PICKUP)"

# ---------------------------------------------------------------- first boot
echo "=== First boot: summons the mobs and stops cleanly ==="
start_server first || exit 1

CMDS="$SETUP"
CMDS="$CMDS;;summon minecraft:zombie 0 150 2 {IsBaby:1b,CanPickUpLoot:1b,PersistenceRequired:1b,LeftHanded:1b,NoAI:1b,NoGravity:1b,Tags:[\"mp_zombie\"]}"
CMDS="$CMDS;;summon minecraft:zombie 0 150 4 {NoAI:1b,NoGravity:1b,PersistenceRequired:1b,Tags:[\"mp_plain\"]}"
CMDS="$CMDS;;summon minecraft:slime 0 150 6 {Size:3,PersistenceRequired:1b,NoAI:1b,NoGravity:1b,Tags:[\"mp_slime\"]}"
CMDS="$CMDS;;summon minecraft:creeper 0 150 8 {powered:1b,Fuse:45s,ExplosionRadius:7b,PersistenceRequired:1b,NoAI:1b,NoGravity:1b,Tags:[\"mp_creeper\"]}"
CMDS="$CMDS;;summon minecraft:blaze 0 150 10 {DeathLootTable:\"minecraft:entities/creeper\",PersistenceRequired:1b,NoAI:1b,NoGravity:1b,Tags:[\"mp_blaze\"]}"
CMDS="$CMDS;;!wait 2"
CMDS="$CMDS$CHECKS"

JOIN_COMMANDS="$CMDS" python3 "$ROOT/dev/join.py" "$PORT" > join-first.log 2>&1
FIRST_STATUS=$?
stop_server

# --------------------------------------------------------------- second boot
echo "=== Second boot: reads them back off disk ==="
start_server second || exit 1

# No summons this time: everything below has to come from the region files.
JOIN_COMMANDS="$SETUP$CHECKS" python3 "$ROOT/dev/join.py" "$PORT" > join-second.log 2>&1
SECOND_STATUS=$?
stop_server

echo "=== first boot ==="
grep -oE "server says: MP_[A-Z_]+" join-first.log | sort -u
echo "=== second boot ==="
grep -oE "server says: MP_[A-Z_]+" join-second.log | sort -u
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server-first.log server-second.log \
  | grep -iE "\[Error\]|panic" | tail -5

fail() { echo "########## MOB PERSIST TEST FAILED ($1) ##########"; exit 1; }
# Only the server's own reply counts: join.py echoes the commands it sends, and
# a bare marker would match that echo whether the condition held or not.
said() { grep -q "server says: $2" "join-$1.log"; }

[ $FIRST_STATUS -eq 0 ] || { tail -20 join-first.log; fail "the client never settled on the first boot"; }
[ $SECOND_STATUS -eq 0 ] || { tail -20 join-second.log; fail "the client never settled on the second boot"; }

ALIVE="MP_ZOMBIE_ALIVE MP_PLAIN_ALIVE MP_SLIME_ALIVE MP_CREEPER_ALIVE MP_BLAZE_ALIVE"
STATE="MP_BABY MP_PICKUP MP_PERSIST MP_LEFTY MP_NOAI MP_SIZE MP_CHARGED MP_FUSE MP_RADIUS MP_LOOTTABLE"

for marker in $ALIVE $STATE; do
  said first "$marker" || fail "$marker was not true even before the restart; the rig is broken"
done
said first MP_PLAIN_BABY && fail "a zombie summoned with no NBT came out a baby anyway"
said first MP_PLAIN_PICKUP && fail "a zombie summoned with no NBT could pick up loot anyway"

for marker in $ALIVE; do
  said second "$marker" || fail "$marker: the mob itself did not survive the restart"
done

said second MP_BABY       || fail "the baby zombie grew up over the restart"
said second MP_PICKUP     || fail "CanPickUpLoot did not survive the restart"
said second MP_PERSIST    || fail "PersistenceRequired did not survive the restart"
said second MP_LEFTY      || fail "LeftHanded did not survive the restart"
said second MP_NOAI       || fail "NoAI did not survive the restart"
said second MP_SIZE       || fail "the slime forgot its size over the restart"
said second MP_CHARGED    || fail "the charged creeper came back ordinary"
said second MP_FUSE       || fail "the creeper's fuse length did not survive the restart"
said second MP_RADIUS     || fail "the creeper's blast radius did not survive the restart"
said second MP_LOOTTABLE  || fail "DeathLootTable did not survive the restart"
said second MP_PLAIN_BABY && fail "the plain zombie came back a baby, so the selector matches anything"

echo "########## MOB PERSIST TEST PASSED ##########"
