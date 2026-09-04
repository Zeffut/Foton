#!/bin/bash
# Give a creaking a real server and see whether it is alive in any sense.
#
# `Creaking.tick` called `Entity::default_tick`, which is only vanilla's
# `Entity.baseTick`. The `super.tick()` of vanilla `Creaking.tick` is
# `LivingEntity.tick`, so everything below it was missing: no mob effects, no
# death handling, and no `ai_step` -- which is the only path to
# `server_ai_step`, to `custom_server_ai_step`, and so to `Brain::tick`. The
# creaking stood exactly where it was summoned, forever, and compiled fine.
#
# The unit tests could not see it. `a_creaking_runs_its_brain` calls
# `LivingEntity::server_ai_step` directly and so comes in *under* the break;
# it stayed green the whole time. This comes in the only way the server does:
# `World::tick_entities` -> `Entity::tick`.
#
# The witness is the second rig: a creaking nobody is looking at has to wander.
# `RandomStroll` is in its idle activity and the only road to it is the brain.
# Reverting the fix and running this again is what it was measured against:
# `WANDERERMOVED` appears two to three times out of four samples with the living
# tick and *never* without it -- the creaking stands exactly where it was put
# for four thousand eight hundred ticks.
#
# The first rig is a control on the rig itself, not a second witness. It was
# built expecting a creaking at zero health to linger without `tick_death`, and
# the reverted run disproved that: `DEADRIGREMOVED` fires either way, because a
# lethal blow removes the mob without needing the living tick. It stays because
# summoning, tagging, damaging and clearing a creaking are the machinery the
# second rig depends on, and it is cheap to know they work.
#
# The player faces west while the wandering creaking stands to the east: a
# creaking freezes under a bare-headed gaze, so a player staring at it would
# freeze exactly what is being measured.
#
# Usage: bash dev/creaking-ai-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

# Respect $CARGO_TARGET_DIR the way `cargo build` itself already does; see
# the same lines in dev/join-test.sh for what hardcoding it used to cost.
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
BIN="$TARGET_DIR/debug/foton"

PORT=25891
RUN_DIR="$ROOT/run-creaking"

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

CMDS='gamemode creative'
CMDS="$CMDS;;time set day"
CMDS="$CMDS;;weather clear"
# Nothing else is allowed to wander into the rig. The tagged selectors below
# would ignore a stray creaking anyway, but a pale garden at spawn would still
# muddy the log.
CMDS="$CMDS;;gamerule spawn_mobs false"

# --- the corridor ---------------------------------------------------------
# A hundred blocks up, where whatever the world generated is somebody else's
# problem. Three wide and fourteen long: enough floor for a stroll, few enough
# `setblock`s to keep the run short.
for X in $(seq -1 12); do
  for Z in $(seq -1 1); do
    CMDS="$CMDS;;setblock $X 100 $Z minecraft:stone"
  done
done

# Yaw 90 is west, and both creakings stand east of here.
CMDS="$CMDS;;teleport @s 0 101 0 90 0"
CMDS="$CMDS;;!wait 2"

# --- rig one: a killed creaking has to leave ------------------------------
# `PersistenceRequired` so that nothing below can be explained by a despawn.
CMDS="$CMDS;;summon minecraft:creaking 3 101 0 {Tags:[\"fotondead\"],PersistenceRequired:1b}"
CMDS="$CMDS;;!wait 2"
# The control: asked before the kill, because an absence assertion after one
# means nothing without it.
CMDS="$CMDS;;execute if entity @e[type=minecraft:creaking,tag=fotondead] run tellraw @s \"DEADRIGSUMMONED\""
# A creaking has one heart of health, so any real blow is fatal. `limit=1`
# because `/damage` takes a single-entity argument, and a selector that could
# match several is a parse error rather than a miss.
CMDS="$CMDS;;damage @e[type=minecraft:creaking,tag=fotondead,limit=1] 1000 minecraft:generic"
# The control for the blow itself: a creaking that shrugged the damage off would
# make the removal assertion below fail for the wrong reason.
CMDS="$CMDS;;execute unless entity @e[type=minecraft:creaking,tag=fotondead,nbt={Health:1.0f}] run tellraw @s \"DEADRIGWASHURT\""
# `tickDeath` removes at twenty; two hundred is room to spare.
CMDS="$CMDS;;tick sprint 200"
CMDS="$CMDS;;!wait 4"
CMDS="$CMDS;;execute unless entity @e[type=minecraft:creaking,tag=fotondead] run tellraw @s \"DEADRIGREMOVED\""

# --- rig two: an unwatched creaking has to wander -------------------------
CMDS="$CMDS;;summon minecraft:creaking 8 101 0 {Tags:[\"fotonwander\"],PersistenceRequired:1b}"
CMDS="$CMDS;;!wait 2"
# The control for the selector, not for the position: a wide radius the
# creaking cannot leave, asked at the same moment as the narrow one below. It
# proves `positioned` + `distance` + `tag` resolve to this creaking at all, so
# that the `unless` firing means "further than two blocks" rather than "the
# selector matches nothing". A tight radius here would be a control racing the
# very thing it controls for -- the creaking is already walking by the time the
# question arrives.
# Sampled rather than asked once: a random stroll can be back near where it
# started on any given tick, and one sample of a random walk is a coin toss.
for _ in 1 2 3 4; do
  CMDS="$CMDS;;tick sprint 1200"
  CMDS="$CMDS;;!wait 4"
  CMDS="$CMDS;;execute positioned 8 101 0 if entity @e[type=minecraft:creaking,tag=fotonwander,distance=..48] run tellraw @s \"WANDERERINRANGE\""
  CMDS="$CMDS;;execute positioned 8 101 0 unless entity @e[type=minecraft:creaking,tag=fotonwander,distance=..2] run tellraw @s \"WANDERERMOVED\""
done
# It has to still be there: "moved" must not be satisfied by "gone".
CMDS="$CMDS;;execute if entity @e[type=minecraft:creaking,tag=fotonwander] run tellraw @s \"WANDERERSTILLTHERE\""

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=3 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what the creakings did ==="
grep "server says" join.log | grep -owE "DEADRIGSUMMONED|DEADRIGWASHURT|DEADRIGREMOVED|WANDERERINRANGE|WANDERERMOVED|WANDERERSTILLTHERE"
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "\[Error\]|panic" | tail -5

# Only the server's own reply counts: join.py echoes the commands it sends, so
# a bare grep would find every marker in the outgoing traffic. `-w` because
# `grep -q MARKER` is happy with `MARKERSOMETHINGELSE`.
said() { grep "server says" join.log | grep -qw "$1"; }
fail() { echo "########## CREAKING AI TEST FAILED ($1) ##########"; exit 1; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }

said DEADRIGSUMMONED || fail "no creaking was summoned, so nothing below means anything"
said DEADRIGWASHURT  || fail "the creaking is still at full health, so the blow never landed and the removal below would prove nothing"
said DEADRIGREMOVED  || fail "a killed creaking is still in the world; the summon-damage-clear rig the wander test depends on is broken"

said WANDERERINRANGE    || fail "the distance selector never matched the second creaking, so the assertion below would be vacuous"
said WANDERERSTILLTHERE || fail "the second creaking left the world; it was supposed to wander, not despawn"
said WANDERERMOVED      || fail "the creaking never moved a step in four thousand eight hundred ticks: its tick is not reaching its brain"

echo "########## CREAKING AI TEST PASSED ##########"
