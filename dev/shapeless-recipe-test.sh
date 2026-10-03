#!/usr/bin/env bash
# Compile the Zelda-style recipe fixture against exact Paper bytes, then run it on Foton.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$REPO/dev/paper-api-test-lib.sh"
scratch="$(mktemp -d "${TMPDIR:-/tmp}/foton-shapeless-recipe.XXXXXX")"
trap 'rm -rf -- "$scratch"' EXIT

paper_api="$(paper_api_prepare "${1:-${FOTON_PAPER_API_JAR:-}}" "$scratch")"

foton_api="$REPO/plugin-api/build/foton-plugin-api.jar"
[ -f "$foton_api" ] || { echo 'Run bash dev/build-plugin-api.sh first' >&2; exit 2; }
bash "$REPO/dev/fetch-plugin-api-libs.sh" --check
mkdir -p "$scratch/classes"
javac --release 21 -cp "$paper_api:$REPO/plugin-api/lib/*" -d "$scratch/classes" \
  "$REPO/plugin-api/check/ShapelessRecipeParity.java"
java -cp "$scratch/classes:$foton_api:$REPO/plugin-api/lib/*" ShapelessRecipeParity
