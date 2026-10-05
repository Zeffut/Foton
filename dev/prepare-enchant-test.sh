#!/usr/bin/env bash
# Execute a Paper-compiled enchantment listener through Foton's actual dispatcher.
set -euo pipefail
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$REPO/dev/paper-api-test-lib.sh"
scratch="$(mktemp -d "${TMPDIR:-/tmp}/foton-prepare-enchant.XXXXXX")"
trap 'rm -rf -- "$scratch"' EXIT
paper_api="$(paper_api_prepare "${1:-${FOTON_PAPER_API_JAR:-}}" "$scratch")"
foton_api="$REPO/plugin-api/build/foton-plugin-api.jar"
fixture="$REPO/dev/fixtures/enchant-prepare-plugin"
mkdir -p "$scratch/classes" "$scratch/plugins"
javac --release 21 -cp "$paper_api:$REPO/plugin-api/lib/*" -d "$scratch/classes" "$fixture/PaperEnchantFixture.java"
cp "$fixture/plugin.yml" "$scratch/classes/"
jar --create --file "$scratch/plugins/PaperEnchantFixture.jar" -C "$scratch/classes" .
javac --release 21 -cp "$foton_api:$REPO/plugin-api/lib/*:$scratch/classes" -d "$scratch/classes" "$fixture/EnchantmentBridgeCheck.java"
java -cp "$scratch/classes:$foton_api:$REPO/plugin-api/lib/*" EnchantmentBridgeCheck "$scratch/plugins"
