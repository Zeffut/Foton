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
