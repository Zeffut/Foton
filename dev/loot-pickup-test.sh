#!/bin/bash
# Drop a sword under a mob that is allowed to loot, and see whether it picks it
# up.
#
# `Mob.aiStep` sweeps for item entities and hands each one to `Mob.pickUpItem`,
# whose body equips the stack. Foton had the sweep and it had
# `equipItemIfPossible`, but nothing joined them -- so every mob that did not
# override `pickUpItem` itself walked over your gear and left it lying there.
#
# The subject is a cow rather than a zombie for two reasons. `CanPickUpLoot` is
# the one knob a command can turn, and the cow is one of the mobs whose `Mob`
# NBT round-trips -- the zombie has no `save_additional`/`load_additional` at
# all, so nothing in its summon NBT survives. And a cow neither burns at dawn
# nor comes looking for the player, so both rigs stay where they were put.
# Vanilla behaves the same way: `/summon cow ~ ~ ~ {CanPickUpLoot:1b}` produces
# a cow that picks gear up off the floor.
#
# The two rigs differ by one byte: the looter is summoned with
# `CanPickUpLoot:1b`, the control with `0b`. If the control's sword vanishes
# too, the flag is not being read and the looter's result means nothing.
#
# Usage: bash dev/loot-pickup-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

# Respect $CARGO_TARGET_DIR the way `cargo build` itself already does; see
# the same lines in dev/join-test.sh for what hardcoding it used to cost.
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
BIN="$TARGET_DIR/debug/foton"

PORT=25627
RUN_DIR="$ROOT/run-loot-pickup"

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
# A scripted client fires commands far faster than a person, and the throttle
# decays per game tick -- so a busy server turns a normal rig into a kick.
sed -i 's/^command_spam_threshold_seconds = .*/command_spam_threshold_seconds = 0/' "$RUN_DIR/config/config.toml"

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

CMDS='gamemode spectator'
CMDS="$CMDS;;time set day"
CMDS="$CMDS;;weather clear"
CMDS="$CMDS;;teleport @s 10 108 0"
# Nothing that wandered in on its own may take part.
CMDS="$CMDS;;gamerule spawn_mobs false"
CMDS="$CMDS;;kill @e[type=minecraft:cow]"
CMDS="$CMDS;;kill @e[type=minecraft:item]"

# Two platforms, twenty blocks apart, so neither cow can stroll into the
# other's sword.
for x in -1 0 1 19 20 21; do
  for z in -1 0 1; do
    CMDS="$CMDS;;setblock $x 99 $z minecraft:stone"
  done
done

# The swords go down first and are counted while nothing can have taken them:
# the world keeps ticking between commands, so a control asked after the cow
# exists is racing the very thing it is a control for.
CMDS="$CMDS;;summon minecraft:item 0 100 0 {Item:{id:\"minecraft:iron_sword\",count:1},PickupDelay:0s,Tags:[\"looterbait\"]}"
CMDS="$CMDS;;summon minecraft:item 20 100 0 {Item:{id:\"minecraft:iron_sword\",count:1},PickupDelay:0s,Tags:[\"controlbait\"]}"
CMDS="$CMDS;;execute if entity @e[type=minecraft:item,tag=looterbait] run tellraw @s \"LOOTERBAIT\""
CMDS="$CMDS;;execute if entity @e[type=minecraft:item,tag=controlbait] run tellraw @s \"CONTROLBAIT\""

# `PersistenceRequired` keeps either cow from despawning mid-test, and the tags
# are what let every assertion below name its subject without depending on
# where it has wandered to.
CMDS="$CMDS;;summon minecraft:cow 0 100 0 {CanPickUpLoot:1b,PersistenceRequired:1b,Silent:1b,Tags:[\"looter\"]}"
CMDS="$CMDS;;summon minecraft:cow 20 100 0 {CanPickUpLoot:0b,PersistenceRequired:1b,Silent:1b,Tags:[\"control\"]}"
CMDS="$CMDS;;execute if entity @e[type=minecraft:cow,tag=looter] run tellraw @s \"LOOTERUP\""
CMDS="$CMDS;;execute if entity @e[type=minecraft:cow,tag=control] run tellraw @s \"CONTROLUP\""
# And each flag really did come off the summon NBT.
CMDS="$CMDS;;execute if entity @e[type=minecraft:cow,tag=looter,nbt={CanPickUpLoot:1b}] run tellraw @s \"LOOTERFLAGGED\""
CMDS="$CMDS;;execute if entity @e[type=minecraft:cow,tag=control,nbt={CanPickUpLoot:0b}] run tellraw @s \"CONTROLUNFLAGGED\""

CMDS="$CMDS;;tick sprint 60"
CMDS="$CMDS;;!wait 3"

# The one that matters: the sword is in the looter's mouth, which happens only
# if `pickUpItem` ran its equip. `nbt={equipment:...}` reads the same saved
# shape `/data` would.
ARMED='nbt={equipment:{mainhand:{id:"minecraft:iron_sword"}}}'
CMDS="$CMDS;;execute if entity @e[type=minecraft:cow,tag=looter,$ARMED] run tellraw @s \"LOOTERARMED\""
# And the control is still empty-mouthed with its sword still on the floor.
CMDS="$CMDS;;execute if entity @e[type=minecraft:item,tag=controlbait] run tellraw @s \"CONTROLSWORDSTAYED\""
CMDS="$CMDS;;execute if entity @e[type=minecraft:cow,tag=control,$ARMED] run tellraw @s \"CONTROLARMED\""

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=3 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what the server said ==="
grep "server says" join.log | grep -oE "LOOTERUP|CONTROLUP|LOOTERBAIT|CONTROLBAIT|LOOTERFLAGGED|CONTROLUNFLAGGED|LOOTERARMED|CONTROLSWORDSTAYED|CONTROLARMED"
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "\[Error\]|panic|Unknown|Incorrect" | tail -8

# Only the server's own reply counts: join.py echoes the commands it sends.
said() { grep "server says" join.log | grep -q "$1"; }
fail() { echo "########## LOOT PICKUP TEST FAILED ($1) ##########"; exit 1; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }

said LOOTERUP         || fail "the looting cow never spawned"
said CONTROLUP        || fail "the control cow never spawned"
said LOOTERBAIT       || fail "the looter's sword never hit the ground"
said CONTROLBAIT      || fail "the control's sword never hit the ground"
said LOOTERFLAGGED    || fail "CanPickUpLoot:1b did not survive the summon NBT"
said CONTROLUNFLAGGED || fail "CanPickUpLoot:0b did not survive the summon NBT"

said LOOTERARMED        || fail "the mob never picked the sword up"
said CONTROLSWORDSTAYED || fail "a mob with CanPickUpLoot:0b took the sword anyway"
if said CONTROLARMED; then
  fail "a mob with CanPickUpLoot:0b ended up holding a sword"
fi
echo "########## LOOT PICKUP TEST PASSED ##########"
