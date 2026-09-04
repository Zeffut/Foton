#!/bin/bash
# Cook on a campfire, and prove the fire is what does it.
#
# Three campfires, each answering a different question:
#
#   A (0 100 0), lit    -- a player clicks it with a porkchop in hand. Nothing
#                          but a real client can do that: no command reaches
#                          `use_item_on`. Then the block is broken and has to
#                          give the food back. That break happens straight away
#                          on purpose: a porkchop takes 600 ticks and this
#                          script's own round trips outlast that, so leaving it
#                          for the end read a campfire that had really finished.
#   B (4 100 0), lit    -- seeded five ticks from done. If the lit ticker runs,
#                          a cooked beef is on the ground a second later. This
#                          doubles as the proof that `setblock`'s NBT reaches
#                          `load_additional`: without it B would start from
#                          zero and need 600 ticks, not five.
#   C (8 100 0), unlit  -- seeded twenty ticks in. An unlit campfire walks its
#                          progress back down two a tick, so two seconds later
#                          it must read zero and the chicken must still be
#                          sitting on it, uncooked. Reading zero is only
#                          meaningful because B proved the seeding lands.
#   D (12 100 0)        -- cold with a salmon on it, then lit. The ticker is
#                          picked from the block state, so the match has to
#                          swap it: raw before, cooked after.
#
# The player stays in survival: in creative the porkchop would stay in hand and
# "it left the hand" would prove nothing.
#
# Usage: bash dev/campfire-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

# Respect $CARGO_TARGET_DIR the way `cargo build` itself already does; see
# the same lines in dev/join-test.sh for what hardcoding it used to cost.
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
BIN="$TARGET_DIR/debug/foton"

PORT=25617
RUN_DIR="$ROOT/run-campfire"

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
# A burst of commands outruns the anti-spam counter, which only sheds one point
# a game tick; without this the client is kicked for `disconnect.spam`.
sed -i 's/^command_spam_threshold_seconds = .*/command_spam_threshold_seconds = 0/' \
  "$RUN_DIR/config/config.toml"
grep -q '^command_spam_threshold_seconds' "$RUN_DIR/config/config.toml" ||
  echo 'command_spam_threshold_seconds = 0' >> "$RUN_DIR/config/config.toml"

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

CMDS='gamemode survival'
CMDS="$CMDS;;teleport @s 0 100 -2"
CMDS="$CMDS;;setblock 0 99 0 minecraft:stone"
CMDS="$CMDS;;setblock 4 99 0 minecraft:stone"
CMDS="$CMDS;;setblock 8 99 0 minecraft:stone"

