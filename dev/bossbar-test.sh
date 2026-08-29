#!/bin/bash
# Make a boss bar out of nothing, drive it, and find it again after a restart.
#
# A named bar has no entity behind it and nothing on the server holds it but
# the domain's own save file, so the whole point of the feature is the second
# boot. The first boot builds the bar and proves the command family answers;
# the second asks the save file for it back.
#
# Reading a bar is done through the command's own return value rather than by
# eye: `execute store result ... run bossbar get <id> value` puts the answer
# into a marker's `data` compound, and a selector asks the marker. That is
# also what makes `execute store ... bossbar` testable in the same breath --
# the value goes in one way and comes out the other.
#
# Usage: bash dev/bossbar-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25715
RUN_DIR="$ROOT/run-bossbar"
BAR="minecraft:foton_test"

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

wait_for_port() {
  for _ in $(seq 1 180); do
    ss -ltn 2>/dev/null | grep -q ":$PORT" && return 0
    sleep 1
  done
  return 1
}

PID=
start_server() {
  nohup "$ROOT/target/debug/foton" > "server-$1.log" 2>&1 < /dev/null &
  PID=$!
  if ! wait_for_port; then
    echo "SERVER NEVER LISTENED ON $PORT ($1)"
    sed 's/\x1b\[[0-9;]*[A-Za-z]//g' "server-$1.log" | tail -20
    kill -9 "$PID" 2>/dev/null
    return 1
  fi
}

# A clean stop, because that is what writes the domain's boss bars out.
stop_server() {
  kill "$PID" 2>/dev/null
  for _ in $(seq 1 60); do kill -0 "$PID" 2>/dev/null || break; sleep 1; done
  kill -0 "$PID" 2>/dev/null && kill -9 "$PID" 2>/dev/null
  sleep 2
}

# The marker every answer is written into.
SETUP='gamemode creative'
SETUP="$SETUP;;time set noon"
for z in 0 3 5 7; do
  SETUP="$SETUP;;setblock 0 149 $z minecraft:stone"
done
SETUP="$SETUP;;teleport @s 0 150 0"
SETUP="$SETUP;;!wait 2"
SETUP="$SETUP;;summon minecraft:armor_stand 0 150 0 {Tags:[\"bb_probe\"],NoGravity:1b,Invulnerable:1b}"
SETUP="$SETUP;;!wait 1"
SETUP="$SETUP;;execute if entity @e[tag=bb_probe,distance=..20] run tellraw @s {\"text\":\"BB_PROBE\"}"

# --------------------------------------------------------------- first boot
echo "=== First boot: builds the bar and drives it ==="
start_server first || exit 1

