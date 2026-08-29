#!/bin/bash
# Grass has to behave like ground cover: creep onto the dirt beside it, die
# under a lid, and answer bone meal with tufts.
#
# `GrassBlock` and `MyceliumBlock` had `get_state_for_placement` and
# `update_shape` and nothing else, so a lawn never grew and a covered one never
# turned back to dirt. All three of those live on paths a unit test cannot
# reach: two random ticks in a running chunk, and a right-click carrying an
# item. So this asks a running world instead.
#
# `random_tick_speed` goes up for the same reason `dev/sapling-test.sh` raises
# it -- at the default rate this would be a coin flip on the clock rather than
# on the code. The rig is kept small on purpose: a long burst of commands trips
# the anti-spam kick before any of it can be answered.
#
# Usage: bash dev/grass-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25578
RUN_DIR="$ROOT/run-grass"

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
# Grass only spreads where the block above the target is lit to 9, so the sun
# has to be up and the sky clear. Everything sits at y=100, well above the
# terrain, so the air over it is already there.
CMDS="$CMDS;;time set day"
CMDS="$CMDS;;weather clear"
CMDS="$CMDS;;teleport @s 0 103 0"

# One patch of open dirt with a single blade of grass in the middle of it.
for z in -1 0 1; do
  CMDS="$CMDS;;setblock -1 100 $z minecraft:dirt"
  CMDS="$CMDS;;setblock 0 100 $z minecraft:dirt"
  CMDS="$CMDS;;setblock 1 100 $z minecraft:dirt"
done
CMDS="$CMDS;;setblock 0 100 0 minecraft:grass_block"

# A grass block and a mycelium one, both still under open sky, so nothing can
# have happened to them yet when the controls below are asked.
CMDS="$CMDS;;setblock 8 100 0 minecraft:grass_block"
CMDS="$CMDS;;setblock 10 100 0 minecraft:mycelium"

CMDS="$CMDS;;execute if block 1 100 0 minecraft:dirt run tellraw @s \"NEIGHBOURISDIRT\""
CMDS="$CMDS;;execute if block 8 100 0 minecraft:grass_block run tellraw @s \"LIDDEDGRASSPLACED\""
CMDS="$CMDS;;execute if block 10 100 0 minecraft:mycelium run tellraw @s \"LIDDEDMYCELIUMPLACED\""

# Now the lids go on. From here both have to give up and turn back to dirt.
CMDS="$CMDS;;setblock 8 101 0 minecraft:stone"
CMDS="$CMDS;;setblock 10 101 0 minecraft:stone"

# The rate goes up only now: a server handing out this many random ticks is too
# busy to build a rig on.
CMDS="$CMDS;;gamerule random_tick_speed 500"
CMDS="$CMDS;;tick sprint 600"
CMDS="$CMDS;;!wait 5"
CMDS="$CMDS;;gamerule random_tick_speed 3"

CMDS="$CMDS;;execute if block 1 100 0 minecraft:grass_block run tellraw @s \"GRASSSPREAD\""
CMDS="$CMDS;;execute if block 8 100 0 minecraft:dirt run tellraw @s \"LIDDEDGRASSDIED\""
CMDS="$CMDS;;execute if block 10 100 0 minecraft:dirt run tellraw @s \"LIDDEDMYCELIUMDIED\""

# Bone meal gets a lawn of its own, far enough from the spreading rig that the
# tufts cannot be mistaken for anything the random ticks did.
for z in -1 0 1; do
  CMDS="$CMDS;;setblock 19 100 $z minecraft:grass_block"
  CMDS="$CMDS;;setblock 20 100 $z minecraft:grass_block"
  CMDS="$CMDS;;setblock 21 100 $z minecraft:grass_block"
done
CMDS="$CMDS;;execute if block 20 101 0 minecraft:air run tellraw @s \"LAWNISBARE\""
CMDS="$CMDS;;teleport @s 20 103 0"
CMDS="$CMDS;;give @s minecraft:bone_meal 64"
CMDS="$CMDS;;!hotbar 0"
CMDS="$CMDS;;!useon 20 100 0 up"
CMDS="$CMDS;;!wait 2"
# One handful covers a patch, not one square, so ask the whole lawn.
for z in -1 0 1; do
  CMDS="$CMDS;;execute if block 19 101 $z minecraft:short_grass run tellraw @s \"BONEMEALGREW\""
  CMDS="$CMDS;;execute if block 20 101 $z minecraft:short_grass run tellraw @s \"BONEMEALGREW\""
  CMDS="$CMDS;;execute if block 21 101 $z minecraft:short_grass run tellraw @s \"BONEMEALGREW\""
done

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=2 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what grew ==="
grep "server says" join.log | grep -oE "NEIGHBOURISDIRT|LIDDEDGRASSPLACED|LIDDEDMYCELIUMPLACED|GRASSSPREAD|LIDDEDGRASSDIED|LIDDEDMYCELIUMDIED|LAWNISBARE|BONEMEALGREW" | sort -u
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "\[Error\]|panic" | tail -5

# Only the server's own reply counts: join.py echoes the commands it sends.
said() { grep "server says" join.log | grep -q "$1"; }
fail() { echo "########## GRASS TEST FAILED ($1) ##########"; exit 1; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }

said NEIGHBOURISDIRT      || fail "the dirt beside the grass never got placed"
said LIDDEDGRASSPLACED    || fail "the grass under the lid never got placed"
said LIDDEDMYCELIUMPLACED || fail "the mycelium under the lid never got placed"
said LAWNISBARE           || fail "something was already standing on the bone meal lawn"

said GRASSSPREAD          || fail "grass never spread onto the dirt beside it"
said LIDDEDGRASSDIED      || fail "grass under a solid block stayed grass"
said LIDDEDMYCELIUMDIED   || fail "mycelium under a solid block stayed mycelium"
said BONEMEALGREW         || fail "bone meal on grass grew nothing"
echo "########## GRASS TEST PASSED ##########"
