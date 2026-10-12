#!/usr/bin/env bash
# CraftPlayer#getHandle() must lead to the player's Netty channel by the names ViaVersion looks for.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
scratch="$(mktemp -d "${TMPDIR:-/tmp}/foton-via-handle.XXXXXX")"
trap 'rm -rf -- "$scratch"' EXIT

foton_api="$REPO/plugin-api/build/foton-plugin-api.jar"
[ -f "$foton_api" ] || { echo 'Run bash dev/build-plugin-api.sh first' >&2; exit 2; }
bash "$REPO/dev/fetch-plugin-api-libs.sh" --check
mkdir -p "$scratch/classes"
javac --release 21 -cp "$foton_api:$REPO/plugin-api/lib/*" -d "$scratch/classes" \
  "$REPO/plugin-api/check/ViaPlayerHandleCheck.java"
java -cp "$scratch/classes:$foton_api:$REPO/plugin-api/lib/*" ViaPlayerHandleCheck