CMDS="$SETUP"
# Nothing before the bar exists.
CMDS="$CMDS;;execute store success entity @n[tag=bb_probe,distance=..20] data.pre byte 1 run bossbar get $BAR value"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute if entity @e[tag=bb_probe,nbt={data:{pre:0b}},distance=..20] run tellraw @s {\"text\":\"BB_PRE_UNKNOWN\"}"

CMDS="$CMDS;;bossbar add $BAR {\"text\":\"Foton Test\"}"
CMDS="$CMDS;;!wait 1"
# A second add of the same id must fail, or a datapack would silently lose the
# bar it already had.
CMDS="$CMDS;;execute store success entity @n[tag=bb_probe,distance=..20] data.dup byte 1 run bossbar add $BAR {\"text\":\"Other\"}"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute if entity @e[tag=bb_probe,nbt={data:{dup:0b}},distance=..20] run tellraw @s {\"text\":\"BB_NO_DUPLICATE\"}"

CMDS="$CMDS;;bossbar set $BAR max 40"
CMDS="$CMDS;;bossbar set $BAR value 10"
CMDS="$CMDS;;bossbar set $BAR color purple"
CMDS="$CMDS;;bossbar set $BAR style notched_12"
CMDS="$CMDS;;bossbar set $BAR players @s"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute store result entity @n[tag=bb_probe,distance=..20] data.value int 1 run bossbar get $BAR value"
CMDS="$CMDS;;execute store result entity @n[tag=bb_probe,distance=..20] data.max int 1 run bossbar get $BAR max"
CMDS="$CMDS;;execute store result entity @n[tag=bb_probe,distance=..20] data.people int 1 run bossbar get $BAR players"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute if entity @e[tag=bb_probe,nbt={data:{value:10}},distance=..20] run tellraw @s {\"text\":\"BB_VALUE\"}"
CMDS="$CMDS;;execute if entity @e[tag=bb_probe,nbt={data:{max:40}},distance=..20] run tellraw @s {\"text\":\"BB_MAX\"}"
CMDS="$CMDS;;execute if entity @e[tag=bb_probe,nbt={data:{people:1}},distance=..20] run tellraw @s {\"text\":\"BB_PLAYERS\"}"

# Setting a value it already has has to fail, or a datapack loop cannot tell
# that it made no progress.
CMDS="$CMDS;;execute store success entity @n[tag=bb_probe,distance=..20] data.again byte 1 run bossbar set $BAR value 10"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute if entity @e[tag=bb_probe,nbt={data:{again:0b}},distance=..20] run tellraw @s {\"text\":\"BB_UNCHANGED\"}"

# `execute store ... bossbar`: three chickens counted into the bar's value.
# `NoGravity` and a floor both: a chicken that falls drops out of the
# selector's twenty-block reach before the count runs, and a count of two
# instead of three looks exactly like a broken store.
CMDS="$CMDS;;summon minecraft:chicken 0 150 3 {Tags:[\"bb_count\"],NoGravity:1b}"
CMDS="$CMDS;;summon minecraft:chicken 0 150 5 {Tags:[\"bb_count\"],NoGravity:1b}"
CMDS="$CMDS;;summon minecraft:chicken 0 150 7 {Tags:[\"bb_count\"],NoGravity:1b}"
CMDS="$CMDS;;!wait 1"
# And the count itself is asked for before it is used, so a rig that lost a
# chicken says so instead of blaming the store.
CMDS="$CMDS;;execute store result entity @n[tag=bb_probe,distance=..20] data.chickens int 1 run execute if entity @e[tag=bb_count,distance=..20]"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute if entity @e[tag=bb_probe,nbt={data:{chickens:3}},distance=..20] run tellraw @s {\"text\":\"BB_COUNT_READY\"}"
CMDS="$CMDS;;execute store result bossbar $BAR value run execute if entity @e[tag=bb_count,distance=..20]"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute store result entity @n[tag=bb_probe,distance=..20] data.stored int 1 run bossbar get $BAR value"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute if entity @e[tag=bb_probe,nbt={data:{stored:3}},distance=..20] run tellraw @s {\"text\":\"BB_STORE_RESULT\"}"
# The control: a store that never reached the bar leaves the ten the command
# before it set, which would otherwise look the same as a wrong count.
CMDS="$CMDS;;execute if entity @e[tag=bb_probe,nbt={data:{stored:10}},distance=..20] run tellraw @s {\"text\":\"BB_STORE_DID_NOTHING\"}"

# A store aimed at a bar nobody made must fail rather than invent one, and the
# proof is the command it was chained to never running: if the clause were
# accepted, the bar below would be twenty-five instead of the three the last
# store left it at.
CMDS="$CMDS;;execute store result bossbar minecraft:no_such_bar value run bossbar set $BAR value 25"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute store result entity @n[tag=bb_probe,distance=..20] data.ghost int 1 run bossbar get $BAR value"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute if entity @e[tag=bb_probe,nbt={data:{ghost:3}},distance=..20] run tellraw @s {\"text\":\"BB_GHOST_REFUSED\"}"
CMDS="$CMDS;;execute if entity @e[tag=bb_probe,nbt={data:{ghost:25}},distance=..20] run tellraw @s {\"text\":\"BB_GHOST_RAN\"}"

CMDS="$CMDS;;kill @e[tag=bb_count,distance=..20]"
CMDS="$CMDS;;kill @e[tag=bb_probe,distance=..20]"

JOIN_COMMANDS="$CMDS" python3 "$ROOT/dev/join.py" "$PORT" > join-first.log 2>&1
FIRST_STATUS=$?
stop_server

# -------------------------------------------------------------- second boot
echo "=== Second boot: asks the save file for the bar back ==="
start_server second || exit 1

CMDS="$SETUP"
CMDS="$CMDS;;execute store result entity @n[tag=bb_probe,distance=..20] data.value int 1 run bossbar get $BAR value"
CMDS="$CMDS;;execute store result entity @n[tag=bb_probe,distance=..20] data.max int 1 run bossbar get $BAR max"
CMDS="$CMDS;;execute store result entity @n[tag=bb_probe,distance=..20] data.bars int 1 run bossbar list"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute if entity @e[tag=bb_probe,nbt={data:{value:3}},distance=..20] run tellraw @s {\"text\":\"BB_KEPT_VALUE\"}"
CMDS="$CMDS;;execute if entity @e[tag=bb_probe,nbt={data:{max:40}},distance=..20] run tellraw @s {\"text\":\"BB_KEPT_MAX\"}"
CMDS="$CMDS;;execute if entity @e[tag=bb_probe,nbt={data:{bars:1}},distance=..20] run tellraw @s {\"text\":\"BB_KEPT_LISTED\"}"
# The color survived too, which is what says the whole record was written and
# not just the two numbers a command wrote last.
CMDS="$CMDS;;execute store success entity @n[tag=bb_probe,distance=..20] data.color byte 1 run bossbar set $BAR color purple"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute if entity @e[tag=bb_probe,nbt={data:{color:0b}},distance=..20] run tellraw @s {\"text\":\"BB_KEPT_COLOR\"}"
# On its own the probe above passes for the wrong reason too: a bar that is
# not there refuses the set as well. Setting a color it is *not* has to
# succeed, which only a bar that exists can do.
CMDS="$CMDS;;execute store success entity @n[tag=bb_probe,distance=..20] data.recolor byte 1 run bossbar set $BAR color white"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute if entity @e[tag=bb_probe,nbt={data:{recolor:1b}},distance=..20] run tellraw @s {\"text\":\"BB_COLOR_ANSWERS\"}"

CMDS="$CMDS;;execute store result entity @n[tag=bb_probe,distance=..20] data.left int 1 run bossbar remove $BAR"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute if entity @e[tag=bb_probe,nbt={data:{left:0}},distance=..20] run tellraw @s {\"text\":\"BB_REMOVED\"}"
CMDS="$CMDS;;execute store success entity @n[tag=bb_probe,distance=..20] data.after byte 1 run bossbar get $BAR value"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute if entity @e[tag=bb_probe,nbt={data:{after:0b}},distance=..20] run tellraw @s {\"text\":\"BB_GONE\"}"

JOIN_COMMANDS="$CMDS" python3 "$ROOT/dev/join.py" "$PORT" > join-second.log 2>&1
SECOND_STATUS=$?
stop_server

echo "=== first boot ==="
grep -oE "server says: BB_[A-Z_]+" join-first.log | sort -u
echo "=== second boot ==="
grep -oE "server says: BB_[A-Z_]+" join-second.log | sort -u
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server-first.log server-second.log \
  | grep -iE "error|panic" | tail -5

fail() { echo "########## BOSSBAR TEST FAILED ($1) ##########"; exit 1; }
said() { grep -q "server says: $2" "join-$1.log"; }

[ $FIRST_STATUS -eq 0 ] || { tail -20 join-first.log; fail "the client never settled on the first boot"; }
[ $SECOND_STATUS -eq 0 ] || { tail -20 join-second.log; fail "the client never settled on the second boot"; }

said first BB_PROBE || fail "the marker the answers are written into was never summoned"
said first BB_PRE_UNKNOWN || fail "a bar nobody made answered before it existed"
said first BB_NO_DUPLICATE || fail "a second bossbar add under the same id was allowed"
said first BB_VALUE || fail "bossbar set value did not reach bossbar get value"
said first BB_MAX || fail "bossbar set max did not reach bossbar get max"
said first BB_PLAYERS || fail "bossbar set players did not put anybody on the bar"
said first BB_UNCHANGED || fail "setting a value the bar already had reported success"
said first BB_COUNT_READY || fail "the three counted chickens were not all in range"
said first BB_STORE_DID_NOTHING && fail "execute store result bossbar left the value where it was"
said first BB_STORE_RESULT || fail "execute store result bossbar never reached the bar"
said first BB_GHOST_RAN && fail "a store clause naming a bar that does not exist still ran its command"
said first BB_GHOST_REFUSED || fail "execute store bossbar accepted a bar that does not exist"

said second BB_PROBE || fail "the second boot's marker was never summoned"
said second BB_KEPT_VALUE || fail "the bar's value did not survive the restart"
said second BB_KEPT_MAX || fail "the bar's max did not survive the restart"
said second BB_KEPT_LISTED || fail "the bar was not in the list after the restart"
said second BB_COLOR_ANSWERS || fail "the restored bar would not take a color at all"
said second BB_KEPT_COLOR || fail "the bar's color did not survive the restart"
said second BB_REMOVED || fail "bossbar remove did not report an empty list afterwards"
said second BB_GONE || fail "a removed bar still answered"

echo "########## BOSSBAR TEST PASSED ##########"
