#!/usr/bin/env bash
# Execute the same pinned plugin inputs against Paper and Foton in temp worlds.
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
SCENARIO=${1:---fixture}
shift || true
if [[ "$SCENARIO" == --fixture ]]; then
  SCENARIO=fixture
  FIXTURE=(--fixture)
else
  FIXTURE=()
  if [[ ! -f "$SCENARIO" ]]; then
    SCENARIO="$ROOT/dev/compat/scenarios/$SCENARIO.json"
  fi
fi
JAVA_BIN=${PAPER_ORACLE_JAVA:-$(command -v java)}
JAVAC_BIN=${PAPER_ORACLE_JAVAC:-$(command -v javac)}
FOTON_BIN=${FOTON_ORACLE_BIN:-${CARGO_TARGET_DIR:-$ROOT/target}/debug/foton}
API_JAR=${FOTON_ORACLE_API_JAR:-$ROOT/plugin-api/build/foton-plugin-api.jar}
LIBS=${FOTON_ORACLE_LIBS:-$ROOT/plugin-api/lib}
python3 "$ROOT/dev/compat/compare.py" run "$SCENARIO" \
  --java "$JAVA_BIN" --javac "$JAVAC_BIN" \
  --foton-bin "$FOTON_BIN" --api-jar "$API_JAR" --libs "$LIBS" "${FIXTURE[@]}" "$@"
