#!/bin/bash
# Load a datapack's functions off disk and prove the server actually runs them.
#
# A function is the one command whose body is written before the server starts,
# so the interesting failures are not "the command was wrong" but "the file was
# never read", "the file was read and quietly dropped", or "the lines ran in
# some other order". Every probe below is a marker a specific line of a
# specific file prints, so a missing marker names the file that did not run.
#
# The two tag-driven entry points cannot print anything, because they run
# before any player is connected. They leave a boss bar behind instead, and a
# pig carries its value back out through `execute store result entity`, which
# dev/store-entity-test.sh already proves works. The values are chosen so the
# three outcomes are distinguishable: 7 means the load tag ran and the tick tag
# did not, 99 means both did, and no value at all means neither.
#
# Usage: bash dev/function-test.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1
ROOT=$(pwd)

PORT=25719
RUN_DIR="$ROOT/run-function"

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

# The datapack directory sits beside the save data, the way vanilla keeps
# <level>/datapacks. save_path comes from worlds.toml and defaults to "saves".
PACK="$RUN_DIR/saves/datapacks/steel_test/data"
FN="$PACK/test/function"
mkdir -p "$FN/nested" "$PACK/test/tags/function" "$PACK/minecraft/tags/function" || exit 1

cat > "$PACK/../pack.mcmeta" <<'EOF'
{"pack": {"description": "steel function test", "pack_format": 96}}
EOF

# A comment must stay a comment even when it looks exactly like a command.
cat > "$FN/greet.mcfunction" <<'EOF'
#tellraw @a {"text":"FN_COMMENT_RAN"}

tellraw @a {"text":"FN_PLAIN"}
EOF

cat > "$FN/nested/inner.mcfunction" <<'EOF'
tellraw @a {"text":"FN_INNER"}
EOF

# The caller has to come back and keep going after the callee finishes.
cat > "$FN/outer.mcfunction" <<'EOF'
function test:nested/inner
tellraw @a {"text":"FN_OUTER_AFTER_CALL"}
EOF

# Three or more lines take the continuation path instead of being queued
# one by one, so their order is worth pinning down separately.
cat > "$FN/multi.mcfunction" <<'EOF'
tellraw @a {"text":"FN_MULTI_1"}
tellraw @a {"text":"FN_MULTI_2"}
tellraw @a {"text":"FN_MULTI_3"}
EOF

# A backslash joins the next line into the same command.
cat > "$FN/continued.mcfunction" <<'EOF'
tellraw @a \
{"text":"FN_CONTINUED"}
EOF

# aaa_early loads before zzz_late does. Calling it anyway is what says names
# are resolved when the call runs and not while the file is being compiled.
cat > "$FN/aaa_early.mcfunction" <<'EOF'
function test:zzz_late
EOF

cat > "$FN/zzz_late.mcfunction" <<'EOF'
tellraw @a {"text":"FN_LATE"}
EOF

cat > "$FN/tagged_a.mcfunction" <<'EOF'
tellraw @a {"text":"FN_TAG_A"}
EOF

cat > "$FN/tagged_b.mcfunction" <<'EOF'
tellraw @a {"text":"FN_TAG_B"}
EOF

cat > "$FN/cond_true.mcfunction" <<'EOF'
return 1
EOF

cat > "$FN/cond_false.mcfunction" <<'EOF'
return 0
EOF

# The first line is perfectly valid. If a broken file compiled up to its bad
# line and kept what it had, this marker would appear.
cat > "$FN/broken.mcfunction" <<'EOF'
tellraw @a {"text":"FN_BROKEN_FIRST_LINE"}
thiscommanddoesnotexist
EOF

cat > "$FN/on_load.mcfunction" <<'EOF'
bossbar add steel_test:probe {"text":"probe"}
bossbar set steel_test:probe max 7
EOF

cat > "$FN/on_tick.mcfunction" <<'EOF'
bossbar set steel_test:probe max 99
EOF

cat > "$PACK/test/tags/function/group.json" <<'EOF'
{"values": ["test:tagged_a", "test:tagged_b"]}
EOF

cat > "$PACK/minecraft/tags/function/load.json" <<'EOF'
{"values": ["test:on_load"]}
EOF

cat > "$PACK/minecraft/tags/function/tick.json" <<'EOF'
{"values": ["test:on_tick"]}
EOF

# A tag that names a function nobody wrote must not become a tag that runs
# nothing; the whole tag is dropped, so calling it is an error.
cat > "$PACK/test/tags/function/incomplete.json" <<'EOF'
{"values": ["test:tagged_a", "test:nobody_wrote_this"]}
EOF

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
CMDS="$CMDS;;difficulty normal"
CMDS="$CMDS;;time set noon"
# One throwaway setblock first: the very first one of a run can land before the
# chunk around the player is ready.
CMDS="$CMDS;;setblock 0 149 0 minecraft:stone"
CMDS="$CMDS;;teleport @s 0 150 0"
CMDS="$CMDS;;!wait 2"
# Something for the probe pig to stand on, so it is still in range later.
for z in 0 1 2 3; do
  CMDS="$CMDS;;setblock 0 149 $z minecraft:stone"
done
CMDS="$CMDS;;!wait 1"

# --- one file, its comment and its command -------------------------------
CMDS="$CMDS;;function test:greet"
CMDS="$CMDS;;!wait 1"

# --- a function calling a function, and the caller carrying on -----------
CMDS="$CMDS;;function test:outer"
CMDS="$CMDS;;!wait 1"

# --- more lines than the queue schedules one by one ----------------------
CMDS="$CMDS;;function test:multi"
CMDS="$CMDS;;!wait 1"

# --- a line continued onto the next line ---------------------------------
CMDS="$CMDS;;function test:continued"
CMDS="$CMDS;;!wait 1"

