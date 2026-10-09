#!/bin/bash
# Run /item and /recipe against a live server, from the console line and from a
# datapack function, which is where they were refused at load.
#
# The datapack carries the line that started this off -- `item replace entity @s
# weapon.mainhand with air` -- and a `recipe give` behind `execute ... run`, so
# a function that fails to compile takes the markers with it. An item modifier
# file rides along to prove `item_modifier/*.json` is read off disk.
#
# Usage: bash dev/item-recipe-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

# Respect $CARGO_TARGET_DIR the way `cargo build` itself already does; see
# the same lines in dev/join-test.sh for what hardcoding it used to cost.
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
BIN="$TARGET_DIR/debug/foton"

PORT=25721
RUN_DIR="$ROOT/run-item-recipe"

echo "=== Building ==="
cargo build 2>&1 | tail -2
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

PACK="$RUN_DIR/saves/datapacks/foton_item/data"
mkdir -p "$PACK/t/function" "$PACK/t/item_modifier" || exit 1
cat > "$PACK/../pack.mcmeta" <<'PACK_EOF'
{"pack": {"description": "foton item test", "pack_format": 96}}
PACK_EOF

# The exact line from the bug report, and the recipe line behind `execute`.
cat > "$PACK/t/function/empty_hand.mcfunction" <<'FN_EOF'
item replace entity @s weapon.mainhand with air
FN_EOF
cat > "$PACK/t/function/learn.mcfunction" <<'FN_EOF'
execute if entity @s run recipe give @s minecraft:stick
FN_EOF
# Names a datapack modifier that is only installed once the pack has loaded.
cat > "$PACK/t/function/gild.mcfunction" <<'FN_EOF'
item modify block 0 149 1 container.3 t:gold
FN_EOF
cat > "$PACK/t/item_modifier/gold.json" <<'JSON_EOF'
{"function": "minecraft:set_item", "item": "minecraft:gold_ingot"}
JSON_EOF

cd "$RUN_DIR" || exit 1
nohup "$BIN" > server.log 2>&1 < /dev/null &
PID=$!
cleanup() {
  kill "$PID" 2>/dev/null
  for _ in $(seq 1 30); do kill -0 "$PID" 2>/dev/null || break; sleep 1; done
  kill -9 "$PID" 2>/dev/null
}

for _ in $(seq 1 180); do
  python3 "$ROOT/dev/wait-tcp.py" 127.0.0.1 "$PORT" 1 "$PID" >/dev/null 2>&1 && break
  sleep 1
done
if ! python3 "$ROOT/dev/wait-tcp.py" 127.0.0.1 "$PORT" 1 "$PID" >/dev/null 2>&1; then
  echo "SERVER NEVER LISTENED ON $PORT"
  sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | tail -20
  cleanup; exit 1
fi

CMDS='gamemode creative'
CMDS="$CMDS;;teleport @s 0 150 0"
CMDS="$CMDS;;setblock 0 148 0 minecraft:stone"
CMDS="$CMDS;;!wait 2"
CMDS="$CMDS;;setblock 0 149 1 minecraft:chest"
CMDS="$CMDS;;!wait 1"

# --- entity targets -------------------------------------------------------
CMDS="$CMDS;;item replace entity @s weapon.mainhand with minecraft:diamond_sword"
CMDS="$CMDS;;execute if items entity @s weapon.mainhand minecraft:diamond_sword run tellraw @s \"MARK_HAND_SWORD\""
CMDS="$CMDS;;function t:empty_hand"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute if items entity @s weapon.mainhand minecraft:diamond_sword run tellraw @s \"MARK_HAND_STILL_SWORD\""
CMDS="$CMDS;;item replace entity @s armor.chest with minecraft:stone"
CMDS="$CMDS;;item replace entity @s armor.chest with minecraft:diamond_chestplate"
CMDS="$CMDS;;execute if items entity @s armor.chest minecraft:diamond_chestplate run tellraw @s \"MARK_CHESTPLATE\""
CMDS="$CMDS;;item replace entity @s weapon.* with minecraft:stone"
CMDS="$CMDS;;item replace entity @s container.0 with minecraft:stone 65"
CMDS="$CMDS;;item replace entity @s container.0 with minecraft:stone 64"
CMDS="$CMDS;;item replace entity @s hotbar.1 from entity @s container.0"
CMDS="$CMDS;;execute if items entity @s hotbar.1 minecraft:stone run tellraw @s \"MARK_COPY_ENTITY\""

