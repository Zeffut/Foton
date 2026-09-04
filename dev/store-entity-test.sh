#!/bin/bash
# Store a command's result into a live entity and ask the world for it back.
#
# `/execute store ... entity` is the only command that writes NBT into a mob
# that is already standing in the world. Vanilla does it by handing the whole
# compound back through `Entity.load`, so the interesting failures are not
# "the value did not land" but "everything else went with it": a store that
# builds a fresh compound instead of editing the entity's own would set the
# field and wipe the name, the tags and the health in the same breath.
#
# So every probe here asks for two things at once -- the stored value *and* a
# tag the pig had before the store. A selector that still finds the pig by tag
# is the proof the rest of it survived.
#
# The fields are chosen for being inert. `Air`, `Fire`, `PortalCooldown` and
# `TicksFrozen` all walk back towards their resting value on the next tick, so
# a test built on them reads the tick, not the store. `data` is a datapack's
# own compound and nothing touches it; `Health` on a mob nothing is hurting
# stays where it is put.
#
# Usage: bash dev/store-entity-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

# Respect $CARGO_TARGET_DIR the way `cargo build` itself already does; see
# the same lines in dev/join-test.sh for what hardcoding it used to cost.
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
BIN="$TARGET_DIR/debug/foton"

PORT=25713
RUN_DIR="$ROOT/run-store-entity"

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
sed -i 's/^command_spam_threshold_seconds = .*/command_spam_threshold_seconds = 0/' \
  "$RUN_DIR/config/config.toml"
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
CMDS="$CMDS;;difficulty normal"
CMDS="$CMDS;;time set noon"
# One throwaway command first: the very first `setblock` of a run can land
# before the chunk around the player is ready.
CMDS="$CMDS;;setblock 0 149 0 minecraft:stone"
CMDS="$CMDS;;teleport @s 0 150 0"
CMDS="$CMDS;;!wait 2"
for z in 0 3 5 7 9 11; do
  CMDS="$CMDS;;setblock 0 149 $z minecraft:stone"
done
CMDS="$CMDS;;!wait 1"

# --- the empty room ------------------------------------------------------
CMDS="$CMDS;;execute if entity @e[tag=store_pig,distance=..20] run tellraw @s {\"text\":\"STO_PRE_PIG\"}"
CMDS="$CMDS;;execute if entity @e[tag=store_counter,distance=..20] run tellraw @s {\"text\":\"STO_PRE_COUNTER\"}"

# Three chickens are the number the stored command reports: `execute if entity`
# returns how many it matched, so the value written below is 3 times the scale
# and not something a default could produce.
CMDS="$CMDS;;summon minecraft:chicken 0 150 3 {Tags:[\"store_counter\"]}"
CMDS="$CMDS;;summon minecraft:chicken 0 150 5 {Tags:[\"store_counter\"]}"
CMDS="$CMDS;;summon minecraft:chicken 0 150 7 {Tags:[\"store_counter\"]}"
CMDS="$CMDS;;summon minecraft:pig 0 150 9 {Tags:[\"store_pig\"]}"
CMDS="$CMDS;;summon minecraft:pig 0 150 11 {Tags:[\"store_plain\"]}"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute if entity @e[tag=store_counter,distance=..20] run tellraw @s {\"text\":\"STO_COUNTER_READY\"}"

# --- store result into a compound the entity did not have ----------------
CMDS="$CMDS;;execute store result entity @n[tag=store_pig,distance=..20] data.counter int 41 run execute if entity @e[tag=store_counter,distance=..20]"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute if entity @e[tag=store_pig,nbt={data:{counter:123}},distance=..20] run tellraw @s {\"text\":\"STO_RESULT\"}"
CMDS="$CMDS;;execute if entity @e[tag=store_plain,nbt={data:{counter:123}},distance=..20] run tellraw @s {\"text\":\"STO_PLAIN_GOT_IT\"}"

# --- a second store must not take the first one with it ------------------
# `store success` is 1 for a condition that held, where `store result` would
# have written the count. A byte of 1 is what says the two are wired apart.
CMDS="$CMDS;;execute store success entity @n[tag=store_pig,distance=..20] data.flag byte 1 run execute if entity @e[tag=store_counter,distance=..20]"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute if entity @e[tag=store_pig,nbt={data:{flag:1b}},distance=..20] run tellraw @s {\"text\":\"STO_SUCCESS\"}"
CMDS="$CMDS;;execute if entity @e[tag=store_pig,nbt={data:{flag:3b}},distance=..20] run tellraw @s {\"text\":\"STO_SUCCESS_WAS_RESULT\"}"
CMDS="$CMDS;;execute if entity @e[tag=store_pig,nbt={data:{counter:123,flag:1b}},distance=..20] run tellraw @s {\"text\":\"STO_KEPT_BOTH\"}"

# --- the living half, on a mob that is already alive ---------------------
CMDS="$CMDS;;execute store result entity @n[tag=store_pig,distance=..20] Health float 0.5 run execute if entity @e[tag=store_counter,distance=..20]"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute if entity @e[tag=store_pig,nbt={Health:1.5f},distance=..20] run tellraw @s {\"text\":\"STO_HEALTH\"}"
CMDS="$CMDS;;execute if entity @e[tag=store_pig,nbt={data:{counter:123}},distance=..20] run tellraw @s {\"text\":\"STO_STILL_HAS_COUNTER\"}"

# --- a player is refused, and the server carries on ----------------------
CMDS="$CMDS;;execute store result entity @s data.counter int 41 run execute if entity @e[tag=store_counter,distance=..20]"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute if entity @s[nbt={data:{counter:123}}] run tellraw @s {\"text\":\"STO_PLAYER_TOOK_IT\"}"
CMDS="$CMDS;;tellraw @s {\"text\":\"STO_ALIVE\"}"

CMDS="$CMDS;;kill @e[tag=store_counter,distance=..20]"

export JOIN_COMMANDS="$CMDS"
python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what happened ==="
grep -E "server says" join.log
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic" | tail -5

fail() { echo "########## STORE ENTITY TEST FAILED ($1) ##########"; exit 1; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }

# Grepping the whole log would also match the echo of the command that asks
# the question, which is printed whether the condition held or not.
said() { grep -q "server says: $1" join.log; }

said STO_PRE_PIG && fail "a tagged pig was already in range before the summons"
said STO_PRE_COUNTER && fail "a counter chicken was already in range before the summons"
said STO_COUNTER_READY || fail "the three chickens the stored command counts were never summoned"

said STO_RESULT || fail "execute store result never reached the pig's data compound"
said STO_PLAIN_GOT_IT && fail "the store landed on a pig it was not aimed at"

said STO_SUCCESS || fail "execute store success never reached the pig"
said STO_SUCCESS_WAS_RESULT && fail "execute store success wrote the result instead of 1"
said STO_KEPT_BOTH || fail "the second store threw away what the first one wrote"

said STO_HEALTH || fail "execute store result never reached the living half"
said STO_STILL_HAS_COUNTER || fail "storing Health threw away the pig's data compound"

said STO_PLAYER_TOOK_IT && fail "a store aimed at a player changed the player"
said STO_ALIVE || fail "the server stopped answering after a store aimed at a player"

echo "########## STORE ENTITY TEST PASSED ##########"