# --- a call to a function that had not been compiled yet -----------------
CMDS="$CMDS;;function test:aaa_early"
CMDS="$CMDS;;!wait 1"

# --- a whole tag ---------------------------------------------------------
CMDS="$CMDS;;function #test:group"
CMDS="$CMDS;;!wait 1"

# --- a tag that could not be resolved is not an empty tag ----------------
CMDS="$CMDS;;function #test:incomplete"
CMDS="$CMDS;;!wait 1"

# --- a file that does not compile keeps none of its lines ----------------
CMDS="$CMDS;;function test:broken"
CMDS="$CMDS;;!wait 1"

# --- a name nobody wrote is an error, not a silent nothing ---------------
CMDS="$CMDS;;function test:nobody_wrote_this"
CMDS="$CMDS;;!wait 1"

# --- execute if|unless function ------------------------------------------
CMDS="$CMDS;;execute if function test:cond_true run tellraw @s {\"text\":\"FN_IF_TRUE\"}"
CMDS="$CMDS;;execute if function test:cond_false run tellraw @s {\"text\":\"FN_IF_FALSE_RAN\"}"
CMDS="$CMDS;;execute unless function test:cond_false run tellraw @s {\"text\":\"FN_UNLESS_FALSE\"}"
CMDS="$CMDS;;execute unless function test:cond_true run tellraw @s {\"text\":\"FN_UNLESS_TRUE_RAN\"}"
CMDS="$CMDS;;!wait 1"

# --- what the load and tick tags left behind -----------------------------
# The boss bar only exists if the load tag ran, and its max is only 99 if the
# tick tag ran on top of it. A pig carries the value back out, because the
# bar's own feedback message renders its name and not a greppable number.
CMDS="$CMDS;;summon minecraft:pig 0 150 3 {Tags:[\"fn_probe\"]}"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute if entity @e[tag=fn_probe,distance=..20] run tellraw @s {\"text\":\"FN_PROBE_READY\"}"
CMDS="$CMDS;;execute store result entity @n[tag=fn_probe,distance=..20] data.probe int 1 run bossbar get steel_test:probe max"
CMDS="$CMDS;;!wait 1"
CMDS="$CMDS;;execute if entity @e[tag=fn_probe,nbt={data:{probe:99}},distance=..20] run tellraw @s {\"text\":\"FN_TICK_TAG\"}"
CMDS="$CMDS;;execute if entity @e[tag=fn_probe,nbt={data:{probe:7}},distance=..20] run tellraw @s {\"text\":\"FN_LOAD_TAG_ONLY\"}"

CMDS="$CMDS;;tellraw @s {\"text\":\"FN_ALIVE\"}"
CMDS="$CMDS;;kill @e[tag=fn_probe,distance=..20]"

export JOIN_COMMANDS="$CMDS"
python3 "$ROOT/dev/join.py" "$PORT" > join.log 2>&1
STATUS=$?

cleanup

echo "=== what happened ==="
grep -E "server says" join.log
echo "=== server ==="
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "function|datapack" | head -10
sed 's/\x1b\[[0-9;]*[A-Za-z]//g' server.log | grep -iE "panic" | tail -5

fail() { echo "########## FUNCTION TEST FAILED ($1) ##########"; exit 1; }

[ $STATUS -eq 0 ] || { tail -20 join.log; fail "the client never settled"; }

# Grepping the whole log would also match the echo of the command that asks
# the question, which is printed whether it produced output or not.
said() { grep -q "server says: $1" join.log; }
said_times() { grep -c "server says: $1" join.log; }

said FN_PLAIN || fail "/function never ran the file's only command"
said FN_COMMENT_RAN && fail "a commented-out line was compiled as a command"

said FN_INNER || fail "a function called from a function never ran"
said FN_OUTER_AFTER_CALL || fail "the caller never resumed after the callee finished"

said FN_MULTI_1 || fail "a longer function's first line never ran"
said FN_MULTI_3 || fail "a longer function's last line never ran"
FIRST=$(grep -n "server says: FN_MULTI_1" join.log | head -1 | cut -d: -f1)
LAST=$(grep -n "server says: FN_MULTI_3" join.log | head -1 | cut -d: -f1)
[ -n "$FIRST" ] && [ -n "$LAST" ] && [ "$FIRST" -lt "$LAST" ] \
  || fail "a longer function's lines ran out of order"

said FN_CONTINUED || fail "a backslash-continued line never became one command"
said FN_LATE || fail "a call to a function compiled later never resolved"

said FN_TAG_A || fail "a function tag's first function never ran"
said FN_TAG_B || fail "a function tag's second function never ran"
[ "$(said_times FN_TAG_A)" = "1" ] || fail "a function tag ran its function more than once"

said FN_BROKEN_FIRST_LINE && fail "a file that does not compile kept the lines before its bad one"

said FN_IF_TRUE || fail "execute if function never ran the command after a passing function"
said FN_IF_FALSE_RAN && fail "execute if function ran the command after a function that returned 0"
said FN_UNLESS_FALSE || fail "execute unless function never ran after a function that returned 0"
said FN_UNLESS_TRUE_RAN && fail "execute unless function ran after a function that returned 1"

grep -q "commands.bossbar.get.max" join.log \
  || fail "#minecraft:load never ran, so the boss bar it creates does not exist"
said FN_PROBE_READY || fail "the probe pig was never summoned"
said FN_LOAD_TAG_ONLY && fail "#minecraft:load ran but #minecraft:tick never did"
said FN_TICK_TAG || fail "#minecraft:tick never changed what #minecraft:load left behind"

said FN_ALIVE || fail "the server stopped answering after an unknown function name"

echo "########## FUNCTION TEST PASSED ##########"