# --- block targets --------------------------------------------------------
CMDS="$CMDS;;item replace block 0 149 1 container.3 with minecraft:apple 5"
CMDS="$CMDS;;execute if items block 0 149 1 container.3 minecraft:apple run tellraw @s \"MARK_BLOCK_APPLE\""
CMDS="$CMDS;;item replace block 0 149 1 container.40 with minecraft:apple"
CMDS="$CMDS;;item replace block 0 148 0 container.0 with minecraft:apple"
CMDS="$CMDS;;item replace block 0 149 1 container.4 from block 0 149 1 container.3"
CMDS="$CMDS;;execute if items block 0 149 1 container.4 minecraft:apple run tellraw @s \"MARK_BLOCK_COPY\""
CMDS="$CMDS;;item replace block 0 149 1 container.5 from block 0 149 1 container.3 {function:\"minecraft:set_item\",item:\"minecraft:bread\"}"
CMDS="$CMDS;;execute if items block 0 149 1 container.5 minecraft:bread run tellraw @s \"MARK_BLOCK_COPY_MODIFIED\""

# --- modifiers -------------------------------------------------------------
CMDS="$CMDS;;item modify block 0 149 1 container.3 {function:\"minecraft:set_item\",item:\"minecraft:dirt\"}"
CMDS="$CMDS;;execute if items block 0 149 1 container.3 minecraft:dirt run tellraw @s \"MARK_MODIFY_INLINE\""
CMDS="$CMDS;;function t:gild"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute if items block 0 149 1 container.3 minecraft:gold_ingot run tellraw @s \"MARK_MODIFY_DATAPACK\""
CMDS="$CMDS;;item modify block 0 149 1 container.3 nobody:wrote_this"
CMDS="$CMDS;;item modify entity @s weapon.mainhand {function:\"minecraft:set_lore\",lore:[],mode:\"append\"}"

# --- recipes ---------------------------------------------------------------
CMDS="$CMDS;;recipe give @s minecraft:stick"
CMDS="$CMDS;;recipe give @s minecraft:stick"
CMDS="$CMDS;;recipe take @s minecraft:stick"
CMDS="$CMDS;;recipe take @s minecraft:stick"
CMDS="$CMDS;;function t:learn"
CMDS="$CMDS;;!wait 1"
# A function runs with its output suppressed, so the only way to see that
# t:learn gave the recipe is that there is now one to take.
CMDS="$CMDS;;recipe take @s minecraft:stick"
CMDS="$CMDS;;recipe give @s nobody:wrote_this"
CMDS="$CMDS;;recipe give @s *"
CMDS="$CMDS;;recipe take @s *"
CMDS="$CMDS;;tellraw @s \"MARK_ALIVE\""

export JOIN_COMMANDS="$CMDS"
python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what happened ==="
grep -E "server says" join.log
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "function|datapack|item modifier" | head -10
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "panic" | tail -5

fail() { echo "########## ITEM/RECIPE TEST FAILED ($1) ##########"; exit 1; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }

said() { grep -q "server says: .*$1" join.log; }
said_times() { grep -c "server says: .*$1" join.log; }

sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -qiE "failed to load function t:" \
  && fail "a datapack function using /item or /recipe was refused at load"

said MARK_HAND_SWORD || fail "item replace entity never put the sword in the main hand"
said MARK_HAND_STILL_SWORD && fail "item replace ... with air inside a function left the sword"
said MARK_CHESTPLATE || fail "a chestplate was refused by the chest slot"
said commands.item.target.no_changed.known_item || fail "stone in the chest slot was not refused"
said MARK_COPY_ENTITY || fail "item replace ... from entity copied nothing"
said arguments.item.overstacked || fail "a hundred stone was accepted as one stack"
said slot.only_single_allowed || fail "weapon.* was accepted where one slot is required"

said MARK_BLOCK_APPLE || fail "item replace block put nothing in the chest"
said commands.item.target.no_such_slot || fail "container.40 was accepted by a chest"
said commands.item.target.not_a_container || fail "a stone block was accepted as a container"
said MARK_BLOCK_COPY || fail "item replace block ... from block copied nothing"
said MARK_BLOCK_COPY_MODIFIED || fail "an inline modifier on a copy was not applied"

said MARK_MODIFY_INLINE || fail "item modify with an inline modifier changed nothing"
said MARK_MODIFY_DATAPACK || fail "item modify with a datapack item_modifier changed nothing"
said argument.resource_or_id.no_such_element || fail "an unknown modifier id was accepted"
said argument.resource_or_id.failed_to_parse || fail "an unsupported function was silently accepted"

[ "$(said_times commands.recipe.take.success.single)" = "3" ] \
  || fail "recipe take did not follow a give by the command, a give by the function and recipe give *"
[ "$(said_times commands.recipe.give.success.single)" = "2" ] \
  || fail "recipe give did not succeed for stick and for *"
said commands.recipe.give.failed || fail "giving a known recipe again was not refused"
said commands.recipe.take.failed || fail "taking an unknown recipe was not refused"
said recipe.notFound || fail "an unknown recipe was not refused"

said MARK_ALIVE || fail "the server stopped answering"

echo "########## ITEM/RECIPE TEST PASSED ##########"
