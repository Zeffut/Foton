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
# A plugin is a JVM artifact: the classes it extends have to compile
# before anything else about plugin support can be true.
run "plugin api builds"                            bash dev/build-plugin-api.sh
# `-D warnings` is not decoration. Without it this suite stayed green while
# `Drowned::travel_in_water` recursed into itself and eleven dead squid
# constants sat in the tree -- clippy had been printing both the whole time
# and nothing was reading. `--workspace` because the old invocation only
# checked the default package.
run "cargo clippy -r --workspace --all-targets --all-features -D warnings" cargo clippy -r --workspace --all-targets --all-features -- -D warnings
run "cargo test --workspace"                       cargo test --workspace
run "test counts are current"                      python3 dev/count-tests.py --check
# Four test files sat in dev/ that nothing ran, which is the same shape as the
# clippy note above: the checks existed and nobody was reading them.
run "dev tooling tests"                            python3 -m unittest discover -s dev -p "test_*.py"

echo
if [ $FAIL -eq 0 ]; then echo "########## ALL GREEN ##########"; else echo "########## FAILURES ##########"; fi
exit $FAIL
