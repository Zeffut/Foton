#!/bin/sh
# Install or update Foton.
#
#   curl -fsSL https://foton.zeffut.fr/install.sh | sh
#   curl -fsSL https://foton.zeffut.fr/install.sh | sh -s -- --update
#
# Prompts read /dev/tty, not standard input: under `curl | sh` standard input
# is this script's own text, and a read there answers questions with source
# code. With no terminal the defaults are taken and said out loud.
set -eu

REPO=Zeffut/Foton
API="https://api.github.com/repos/$REPO/releases/latest"
UPDATE=0
[ "${1:-}" = "--update" ] && UPDATE=1

red() { printf '\033[31m%s\033[0m\n' "$1" >&2; }
bold() { printf '\033[1m%s\033[0m\n' "$1"; }
die() { red "error: $1"; exit 1; }

have_tty=0
[ -r /dev/tty ] && have_tty=1

# ask <prompt> <default> -- echoes the answer
ask() {
  if [ "$have_tty" -eq 0 ]; then
    printf '%s' "$2"
    return
  fi
  printf '%s [%s]: ' "$1" "$2" > /dev/tty
  read -r reply < /dev/tty || reply=""
  [ -n "$reply" ] && printf '%s' "$reply" || printf '%s' "$2"
}

need() { command -v "$1" >/dev/null 2>&1 || die "$1 is required and was not found"; }
need curl

case "$(uname -s)" in
  Darwin) OS=macos ;;
  Linux)  OS=linux ;;
  *) die "unsupported system: $(uname -s). Foton publishes macOS and Linux builds." ;;
esac
case "$(uname -m)" in
  arm64|aarch64) ARCH=aarch64 ;;
  x86_64|amd64)  ARCH=x86_64 ;;
  *) die "unsupported processor: $(uname -m)" ;;
esac

if [ "$OS" = linux ]; then
  ASSET="foton-linux-x86_64-musl"
  [ "$ARCH" = x86_64 ] || die "Linux builds are x86_64 only for now; yours is $ARCH"
else
  ASSET="foton-macos-$ARCH"
fi

bold "Foton installer"
printf 'Looking up the latest release...\n'
META=$(curl -fsSL "$API") || die "could not reach the GitHub API"
TAG=$(printf '%s' "$META" | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -1)
[ -n "$TAG" ] || die "no published release yet"
BASE="https://github.com/$REPO/releases/download/$TAG"

printf 'Latest release: %s\n' "$TAG"
printf 'Asset for this machine: %s\n' "$ASSET"

if [ "$UPDATE" -eq 1 ]; then
  [ -x ./foton ] || die "--update must run inside an existing installation"
  CURRENT=$(./foton --version 2>/dev/null | awk '{print $2}')
  if [ "v$CURRENT" = "$TAG" ]; then
    bold "Already on $TAG. Nothing to do."
    exit 0
  fi
  printf 'Updating from %s to %s\n' "$CURRENT" "$TAG"
  DIR=.
else
  DIR=$(ask "Where should Foton live?" "./foton")
  if [ -e "$DIR/foton" ]; then
    OVERWRITE=$(ask "$DIR already has Foton in it. Replace the binary?" "no")
    case "$OVERWRITE" in y|Y|yes|Yes) ;; *) die "stopping, nothing was changed" ;; esac
  fi
  mkdir -p "$DIR"
fi

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

printf 'Downloading...\n'
curl -fsSL "$BASE/$ASSET" -o "$TMP/foton" || die "could not download $ASSET from $TAG"
curl -fsSL "$BASE/SHA256SUMS" -o "$TMP/SHA256SUMS" || die "could not download SHA256SUMS"

printf 'Verifying...\n'
EXPECTED=$(grep " $ASSET\$" "$TMP/SHA256SUMS" | awk '{print $1}')
[ -n "$EXPECTED" ] || die "$ASSET is not listed in SHA256SUMS"
if command -v shasum >/dev/null 2>&1; then
  ACTUAL=$(shasum -a 256 "$TMP/foton" | awk '{print $1}')
else
  ACTUAL=$(sha256sum "$TMP/foton" | awk '{print $1}')
fi
if [ "$EXPECTED" != "$ACTUAL" ]; then
  rm -f "$TMP/foton"
  die "checksum mismatch -- the download does not match the published release"
fi

[ -f "$DIR/foton" ] && mv "$DIR/foton" "$DIR/foton.previous"
mv "$TMP/foton" "$DIR/foton"
chmod +x "$DIR/foton"
bold "Installed $TAG to $DIR/foton"

if [ "$UPDATE" -eq 1 ]; then
  bold "Updated. Your config/ and saves/ were left alone."
  exit 0
fi

printf 'Writing the default configuration...\n'
( cd "$DIR" && ./foton --generate-config ) || die "could not generate the configuration"

if [ "$have_tty" -eq 0 ]; then
  bold "No terminal here, so the defaults were kept. Edit $DIR/config/ to change them."
  exit 0
fi

NAME=$(ask "Server name" "A Foton Server")
PORT=$(ask "Port" "25565")
PLAYERS=$(ask "Maximum players" "20")
ONLINE=$(ask "Require a Mojang account to join?" "yes")
DIFFICULTY=$(ask "Difficulty (peaceful, easy, normal, hard)" "normal")

case "$ONLINE" in n|N|no|No) ONLINE_VALUE=false ;; *) ONLINE_VALUE=true ;; esac

set_key() {  # set_key <file> <key> <value>
  if grep -q "^$2 *=" "$1"; then
    sed -i.bak "s|^$2 *=.*|$2 = $3|" "$1" && rm -f "$1.bak"
  fi
}
set_key "$DIR/config/config.toml" motd "\"$NAME\""
set_key "$DIR/config/config.toml" server_port "$PORT"
set_key "$DIR/config/config.toml" max_players "$PLAYERS"
set_key "$DIR/config/config.toml" online_mode "$ONLINE_VALUE"
set_key "$DIR/config/worlds.toml" difficulty "\"$DIFFICULTY\""

bold "Done."
printf 'Start it with:  cd %s && ./foton\n' "$DIR"
START=$(ask "Start it now?" "yes")
case "$START" in y|Y|yes|Yes) cd "$DIR" && exec ./foton ;; esac
