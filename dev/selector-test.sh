#!/bin/bash
# Ask entity selectors the questions an admin actually asks, in a live world.
#
# `@e[type=...]` is the option every command block, datapack and admin script
# leans on, and it was reported broken for mobs -- `@e[type=minecraft:pig]`
# matching nothing while `@e[type=minecraft:item]` matched. It does not
# reproduce, and this test is what says so on every build from now on.
#
# The whole thing happens at y=150 on a five-block platform, a long way above
# anything the world generated. That matters: a selector question asked at
# ground level is answered by the livestock the chunk was stocked with, and an
# assertion that passes because a cow wandered past is worth nothing. Every
# probe here is bounded by `distance=`, and every one of them is asked *before*
# the summon as well as after -- the "before" run has to come back empty, which
# is what proves the "after" run is reading the entity this test made.
#
# The two sheep share a position on purpose. The reported second symptom was
# `distance=..N` seeing one of two entities standing in the same block, so the
# count is asserted at exactly two rather than at "some".
#
# The `by` clause is asked in survival, also on purpose. In creative the target
# is invulnerable and `/damage` answers `commands.damage.invulnerable` whatever
# the selector resolved to -- which is the likeliest reading of the original
# "`/damage ... by <mob>` is unusable" report.
#
# Usage: bash dev/selector-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

# Respect $CARGO_TARGET_DIR the way `cargo build` itself already does; see
# the same lines in dev/join-test.sh for what hardcoding it used to cost.
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
BIN="$TARGET_DIR/debug/foton"

PORT=25703
RUN_DIR="$ROOT/run-selector"

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
# A selector probe is a burst of commands, and the anti-spam counter only sheds
# one point per game tick. On a loaded machine an ordinary burst reads as spam
# and the client is kicked mid-test.
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
# Peaceful refuses to hold the mobs this test summons.
CMDS="$CMDS;;difficulty normal"
# One throwaway command first: the very first `setblock` of a run can land
# before the chunk around the player is ready.
CMDS="$CMDS;;time set noon"
CMDS="$CMDS;;setblock 0 149 0 minecraft:stone"
CMDS="$CMDS;;teleport @s 0 150 0"
CMDS="$CMDS;;!wait 2"
# Something to stand on, and room for the mobs to land on.
CMDS="$CMDS;;setblock 0 149 0 minecraft:stone"
CMDS="$CMDS;;setblock 0 149 3 minecraft:stone"
CMDS="$CMDS;;setblock 0 149 7 minecraft:stone"
CMDS="$CMDS;;setblock 0 149 12 minecraft:stone"
CMDS="$CMDS;;!wait 1"

# --- the empty room ------------------------------------------------------
# Every question this test later answers "yes" is asked here first. A "yes"
# now would mean the world put a mob in range and the rest of the run is
# reading the scenery instead of the summons.
CMDS="$CMDS;;execute if entity @e[type=minecraft:pig,distance=..20] run tellraw @s {\"text\":\"SEL_PRE_PIG\"}"
CMDS="$CMDS;;execute if entity @e[type=minecraft:cow,distance=..20] run tellraw @s {\"text\":\"SEL_PRE_COW\"}"
CMDS="$CMDS;;execute as @e[type=minecraft:sheep,distance=..20] run tellraw @a {\"text\":\"SEL_PRE_SHEEP\"}"

# --- what the test puts there --------------------------------------------
CMDS="$CMDS;;summon minecraft:pig 0 150 3"
CMDS="$CMDS;;summon minecraft:cow 0 150 12"
CMDS="$CMDS;;summon minecraft:sheep 0 150 7"
CMDS="$CMDS;;summon minecraft:sheep 0 150 7"
CMDS="$CMDS;;!wait 2"

