#!/bin/bash
# Earn advancements with a real client on the wire and read the tree it is sent.
#
# An advancement is protocol-only: nothing inside the server can tell a criterion
# that was awarded from one that reached the client, and nothing but
# `ClientboundUpdateAdvancementsPacket` says which advancements the screen is
# allowed to draw. So this test reads that packet off the wire rather than
# asking the server what it thinks.
#
# It leans on three separate things being right at once:
#   * the criterion fires at all, with its predicate discriminating -- oak
#     planks are not in `#minecraft:stone_tool_materials`, cobblestone is, and
#     the completion has to land after the cobblestone and not after the planks;
#   * the visibility rule reveals two levels below a finished advancement, so
#     finishing `story/mine_stone` has to make `story/upgrade_tools` appear
#     without finishing it;
#   * the tree layout put the icons somewhere other than the origin.
#
# Usage: bash dev/advancement-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25631
RUN_DIR="$ROOT/run-advancement"

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
# The anti-spam counter drains one point per game tick, and this test sends a
# long burst of commands back to back.
sed -i 's/^command_spam_threshold_seconds = .*/command_spam_threshold_seconds = 0/' \
  "$RUN_DIR/config/config.toml"

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
CMDS="$CMDS;;clear @s"
# A marker before anything is earned, so the log can be read in order.
CMDS="$CMDS;;tellraw @s {\"text\":\"ADVMARKERONE\"}"
# The story root asks for a crafting table and nothing else.
CMDS="$CMDS;;give @s minecraft:crafting_table"
CMDS="$CMDS;;tellraw @s {\"text\":\"ADVMARKERTWO\"}"
# Oak planks are in `#minecraft:planks`, NOT in `#minecraft:stone_tool_materials`.
# A predicate that waved everything through would finish `story/mine_stone` here.
CMDS="$CMDS;;give @s minecraft:oak_planks"
CMDS="$CMDS;;tellraw @s {\"text\":\"ADVMARKERTHREE\"}"
# Cobblestone is in that tag, so this is the give that must finish it.
CMDS="$CMDS;;give @s minecraft:cobblestone"
CMDS="$CMDS;;tellraw @s {\"text\":\"ADVMARKERFOUR\"}"
# And the screen has to accept a tab the client opens.
CMDS="$CMDS;;!seentab story/root"
CMDS="$CMDS;;!seenclose"

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=3 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what happened ==="
grep -E "advancement|advancements|server says" join.log | tail -40
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic|incorrect|unknown" | tail -6

fail() { echo "########## ADVANCEMENT TEST FAILED ($1) ##########"; exit 1; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }

line_of() { grep -n -- "$1" join.log | head -1 | cut -d: -f1; }

MARKER_TWO=$(line_of "server says: ADVMARKERTWO")
MARKER_THREE=$(line_of "server says: ADVMARKERTHREE")
MARKER_FOUR=$(line_of "server says: ADVMARKERFOUR")
[ -n "$MARKER_TWO" ] && [ -n "$MARKER_THREE" ] && [ -n "$MARKER_FOUR" ] \
  || fail "the ordering markers never came back, so nothing below can be trusted"

# 1. The crafting table finishes the story root, and that reaches the client.
ROOT_DONE=$(line_of "advancement minecraft:story/root is complete")
[ -n "$ROOT_DONE" ] || fail "a crafting table did not finish story/root"
[ "$ROOT_DONE" -gt "$MARKER_TWO" ] || fail "story/root finished before the crafting table existed"
[ "$ROOT_DONE" -lt "$MARKER_THREE" ] || fail "story/root did not finish on the crafting table"

# 2. The root is drawn, and somewhere other than the origin every icon would
#    pile onto if the tree layout had been skipped.
grep -qE "advancement minecraft:story/root is drawn at .* background=minecraft:gui/advancements/backgrounds/stone" join.log \
  || fail "story/root arrived without its tab background"

# 3. The tag predicate discriminates: planks are not stone tool materials.
STONE_DONE=$(line_of "advancement minecraft:story/mine_stone is complete")
[ -n "$STONE_DONE" ] || fail "cobblestone did not finish story/mine_stone"
[ "$STONE_DONE" -gt "$MARKER_THREE" ] \
  || fail "story/mine_stone finished on oak planks, so the item predicate is not discriminating"
[ "$STONE_DONE" -lt "$MARKER_FOUR" ] || fail "story/mine_stone did not finish on the cobblestone"

# 4. Visibility reaches two levels below what was finished, and no further.
grep -q "advancement minecraft:story/upgrade_tools is drawn at" join.log \
  || fail "finishing story/mine_stone did not reveal its child story/upgrade_tools"
grep -q "advancement minecraft:story/smelt_iron is drawn at" join.log \
  && fail "story/smelt_iron is three levels down and must stay hidden"

# 5. Nothing the player never did may be handed out.
grep -q "advancement minecraft:story/smelt_iron is complete" join.log \
  && fail "an advancement nobody earned was granted"
grep -q "advancement minecraft:husbandry/root is complete" join.log \
  && fail "an advancement nobody earned was granted"

# 6. The chat announcement is the server's job, and mine_stone is announced
#    where the roots deliberately are not.
grep -q "server says:.*chat.type.advancement.task" join.log \
  || fail "finishing story/mine_stone was never announced in chat"
grep -q "server says:.*advancements.story.mine_stone.title" join.log \
  || fail "the announcement did not name the advancement"

# 7. The screen's tab selection round-trips.
grep -q "advancements tab selected minecraft:story/root" join.log \
  || fail "opening the story tab was not accepted"

# 8. Progress that is not finished still reaches the client as progress: the
#    revealed child has to arrive with its criteria unmet rather than missing.
grep -q "advancement minecraft:story/upgrade_tools criterion .* not met" join.log \
  || fail "the revealed child arrived without its unmet criteria"

echo "########## ADVANCEMENT TEST PASSED ##########"
