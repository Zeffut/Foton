#!/bin/bash
# Replay the full verification suite locally.
# Usage: bash dev/ci.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1

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
    tail -25 /tmp/foton-ci.log
    FAIL=1
  fi
}

run "cargo fmt --all --check"                      cargo fmt --all --check
run "typos"                                        typos
run "config reference is current"                  python3 dev/gen-config-docs.py --check
run "site builds"                                  python3 dev/gen-site.py --check
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
run "test counts are current"                      python3 dev/count-tests.py --check
# Four test files sat in dev/ that nothing ran, which is the same shape as the
# clippy note above: the checks existed and nobody was reading them.
run "dev tooling tests"                            python3 -m unittest discover -s dev -p "test_*.py"

echo
if [ $FAIL -eq 0 ]; then echo "########## ALL GREEN ##########"; else echo "########## FAILURES ##########"; fi
exit $FAIL
