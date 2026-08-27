#!/bin/bash
# Stand an enderman in a ring of grass and watch it walk off with a block.
#
# `EndermanTakeBlockGoal` and `EndermanLeaveBlockGoal` sit at priorities 11 and
# 10 of the enderman's goal selector, and both are gated behind a die roll --
# one attempt in ten to take, one in a thousand to leave. The unit tests drive
# the goal bodies directly, which proves what the bodies do but not that the
# selector ever reaches them. This does: it summons an enderman and lets the
# real tick run.
#
# Midnight, because daylight makes an enderman teleport away from its ring, and
# clear weather, because rain hurts it into teleporting too.
#
# The ring is at the enderman's own feet level rather than under them: the take
# box is `y .. y+3`, so a floor is out of reach by construction.
#
# Usage: bash dev/enderman-block-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25629
RUN_DIR="$ROOT/run-enderman-block"

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

CMDS='gamemode spectator'
CMDS="$CMDS;;time set 18000"
CMDS="$CMDS;;weather clear"
CMDS="$CMDS;;teleport @s 0 108 0"
# Nothing that wandered in on its own may take part.
CMDS="$CMDS;;gamerule spawn_mobs false"
CMDS="$CMDS;;kill @e[type=minecraft:enderman]"

# Stone floor to stand on, then a ring of grass at standing height so the take
# box has something holdable in it. The middle stays empty for the enderman.
for x in -1 0 1; do
  for z in -1 0 1; do
    CMDS="$CMDS;;setblock $x 99 $z minecraft:stone"
    if [ "$x" != 0 ] || [ "$z" != 0 ]; then
      CMDS="$CMDS;;setblock $x 100 $z minecraft:grass_block"
    fi
  done
done

CMDS="$CMDS;;summon minecraft:enderman 0 100 0 {PersistenceRequired:1b,Silent:1b,Tags:[\"carrier\"]}"

# The controls, asked before anything is allowed to tick.
CMDS="$CMDS;;execute if entity @e[type=minecraft:enderman,tag=carrier] run tellraw @s \"ENDERMANUP\""
CMDS="$CMDS;;execute if block 1 100 0 minecraft:grass_block run tellraw @s \"RINGDOWN\""
# And the flag came off the summon NBT, which is also the load path the carried
# block itself uses.
CMDS="$CMDS;;execute if entity @e[type=minecraft:enderman,tag=carrier,nbt={PersistenceRequired:1b}] run tellraw @s \"ENDERMANPINNED\""

# A take attempt is one tick in ten and lands on a reachable grass cell about
# one time in twelve, so this asks several times rather than once: an enderman
# that has already put its block back down would read as a failure at a single
# checkpoint.
CARRYING='nbt={carriedBlockState:{Name:"minecraft:grass_block"}}'
for _ in 1 2 3 4 5 6; do
  CMDS="$CMDS;;tick sprint 400"
  CMDS="$CMDS;;execute if entity @e[type=minecraft:enderman,tag=carrier,$CARRYING] run tellraw @s \"ENDERMANCARRIES\""
done

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=3 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what the server said ==="
grep "server says" join.log | grep -oE "ENDERMANUP|RINGDOWN|ENDERMANPINNED|ENDERMANCARRIES"
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "\[Error\]|panic|Unknown|Incorrect" | tail -8

# Only the server's own reply counts: join.py echoes the commands it sends.
said() { grep "server says" join.log | grep -q "$1"; }
fail() { echo "########## ENDERMAN BLOCK TEST FAILED ($1) ##########"; exit 1; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }

said ENDERMANUP     || fail "the enderman never spawned"
said RINGDOWN       || fail "the ring of grass never got placed"
said ENDERMANPINNED || fail "the summon NBT never reached the mob"

said ENDERMANCARRIES || fail "the enderman never picked a block up"
echo "########## ENDERMAN BLOCK TEST PASSED ##########"
