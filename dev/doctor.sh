#!/bin/bash
# Check that the development environment is complete and self-consistent.
# Usage: bash dev/doctor.sh
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.." || exit 1

OK=0; KO=0
chk() {
  local label="$1"; shift
  if out=$("$@" 2>&1); then
    printf '  [ OK ] %-32s %s\n' "$label" "$(echo "$out" | head -1 | cut -c1-46)"
    OK=$((OK + 1))
  else
    printf '  [FAIL] %-32s %s\n' "$label" "$(echo "$out" | head -1 | cut -c1-46)"
    KO=$((KO + 1))
  fi
}

echo "=== Tools ==="
chk "rustc"    rustc --version
chk "cargo"    cargo --version
chk "typos"    typos --version
chk "ast-grep" ast-grep --version
chk "prek"     prek --version
chk "gh"       gh --version
chk "java"     java -version

echo
echo "=== Repository ==="
chk "origin remote"   git remote get-url origin
chk "gh auth"         gh auth status

echo
echo "=== Version consistency ==="
CARGO_V=$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)".*/\1/')
MC_TARGET=${CARGO_V##*+mc}
echo "  Cargo.toml           : $CARGO_V (targets MC $MC_TARGET)"
if [ -d minecraft-src/.git ]; then
  MC_SRC=$(git -C minecraft-src log --oneline -1 --format=%s)
  echo "  minecraft-src (HEAD) : $MC_SRC"
  if [ "$MC_SRC" = "$MC_TARGET" ]; then
    echo "  [ OK ] vanilla sources match the target version"
    OK=$((OK + 1))
  else
    echo "  [FAIL] version mismatch: rerun ./update-minecraft-src.sh with JDK 25"
    KO=$((KO + 1))
  fi
else
  echo "  [FAIL] minecraft-src missing: run ./update-minecraft-src.sh with JDK 25"
  KO=$((KO + 1))
fi

if [ -d "$HOME/FotonExtractor" ]; then
  echo "  [ OK ] FotonExtractor checkout present"
  OK=$((OK + 1))
else
  echo "  [FAIL] FotonExtractor checkout missing"
  KO=$((KO + 1))
fi

echo
echo "=== Bedrock (Geyser pin) ==="
GEYSER_VERSION=$(grep -m1 'pub const GEYSER_VERSION' foton-bedrock/src/geyser.rs | sed -E 's/.*"([^"]+)".*/\1/')
GEYSER_BUILD=$(grep -m1 'pub const GEYSER_BUILD' foton-bedrock/src/geyser.rs | grep -oE '[0-9]+' | tail -1)
if [ -z "$GEYSER_VERSION" ] || [ -z "$GEYSER_BUILD" ]; then
  echo "  [FAIL] could not read GEYSER_VERSION/GEYSER_BUILD from foton-bedrock/src/geyser.rs"
  KO=$((KO + 1))
else
  GEYSER_API="https://download.geysermc.org/v2/projects/geyser/versions/$GEYSER_VERSION/builds/$GEYSER_BUILD"
  echo "  pinned build         : Geyser $GEYSER_VERSION build $GEYSER_BUILD"
  if out=$(curl -fsS --max-time 10 "$GEYSER_API" 2>&1); then
    echo "  [ OK ] pinned build still exists on GeyserMC"
    OK=$((OK + 1))
  else
    # curl -f turns an HTTP error into exit 22 -- the server answered, and it
    # said the pinned build is gone. Anything else (DNS failure, connection
    # refused, timeout) means the check never actually ran, which is not the
    # same failure and must not fail doctor.sh offline.
    CURL_STATUS=$?
    if [ "$CURL_STATUS" -eq 22 ]; then
      echo "  [FAIL] pinned Geyser $GEYSER_VERSION build $GEYSER_BUILD no longer exists on GeyserMC: bump GEYSER_VERSION/GEYSER_BUILD in foton-bedrock/src/geyser.rs"
      KO=$((KO + 1))
    else
      echo "  [WARN] could not reach GeyserMC to verify the pin (offline?): $out"
    fi
  fi
fi

echo
echo "=== Hooks ==="
if [ -f .git/hooks/pre-commit ]; then
  echo "  [ OK ] pre-commit hook installed"
  OK=$((OK + 1))
else
  echo "  [FAIL] hook missing: run 'prek install'"
  KO=$((KO + 1))
fi

echo
echo "########## $OK passed / $KO failed ##########"
[ $KO -eq 0 ]
