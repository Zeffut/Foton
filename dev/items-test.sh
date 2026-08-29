#!/bin/bash
# Ask the world how many of something is in a slot.
#
# `execute if items` counts *items*, not slots: seventeen stone in one slot is
# seventeen, and that is the number the command returns. Getting that wrong is
# the failure this rig is built around, because a condition that answers 1
# still passes every "did it find any" test anyone would write by eye.
#
# So the count is read back rather than looked at: `execute store result
# entity` puts it into a marker's `data` compound and a selector asks the
# marker what it got. Both halves of the command are driven that way, and both
# have a control that would fire if the count were the number of slots.
#
# The other thing worth proving is what happens at a block that is not a
# container. Vanilla throws there; a silent zero would make `execute unless
# items block` succeed on a stone block, which is a condition quietly answering
# a question nobody can see it got wrong. The `unless` probe below is what
# tells those two apart.
#
# Usage: bash dev/items-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25714
RUN_DIR="$ROOT/run-items"
TABLE='minecraft:chests/simple_dungeon'

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
sed -i 's/^command_spam_threshold_seconds = .*/command_spam_threshold_seconds = 0/' \
  "$RUN_DIR/config/config.toml"
sed -i 's/^default_groups = .*/default_groups = ["op"]/' "$RUN_DIR/config/groups.toml"

cd "$RUN_DIR" || exit 1
nohup "$ROOT/target/debug/foton" > server.log 2>&1 < /dev/null &
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