# --- A: a player puts food on by hand ---
CMDS="$CMDS;;setblock 0 100 0 minecraft:campfire[lit=true]"
CMDS="$CMDS;;execute if block 0 100 0 minecraft:campfire[lit=true] run tellraw @s \"CAMPFIRELIT\""
CMDS="$CMDS;;give @s minecraft:porkchop 1"
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;execute if entity @s[nbt={Inventory:[{id:\"minecraft:porkchop\"}]}] run tellraw @s \"PORKINHAND\""
CMDS="$CMDS;;!useon 0 100 0 up"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute if block 0 100 0 minecraft:campfire{Items:[{id:\"minecraft:porkchop\"}]} run tellraw @s \"CAMPFIRETOOKPORK\""
CMDS="$CMDS;;execute unless entity @s[nbt={Inventory:[{id:\"minecraft:porkchop\"}]}] run tellraw @s \"PORKLEFTHAND\""
CMDS="$CMDS;;execute if block 0 100 0 minecraft:campfire{CookingTotalTimes:[I;600,0,0,0]} run tellraw @s \"PORKTIMER600\""
# Out of arm's reach from here on. Everything below counts loose items, and a
# player standing next to a campfire picks them up -- which made the porkchop
# assertion fire or not depending on which way the drop happened to scatter.
CMDS="$CMDS;;teleport @s 0 100 -20"
# Break it now, not at the end. A porkchop needs 600 ticks and this script's own
# command round trips take longer than that, so a check further down was reading
# a campfire that had genuinely finished cooking and dropped its food already.
CMDS="$CMDS;;setblock 0 100 0 minecraft:air destroy"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute if entity @e[type=minecraft:item,nbt={Item:{id:\"minecraft:porkchop\"}}] run tellraw @s \"CAMPFIREGAVEBACKITSFOOD\""

# --- B and C: seeded timers, one fire lit and one out ---
CMDS="$CMDS;;setblock 4 100 0 minecraft:campfire[lit=true]{Items:[{id:\"minecraft:beef\",count:1,Slot:0b}],CookingTimes:[I;595,0,0,0],CookingTotalTimes:[I;600,0,0,0]}"
CMDS="$CMDS;;setblock 8 100 0 minecraft:campfire[lit=false]{Items:[{id:\"minecraft:chicken\",count:1,Slot:0b}],CookingTimes:[I;20,0,0,0],CookingTotalTimes:[I;600,0,0,0]}"
CMDS="$CMDS;;execute if block 8 100 0 minecraft:campfire{Items:[{id:\"minecraft:chicken\"}]} run tellraw @s \"CHICKENONTHECOLDFIRE\""
CMDS="$CMDS;;!wait 3"
CMDS="$CMDS;;execute if entity @e[type=minecraft:item,nbt={Item:{id:\"minecraft:cooked_beef\"}}] run tellraw @s \"COOKEDBEEFDROPPED\""
CMDS="$CMDS;;execute if block 8 100 0 minecraft:campfire{Items:[{id:\"minecraft:chicken\"}]} run tellraw @s \"CAMPFIREKEPTCHICKEN\""
CMDS="$CMDS;;execute if block 8 100 0 minecraft:campfire{CookingTimes:[I;0,0,0,0]} run tellraw @s \"CAMPFIRECOOLED\""

# --- D: lighting a cold campfire has to swap its ticker ---
# The ticker is chosen from the block state, so relighting must re-pick it. A
# total of 20 rather than a recipe's 600 makes the wait short *and* the timing
# irrelevant: an unlit campfire's progress is already clamped at zero, so
# however many ticks pass before the match, lighting it is always 20 ticks from
# a cooked salmon.
CMDS="$CMDS;;setblock 12 99 0 minecraft:stone"
CMDS="$CMDS;;setblock 12 100 0 minecraft:campfire[lit=false]{Items:[{id:\"minecraft:salmon\",count:1,Slot:0b}],CookingTimes:[I;0,0,0,0],CookingTotalTimes:[I;20,0,0,0]}"
CMDS="$CMDS;;!wait 2"
CMDS="$CMDS;;execute if block 12 100 0 minecraft:campfire{Items:[{id:\"minecraft:salmon\"}]} run tellraw @s \"COLDFIREKEPTSALMONRAW\""
CMDS="$CMDS;;setblock 12 100 0 minecraft:campfire[lit=true]"
CMDS="$CMDS;;!wait 2"
CMDS="$CMDS;;execute if entity @e[type=minecraft:item,nbt={Item:{id:\"minecraft:cooked_salmon\"}}] run tellraw @s \"RELIGHTINGRESUMEDCOOKING\""

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=2 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what the server answered ==="
grep "server says" join.log | grep -vE "setblock|teleport|gamemode|give" | tail -14
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic|incorrect|unknown" | tail -6

if [ $STATUS -ne 0 ]; then
  echo "########## CAMPFIRE TEST FAILED (the client never settled) ##########"
  tail -20 join.log
  exit 1
fi
for marker in CAMPFIRELIT PORKINHAND CAMPFIRETOOKPORK PORKLEFTHAND PORKTIMER600 \
              CAMPFIREGAVEBACKITSFOOD CHICKENONTHECOLDFIRE COOKEDBEEFDROPPED \
              CAMPFIREKEPTCHICKEN CAMPFIRECOOLED COLDFIREKEPTSALMONRAW \
              RELIGHTINGRESUMEDCOOKING; do
  # Only the server's own reply counts: join.py echoes the commands it sends.
  if ! grep "server says" join.log | grep -q "$marker"; then
    echo "########## CAMPFIRE TEST FAILED ($marker missing) ##########"
    exit 1
  fi
done
echo "########## CAMPFIRE TEST PASSED ##########"
