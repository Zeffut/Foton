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
  if "$@" > /tmp/steel-ci.log 2>&1; then
    echo "PASS ($(($(date +%s) - START))s)"
  else
    echo "FAIL ($(($(date +%s) - START))s)"
    tail -25 /tmp/steel-ci.log
    FAIL=1
  fi
}

run "cargo fmt --all --check"                      cargo fmt --all --check
run "typos"                                        typos
run "cargo clippy -r --all-targets --all-features" cargo clippy -r --all-targets --all-features
run "cargo test --workspace"                       cargo test --workspace

echo
if [ $FAIL -eq 0 ]; then echo "########## ALL GREEN ##########"; else echo "########## FAILURES ##########"; fi
exit $FAIL
