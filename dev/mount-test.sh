#!/bin/bash
# Open a mount's own inventory screen and put a saddle on it from inside it.
#
# A mount screen is not an open-screen packet: the client rebuilds the menu from
# the entity it already tracks, so `ClientboundMountScreenOpenPacket` is the only
# thing that says the screen opened at all. Nothing inside the server can tell a
# menu that was built from one that was also sent, which is exactly the shape of
# bug this repository keeps finding, so the packet is read off the wire here.
#
# The camel is the mount used because it is tame from birth -- every other one
# has to be broken in by riding it until its temper gives, which no command can
# do. Its saddle and body slots are the same code every horse and nautilus uses.
#
# Usage: bash dev/mount-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25620
RUN_DIR="$ROOT/run-mount"

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

# A floor to stand on, laid one block at a time because Foton has no `/fill`.
# Without it the camel falls out of the world before it can be clicked.
CMDS='gamemode creative'
for x in $(seq -2 3); do
  for z in $(seq -2 2); do
    CMDS="$CMDS;;setblock $x 99 $z minecraft:stone"
  done
done
CMDS="$CMDS;;teleport @s 0 100 0"
CMDS="$CMDS;;summon minecraft:camel 2 100 0"
CMDS="$CMDS;;clear @s"
CMDS="$CMDS;;give @s minecraft:saddle"
CMDS="$CMDS;;!hotbar 0"
# A camel walks off while the commands settle, so it is put back within reach
# right before every gesture aimed at it.
CMDS="$CMDS;;teleport @e[type=minecraft:camel] 2 100 0"
# Sneaking at a mount opens its screen.
CMDS="$CMDS;;!sneakuse camel"
# Slot 29 is the player's first hotbar slot inside that screen: the saddle slot,
# the body slot, the 27 main inventory slots, and then the hotbar.
CMDS="$CMDS;;!click 29"
# Slot 0 is the saddle. Landing there has to reach the camel's own equipment,
# which is what the SetEquipment packet coming back proves.
CMDS="$CMDS;;!click 0"
CMDS="$CMDS;;!close"
# The other way in, and the one a player actually uses: ride it, press E.
CMDS="$CMDS;;teleport @e[type=minecraft:camel] 2 100 0"
CMDS="$CMDS;;!useentity camel"
CMDS="$CMDS;;!mountscreen"

export JOIN_COMMANDS="$CMDS"
JOIN_WATCH_SECONDS=2 python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what happened ==="
grep -E "right-clicked|is carrying|a mount screen opened|was equipped|the screen was closed" join.log | tail -12
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "error|panic|incorrect|unknown" | tail -6

fail() { echo "########## MOUNT TEST FAILED ($1) ##########"; exit 1; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }

player=$(grep -o 'joined the world as entity [0-9]*' join.log | head -1 | awk '{print $NF}')
[ -n "$player" ] || fail "never learned the player entity id"
camel=$(grep -o 'right-clicked the camel (entity [0-9]*' join.log | head -1 | awk '{print $NF}')
[ -n "$camel" ] || fail "no camel spawned to click"

# The screen opened, for that camel, and said it carries no cargo -- a camel's
# `getInventoryColumns` is zero, and a wrong number here would size the client's
# grid wrong rather than fail loudly.
grep -q "a mount screen opened for entity $camel with 0 columns" join.log \
  || fail "sneaking at the camel opened no mount screen"

# Clicking the saddle into slot 0 reached the camel's real equipment.
grep -q "entity $camel was equipped in saddle" join.log \
  || fail "the saddle slot of the screen did not equip the camel"

# And the inventory key while riding opens it too, which is the path the
# `open_vehicle_inventory` player command takes.
grep -q "entity $camel is carrying \[$player\]" join.log \
  || fail "the camel never took the rider"
screens=$(grep -c "a mount screen opened" join.log)
[ "$screens" -ge 2 ] \
  || fail "the inventory key while riding opened no mount screen (saw $screens in all)"

echo "########## MOUNT TEST PASSED ##########"