# The marker every count is written into.
CMDS="$CMDS;;summon minecraft:armor_stand 0 150 11 {Tags:[\"items_probe\"],NoGravity:1b,Invulnerable:1b}"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute if entity @e[tag=items_probe,distance=..20] run tellraw @s {\"text\":\"ITM_PROBE\"}"

# --- the empty room ------------------------------------------------------
CMDS="$CMDS;;execute if items entity @s hotbar.* minecraft:stone run tellraw @s {\"text\":\"ITM_PRE_STONE\"}"

# --- an entity, counting items and not slots -----------------------------
CMDS="$CMDS;;give @s minecraft:stone 17"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute store result entity @n[tag=items_probe,distance=..20] data.hand int 1 run execute if items entity @s hotbar.* minecraft:stone"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute if entity @e[tag=items_probe,nbt={data:{hand:17}},distance=..20] run tellraw @s {\"text\":\"ITM_ENTITY_COUNT\"}"
CMDS="$CMDS;;execute if entity @e[tag=items_probe,nbt={data:{hand:1}},distance=..20] run tellraw @s {\"text\":\"ITM_ENTITY_COUNTED_SLOTS\"}"
CMDS="$CMDS;;execute unless items entity @s hotbar.* minecraft:diamond run tellraw @s {\"text\":\"ITM_ENTITY_UNLESS\"}"

# --- a mob's equipment, which is a different slot family -----------------
CMDS="$CMDS;;summon minecraft:zombie 0 150 3 {Tags:[\"items_zombie\"],equipment:{head:{id:\"minecraft:diamond_helmet\",count:1}}}"
CMDS="$CMDS;;summon minecraft:zombie 0 150 5 {Tags:[\"items_bare\"]}"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute if items entity @e[tag=items_zombie,distance=..20] armor.head minecraft:diamond_helmet run tellraw @s {\"text\":\"ITM_ARMOR\"}"
CMDS="$CMDS;;execute if items entity @e[tag=items_bare,distance=..20] armor.head minecraft:diamond_helmet run tellraw @s {\"text\":\"ITM_BARE_WAS_ARMORED\"}"

# --- an entity that is a container --------------------------------------
CMDS="$CMDS;;summon minecraft:chest_minecart 0 150 9 {Tags:[\"items_cart\"],Items:[{Slot:0b,id:\"minecraft:gold_ingot\",count:7}]}"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute store result entity @n[tag=items_probe,distance=..20] data.cart int 1 run execute if items entity @e[tag=items_cart,distance=..20] container.* minecraft:gold_ingot"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute if entity @e[tag=items_probe,nbt={data:{cart:7}},distance=..20] run tellraw @s {\"text\":\"ITM_CART\"}"

# --- a container block, counting across two slots ------------------------
CMDS="$CMDS;;setblock 0 150 3 minecraft:chest{Items:[{Slot:0b,id:\"minecraft:diamond\",count:5},{Slot:9b,id:\"minecraft:diamond\",count:3}]}"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute store result entity @n[tag=items_probe,distance=..20] data.chest int 1 run execute if items block 0 150 3 container.* minecraft:diamond"
CMDS="$CMDS;;execute store result entity @n[tag=items_probe,distance=..20] data.one int 1 run execute if items block 0 150 3 container.0 minecraft:diamond"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute if entity @e[tag=items_probe,nbt={data:{chest:8}},distance=..20] run tellraw @s {\"text\":\"ITM_BLOCK_COUNT\"}"
CMDS="$CMDS;;execute if entity @e[tag=items_probe,nbt={data:{chest:2}},distance=..20] run tellraw @s {\"text\":\"ITM_BLOCK_COUNTED_SLOTS\"}"
CMDS="$CMDS;;execute if entity @e[tag=items_probe,nbt={data:{one:5}},distance=..20] run tellraw @s {\"text\":\"ITM_BLOCK_ONE_SLOT\"}"
# A chest has twenty-seven slots. `container.30` names a slot it does not have,
# and a slot that does not exist is skipped rather than counted.
CMDS="$CMDS;;execute unless items block 0 150 3 container.30 minecraft:diamond run tellraw @s {\"text\":\"ITM_BLOCK_PAST_END\"}"

# --- a block that is not a container is an error, not a zero -------------
CMDS="$CMDS;;execute unless items block 0 149 5 container.* minecraft:diamond run tellraw @s {\"text\":\"ITM_STONE_ANSWERED\"}"

# --- reading a slot rolls a loot table that is still packed --------------
CMDS="$CMDS;;setblock 0 150 7 minecraft:chest{LootTable:\"$TABLE\",LootTableSeed:1234L}"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute if block 0 150 7 minecraft:chest{LootTable:\"$TABLE\"} run tellraw @s {\"text\":\"ITM_LOOT_PACKED\"}"
CMDS="$CMDS;;execute if items block 0 150 7 container.0 minecraft:bone"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute unless block 0 150 7 minecraft:chest{LootTable:\"$TABLE\"} run tellraw @s {\"text\":\"ITM_LOOT_ROLLED\"}"

CMDS="$CMDS;;kill @e[tag=items_probe,distance=..20]"

export JOIN_COMMANDS="$CMDS"
python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what happened ==="
grep -E "server says" join.log
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic" | tail -5

fail() { echo "########## ITEMS TEST FAILED ($1) ##########"; exit 1; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }

# Grepping the whole log would also match the echo of the command that asks
# the question, which is printed whether the condition held or not.
said() { grep -q "server says: $1" join.log; }

said ITM_PROBE || fail "the marker the counts are written into was never summoned"
said ITM_PRE_STONE && fail "the player was already holding stone before the give"

said ITM_ENTITY_COUNT || fail "if items entity did not count seventeen stone"
said ITM_ENTITY_COUNTED_SLOTS && fail "if items entity counted slots instead of items"
said ITM_ENTITY_UNLESS || fail "unless items entity did not hold for an item nobody has"

said ITM_ARMOR || fail "if items entity never reached a mob's equipment slot"
said ITM_BARE_WAS_ARMORED && fail "a zombie with no helmet answered for one"

said ITM_CART || fail "if items entity never reached a chest minecart's container"

said ITM_BLOCK_COUNT || fail "if items block did not add up two slots of diamonds"
said ITM_BLOCK_COUNTED_SLOTS && fail "if items block counted slots instead of items"
said ITM_BLOCK_ONE_SLOT || fail "if items block over one named slot read the wrong slot"
said ITM_BLOCK_PAST_END || fail "a slot past the end of the chest was not skipped"

said ITM_STONE_ANSWERED && fail "if items block answered zero for a block with no container"

said ITM_LOOT_PACKED || fail "the chest lost its loot table on the way in"
said ITM_LOOT_ROLLED || fail "reading a slot did not roll the chest's packed loot table"

echo "########## ITEMS TEST PASSED ##########"
