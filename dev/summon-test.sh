#!/bin/bash
# Summon configured entities the way a datapack or a test harness does.
#
# `/summon <entity> <pos> <nbt>` is what makes an entity a *specific* entity
# rather than whichever one the game felt like rolling. Without it there was no
# way to put a tamed, chested donkey in a world to test its inventory, and no
# way to stand a rider on a mount on purpose.
#
# Three separate things have to work, and each is asked for on its own so a
# failure says which:
#   - the base fields vanilla `Entity.load` owns (`Tags` here, because a tag is
#     the one piece of entity state a selector can ask about directly);
#   - the type's own reader (`Tame` and `ChestedHorse` on a donkey), which is a
#     different code path and is checked against a plain donkey summoned beside
#     it so a pass cannot come from a donkey that was tame anyway;
#   - `Passengers`, checked through `execute ... on passengers`, which reads the
#     riding relationship rather than the compound that asked for it.
#
# Every probe is asked once before anything is summoned. Those runs have to come
# back empty; if the world already had a tagged donkey in range the rest of this
# would be reading the scenery.
#
# Usage: bash dev/summon-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

# Respect $CARGO_TARGET_DIR the way `cargo build` itself already does; see
# the same lines in dev/join-test.sh for what hardcoding it used to cost.
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
BIN="$TARGET_DIR/debug/foton"

PORT=25706
RUN_DIR="$ROOT/run-summon"

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
for z in 0 3 4 6 8 10 12; do
  CMDS="$CMDS;;setblock 0 149 $z minecraft:stone"
done
CMDS="$CMDS;;!wait 1"

# --- the empty room ------------------------------------------------------
CMDS="$CMDS;;execute if entity @e[tag=summon_marker,distance=..20] run tellraw @s {\"text\":\"SUM_PRE_TAG\"}"
CMDS="$CMDS;;execute if entity @e[type=minecraft:donkey,distance=..20] run tellraw @s {\"text\":\"SUM_PRE_DONKEY\"}"
CMDS="$CMDS;;execute as @e[type=minecraft:pig,distance=..20] on passengers run tellraw @a {\"text\":\"SUM_PRE_RIDING\"}"

# --- a base field the compound asked for ---------------------------------
CMDS="$CMDS;;summon minecraft:pig 0 150 3 {Tags:[\"summon_marker\"]}"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute if entity @e[type=minecraft:pig,tag=summon_marker,distance=..20] run tellraw @s {\"text\":\"SUM_TAG\"}"

# --- the argument's type beats an `id` in the compound -------------------
CMDS="$CMDS;;summon minecraft:pig 0 150 4 {id:\"minecraft:cow\",Tags:[\"summon_idwin\"]}"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute if entity @e[type=minecraft:pig,tag=summon_idwin,distance=..20] run tellraw @s {\"text\":\"SUM_IDWIN\"}"
CMDS="$CMDS;;execute if entity @e[type=minecraft:cow,tag=summon_idwin,distance=..20] run tellraw @s {\"text\":\"SUM_IDLOST\"}"

# --- the type's own reader -----------------------------------------------
# The plain donkey is the control: if `nbt={Tame:1b}` matched a donkey nobody
# tamed, the tamed one proving nothing would go unnoticed.
CMDS="$CMDS;;summon minecraft:donkey 0 150 6 {Tags:[\"summon_plain\"]}"
CMDS="$CMDS;;summon minecraft:donkey 0 150 8 {Tame:1b,ChestedHorse:1b,Tags:[\"summon_tamed\"]}"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute if entity @e[type=minecraft:donkey,tag=summon_tamed,nbt={Tame:1b},distance=..20] run tellraw @s {\"text\":\"SUM_TAME\"}"
CMDS="$CMDS;;execute if entity @e[type=minecraft:donkey,tag=summon_tamed,nbt={ChestedHorse:1b},distance=..20] run tellraw @s {\"text\":\"SUM_CHEST\"}"
CMDS="$CMDS;;execute if entity @e[type=minecraft:donkey,tag=summon_plain,nbt={Tame:1b},distance=..20] run tellraw @s {\"text\":\"SUM_PLAIN_WAS_TAME\"}"

# --- a rider that was asked for ------------------------------------------
CMDS="$CMDS;;summon minecraft:pig 0 150 10 {Tags:[\"summon_vehicle\"],Passengers:[{id:\"minecraft:zombie\",Tags:[\"summon_rider\"]}]}"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute if entity @e[type=minecraft:zombie,tag=summon_rider,distance=..20] run tellraw @s {\"text\":\"SUM_RIDER\"}"
CMDS="$CMDS;;execute as @e[tag=summon_vehicle,distance=..20] on passengers run tellraw @a {\"text\":\"SUM_RIDING\"}"

# --- a custom name -------------------------------------------------------
CMDS="$CMDS;;summon minecraft:pig 0 150 12 {CustomName:{text:\"SummonedPig\"},Tags:[\"summon_named\"]}"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute if entity @e[type=minecraft:pig,name=SummonedPig,distance=..20] run tellraw @s {\"text\":\"SUM_NAME\"}"

CMDS="$CMDS;;kill @e[tag=summon_marker,distance=..20]"

export JOIN_COMMANDS="$CMDS"
python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what happened ==="
grep -E "server says" join.log
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic" | tail -5

fail() { echo "########## SUMMON TEST FAILED ($1) ##########"; exit 1; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }

# Grepping the whole log would also match the echo of the command that asks
# the question, which is printed whether the condition held or not.
said() { grep -q "server says: $1" join.log; }

said SUM_PRE_TAG && fail "a tagged entity was already in range before the summons"
said SUM_PRE_DONKEY && fail "a donkey was already in range before the summons"
said SUM_PRE_RIDING && fail "a ridden pig was already in range before the summons"

said SUM_TAG || fail "Tags from the summon compound never reached the entity"
said SUM_IDWIN || fail "the entity argument's type lost to an id in the compound"
said SUM_IDLOST && fail "an id in the compound overrode the entity argument"

said SUM_TAME || fail "Tame from the summon compound never reached the donkey"
said SUM_CHEST || fail "ChestedHorse from the summon compound never reached the donkey"
said SUM_PLAIN_WAS_TAME && fail "a donkey summoned with no NBT came out tame anyway"

said SUM_RIDER || fail "the passenger in the summon compound was never created"
said SUM_RIDING || fail "the passenger was created but never seated on its vehicle"

said SUM_NAME || fail "CustomName from the summon compound never reached the entity"

echo "########## SUMMON TEST PASSED ##########"
