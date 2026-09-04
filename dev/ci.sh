#!/bin/bash
# Replay the full verification suite locally.
# Usage: bash dev/ci.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1

# `python3` on Windows is usually the Microsoft Store stub: it prints an
# advertisement, runs nothing, and exits 0. The five Python steps below would
# then report PASS without having executed, which is worse than failing.
# Resolve an interpreter that actually runs code.
PY=""
for candidate in python3 python py; do
  if [ "$("$candidate" -c 'print(3)' 2>/dev/null)" = "3" ]; then PY="$candidate"; break; fi
done
if [ -z "$PY" ]; then
  echo "no working Python interpreter (tried python3, python, py)" >&2
  exit 1
fi
# gen-config-docs.py opens its output before it dies on a cp1252 encode, which
# is how CONFIGURATION.md once ended up empty.
export PYTHONUTF8=1

FAIL=0
run() {
  echo "=============================="
  echo ">>> $1"
  echo "=============================="
  shift
  START=$(date +%s)
  if "$@" > /tmp/foton-ci.log 2>&1; then
    echo "PASS ($(($(date +%s) - START))s)"
  else
    echo "FAIL ($(($(date +%s) - START))s)"
    # Show the errors first, then the tail. Tailing alone hid a javac failure
    # behind forty-four warnings that came after it, and the log on a CI runner
    # is the only copy anyone gets.
    grep -inE "error" /tmp/foton-ci.log | head -20
    tail -25 /tmp/foton-ci.log
    FAIL=1
  fi
}

run "cargo fmt --all --check"                      cargo fmt --all --check
run "typos"                                        typos
run "config reference is current"                  "$PY" dev/gen-config-docs.py --check
run "site builds"                                  "$PY" dev/gen-site.py --check
# `-D warnings` is not decoration. Without it this suite stayed green while
# `Drowned::travel_in_water` recursed into itself and eleven dead squid
# constants sat in the tree -- clippy had been printing both the whole time
# and nothing was reading. `--workspace` because the old invocation only
# checked the default package.
run "cargo clippy -r --workspace --all-targets --all-features -D warnings" cargo clippy -r --workspace --all-targets --all-features -- -D warnings
run "cargo test --workspace"                       cargo test --workspace
# A plugin is a JVM artifact: the classes it extends have to compile before
# anything else about plugin support can be true.
#
# After the cargo steps, not before them. The Java enchantment handles are
# generated from `foton-registry/src/generated/`, which is gitignored and
# written by that crate's build script -- so on a machine that has never built
# the workspace, running this first reads a file nobody has produced yet. That
# is why Build Release failed on the runner while passing on every developer's
# machine, where the file was left over from an earlier build.
run "plugin api builds"                            bash dev/build-plugin-api.sh --check
# RegisterNatives is all-or-nothing: one registered method the class does
# not declare and no plugin loads at all, one declared method left
# unregistered and the first plugin to call it takes an
# UnsatisfiedLinkError. Neither shows up in a build.
run "every native is registered"                   "$PY" dev/check-natives.py --quiet
run "test counts are current"                      "$PY" dev/count-tests.py --check
# Four test files sat in dev/ that nothing ran, which is the same shape as the
# clippy note above: the checks existed and nobody was reading them.
run "dev tooling tests"                            "$PY" -m unittest discover -s dev -p "test_*.py"

echo
if [ $FAIL -eq 0 ]; then echo "########## ALL GREEN ##########"; else echo "########## FAILURES ##########"; fi
exit $FAIL
