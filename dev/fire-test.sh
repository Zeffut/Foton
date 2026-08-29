#!/bin/bash
# Let a fire live its life: age out, eat its neighbour, and burn forever on
# netherrack.
#
# `FireBlock` had neither `tick` nor `random_tick`, so a lit fire simply stood
# there: it never aged, never went out, never lit anything. None of that is
# visible to a unit test, because everything about fire happens on a scheduled
# block tick -- the one path a unit test cannot reach without a running chunk.
# So this lights three fires and asks the world what became of them.
#
# `/tick sprint` is what makes it a test rather than a wait: fire ticks every
# 30-40 game ticks and ages by an average of a third per tick, so the answers
# below need a couple of thousand ticks and nobody wants to sit through those in
# real time.
#
# The player has to be in creative, not spectator: fire only ticks within
# `fire_spread_radius_around_player` of a *non-spectator* player, so watching
# from spectator would freeze exactly what is being measured.
#
# Usage: bash dev/fire-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25577
RUN_DIR="$ROOT/run-fire"

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
CMDS="$CMDS;;time set day"
# Rain puts fire out, and the seed is not ours to choose here.
CMDS="$CMDS;;weather clear"
# Four blocks up: close enough to keep the fires ticking, far enough not to
# stand in them.
CMDS="$CMDS;;teleport @s 0 104 0"

# Rig one: fire on netherrack, with a plank beside it to eat.
CMDS="$CMDS;;setblock 0 99 0 minecraft:netherrack"
CMDS="$CMDS;;setblock 1 99 0 minecraft:netherrack"
CMDS="$CMDS;;setblock 1 100 0 minecraft:oak_planks"
# Asked before anything is lit: once the fire is up the plank has a couple of
# seconds to live, and a control that races the thing it controls for is no
# control at all.
CMDS="$CMDS;;execute if block 1 100 0 minecraft:oak_planks run tellraw @s \"PLANKSTANDING\""
CMDS="$CMDS;;setblock 0 100 0 minecraft:fire"
# Rig two: fire on plain stone with nothing to burn. Vanilla lets it age to 4
# and then takes it away.
CMDS="$CMDS;;setblock 6 99 0 minecraft:stone"
CMDS="$CMDS;;setblock 6 100 0 minecraft:fire"

# The controls: both fires have to be alight, or nothing below means anything.
CMDS="$CMDS;;execute if block 0 100 0 minecraft:fire run tellraw @s \"NETHERFIRELIT\""
CMDS="$CMDS;;execute if block 6 100 0 minecraft:fire run tellraw @s \"STONEFIRELIT\""

CMDS="$CMDS;;tick sprint 3000"
CMDS="$CMDS;;!wait 5"

# Netherrack is in the overworld's infiniburn tag, so this one neither ages out
# nor drowns in the rain.
CMDS="$CMDS;;execute if block 0 100 0 minecraft:fire run tellraw @s \"NETHERFIRESTILLBURNS\""
# Nothing to burn and a sturdy floor: it ages past 3 and goes.
CMDS="$CMDS;;execute if block 6 100 0 minecraft:air run tellraw @s \"STONEFIREWENTOUT\""
# `checkBurnOut` leaves either a new fire or nothing at all where the plank was.
CMDS="$CMDS;;execute if block 1 100 0 minecraft:fire run tellraw @s \"PLANKCAUGHT\""
CMDS="$CMDS;;execute if block 1 100 0 minecraft:air run tellraw @s \"PLANKBURNEDAWAY\""

# Second half: the radius gamerule is the off switch 26.2 folded `doFireTick`
# into, and zero means no fire ticks anywhere. A fire that would otherwise have
# aged out above must now stand still forever.
CMDS="$CMDS;;gamerule fire_spread_radius_around_player 0"
CMDS="$CMDS;;setblock 12 99 0 minecraft:stone"
CMDS="$CMDS;;setblock 12 100 0 minecraft:fire"
CMDS="$CMDS;;execute if block 12 100 0 minecraft:fire run tellraw @s \"FROZENFIRELIT\""
CMDS="$CMDS;;tick sprint 3000"
CMDS="$CMDS;;!wait 5"
CMDS="$CMDS;;execute if block 12 100 0 minecraft:fire run tellraw @s \"FROZENFIRESTAYED\""

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=3 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what burned ==="
grep "server says" join.log | grep -oE "NETHERFIRELIT|STONEFIRELIT|PLANKSTANDING|NETHERFIRESTILLBURNS|STONEFIREWENTOUT|PLANKCAUGHT|PLANKBURNEDAWAY|FROZENFIRELIT|FROZENFIRESTAYED"
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "\[Error\]|panic" | tail -5

# Only the server's own reply counts: join.py echoes the commands it sends.
said() { grep "server says" join.log | grep -q "$1"; }
fail() { echo "########## FIRE TEST FAILED ($1) ##########"; exit 1; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }

said NETHERFIRELIT  || fail "no fire on the netherrack to begin with"
said STONEFIRELIT   || fail "no fire on the stone to begin with"
said PLANKSTANDING  || fail "the plank never got placed"

said NETHERFIRESTILLBURNS || fail "fire on netherrack went out; infiniburn is not read"
said STONEFIREWENTOUT     || fail "fire on bare stone never aged out; it is not ticking"
if ! said PLANKCAUGHT && ! said PLANKBURNEDAWAY; then
  fail "the plank beside the fire is untouched; nothing burns"
fi

said FROZENFIRELIT   || fail "the third fire never got placed"
said FROZENFIRESTAYED || fail "fire ticked with fire_spread_radius_around_player at 0"
echo "########## FIRE TEST PASSED ##########"