# --- type= on a mob ------------------------------------------------------
CMDS="$CMDS;;execute if entity @e[type=minecraft:pig,distance=..20] run tellraw @s {\"text\":\"SEL_PIG\"}"
CMDS="$CMDS;;execute if entity @e[type=minecraft:pig,limit=1,distance=..20] run tellraw @s {\"text\":\"SEL_PIG_LIMIT\"}"
# The unqualified namespace has to resolve to the same entity type.
CMDS="$CMDS;;execute if entity @e[type=pig,distance=..20] run tellraw @s {\"text\":\"SEL_PIG_SHORT\"}"

# --- distance= actually slices -------------------------------------------
# The cow stands at twelve blocks. Inside twenty it is found; inside five it
# must not be, or `distance` is not filtering at all.
CMDS="$CMDS;;execute if entity @e[type=minecraft:cow,distance=..20] run tellraw @s {\"text\":\"SEL_COW_FAR\"}"
CMDS="$CMDS;;execute if entity @e[type=minecraft:cow,distance=..5] run tellraw @s {\"text\":\"SEL_COW_NEAR\"}"

# --- two entities sharing one block --------------------------------------
CMDS="$CMDS;;execute as @e[type=minecraft:sheep,distance=..20] run tellraw @a {\"text\":\"SEL_SHEEP\"}"

# --- a mob selector as a single-entity argument --------------------------
# Survival, because a creative target is invulnerable and would answer
# `commands.damage.invulnerable` no matter which entity `by` resolved to.
CMDS="$CMDS;;gamemode survival"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;damage @s 3 minecraft:mob_attack by @e[type=minecraft:pig,limit=1,distance=..20]"
# And the other half of the contract: a `by` selector that matches nothing has
# to fail the command. Damaging the target anyway from an anonymous source is
# worse than doing nothing -- the death message, the knockback direction and
# every damage-source predicate would all be reading a source that was never
# asked for.
CMDS="$CMDS;;damage @s 1 minecraft:mob_attack by @e[type=minecraft:warden,limit=1,distance=..20]"
CMDS="$CMDS;;gamemode creative"

# --- and the selector can take them away again ---------------------------
CMDS="$CMDS;;kill @e[type=minecraft:pig,distance=..20]"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute if entity @e[type=minecraft:pig,distance=..20] run tellraw @s {\"text\":\"SEL_POST_PIG\"}"

export JOIN_COMMANDS="$CMDS"
python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what happened ==="
grep -E "server says" join.log
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic" | tail -5

fail() { echo "########## SELECTOR TEST FAILED ($1) ##########"; exit 1; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }

# Grepping the whole log would also match the echo of the command that asks
# the question, which is printed whether the condition held or not.
said() { grep -q "server says: $1" join.log; }
said_times() { grep -c "server says: $1" join.log; }

# The room has to be empty first, or nothing below means anything.
said SEL_PRE_PIG && fail "a pig was already within twenty blocks before the summon"
said SEL_PRE_COW && fail "a cow was already within twenty blocks before the summon"
said SEL_PRE_SHEEP && fail "a sheep was already within twenty blocks before the summon"

said SEL_PIG || fail "@e[type=minecraft:pig] did not match a summoned pig"
said SEL_PIG_LIMIT || fail "@e[type=minecraft:pig,limit=1] did not match a summoned pig"
said SEL_PIG_SHORT || fail "@e[type=pig] did not match a summoned pig"

said SEL_COW_FAR || fail "@e[type=minecraft:cow,distance=..20] missed a cow twelve blocks away"
said SEL_COW_NEAR && fail "@e[type=minecraft:cow,distance=..5] matched a cow twelve blocks away"

sheep=$(said_times SEL_SHEEP)
[ "$sheep" -eq 2 ] || fail "distance= saw $sheep of the two sheep sharing one block"

grep -q "translate commands.damage.success" join.log \
  || fail "/damage ... by a mob selector did not land"
grep -q "translate argument.entity.notfound.entity" join.log \
  || fail "/damage ... by a selector matching nothing damaged the target anyway"

said SEL_POST_PIG && fail "the pig survived kill @e[type=minecraft:pig]"

echo "########## SELECTOR TEST PASSED ##########"
