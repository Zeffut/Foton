#!/bin/bash
# The player reports filed with /bug that still need work.
# Usage: bash dev/reports.sh [number | --all | --category <name> | --json]
#
# A thin wrapper so every agent entry point can name one command that works on
# both checkouts; see dev/python.sh for why naming an interpreter directly does
# not. The loop these reports go through is documented in REPORTING.md.
cd "$(dirname "$0")/.." || exit 1
. dev/python.sh || exit 1
exec "$PY" dev/reports.py "$@"
