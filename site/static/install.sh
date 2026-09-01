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

# The installed binary's name -- foton.exe on Windows, foton everywhere else.
# Every reference to it below goes through this variable.
BIN=foton

red() { printf '\033[31m%s\033[0m\n' "$1" >&2; }
bold() { printf '\033[1m%s\033[0m\n' "$1"; }
die() { red "error: $1"; exit 1; }

# /dev/tty can exist and still not be openable -- a detached session, a cron
# job, a container without a terminal. `[ -r /dev/tty ]` says yes there and
# the open then fails, so the test is an actual open, not a permission check.
have_tty=0
if (exec 3< /dev/tty) 2>/dev/null; then
  have_tty=1
fi

# ask <prompt> <default> -- echoes the answer
ask() {
  if [ "$have_tty" -eq 0 ]; then
    printf '%s' "$2"
    return
  fi
  printf '%s [%s]: ' "$1" "$2" > /dev/tty 2>/dev/null || {
    printf '%s' "$2"
    return
  }
  # A read that fails mid-run must fall back rather than kill the install.
  reply=""
  read -r reply < /dev/tty 2>/dev/null || reply=""
  [ -n "$reply" ] && printf '%s' "$reply" || printf '%s' "$2"
}

need() { command -v "$1" >/dev/null 2>&1 || die "$1 is required and was not found"; }
need curl

# Windows has no native POSIX shell, so this script only ever runs there
# inside Git Bash, MSYS2 or Cygwin, which report one of these uname strings.
# WSL reports plain "Linux" and needs no special case: the Linux binary is
# correct there too.
case "$(uname -s)" in
  Darwin) OS=macos ;;
  Linux)  OS=linux ;;
  MINGW*|MSYS*|CYGWIN*) OS=windows ;;
  *) die "unsupported system: $(uname -s). Foton publishes macOS, Linux and Windows builds." ;;
esac
case "$(uname -m)" in
  arm64|aarch64) ARCH=aarch64 ;;
  x86_64|amd64)  ARCH=x86_64 ;;
  *) die "unsupported processor: $(uname -m)" ;;
esac

case "$OS" in
  linux)   ASSET="foton-linux-$ARCH-musl" ;;
  windows)
    [ "$ARCH" = x86_64 ] || die "Windows builds are x86_64 only for now; yours is $ARCH"
    ASSET="foton-windows-x86_64.exe"
    BIN=foton.exe
    ;;
  *) ASSET="foton-macos-$ARCH" ;;
esac

TMP_META=$(mktemp)
TMP=""
# One trap for the whole script: a second `trap ... EXIT` would replace this
# one rather than run alongside it, and the first temporary file would leak.
trap 'rm -f "$TMP_META"; [ -n "$TMP" ] && rm -rf "$TMP"' EXIT

bold "Foton installer"
printf 'Looking up the latest release...\n'
# 404 here means the project has no release yet, which is a different
# problem from a network failure and deserves a different sentence.
HTTP=$(curl -sSL -o "$TMP_META" -w '%{http_code}' "$API" 2>/dev/null) || HTTP=000
case "$HTTP" in
  200) ;;
  404) die "Foton has no published release yet. Build from source instead: https://github.com/$REPO" ;;
  000) die "could not reach the GitHub API -- check the network and try again" ;;
  *)   die "the GitHub API answered $HTTP; try again in a moment" ;;
esac
META=$(cat "$TMP_META")
TAG=$(printf '%s' "$META" | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -1)
[ -n "$TAG" ] || die "no published release yet"
BASE="https://github.com/$REPO/releases/download/$TAG"

printf 'Latest release: %s\n' "$TAG"
printf 'Asset for this machine: %s\n' "$ASSET"

if [ "$UPDATE" -eq 1 ]; then
  [ -x "./$BIN" ] || die "--update must run inside an existing installation"
  CURRENT=$("./$BIN" --version 2>/dev/null | awk '{print $2}')
  if [ "v$CURRENT" = "$TAG" ]; then
    bold "Already on $TAG. Nothing to do."
    exit 0
  fi
  printf 'Updating from %s to %s\n' "$CURRENT" "$TAG"
  DIR=.
else
  DIR=.
  if [ -e "$DIR/$BIN" ]; then
    OVERWRITE=$(ask "This directory already has Foton. Replace the binary?" "no")
    case "$OVERWRITE" in y|Y|yes|Yes) ;; *) die "stopping, nothing was changed" ;; esac
  fi
fi

TMP=$(mktemp -d)

printf 'Downloading...\n'
curl -fsSL "$BASE/$ASSET" -o "$TMP/$BIN" || die "could not download $ASSET from $TAG"
curl -fsSL "$BASE/SHA256SUMS" -o "$TMP/SHA256SUMS" || die "could not download SHA256SUMS"

printf 'Verifying...\n'
EXPECTED=$(grep " $ASSET\$" "$TMP/SHA256SUMS" | awk '{print $1}')
[ -n "$EXPECTED" ] || die "$ASSET is not listed in SHA256SUMS"
if command -v shasum >/dev/null 2>&1; then
  ACTUAL=$(shasum -a 256 "$TMP/$BIN" | awk '{print $1}')
else
  ACTUAL=$(sha256sum "$TMP/$BIN" | awk '{print $1}')
fi
if [ "$EXPECTED" != "$ACTUAL" ]; then
  rm -f "$TMP/$BIN"
  die "checksum mismatch -- the download does not match the published release"
fi

# chmod is meaningless on a Windows filesystem but harmless there, so it runs
# unconditionally rather than behind an OS check.
[ -f "$DIR/$BIN" ] && mv "$DIR/$BIN" "$DIR/$BIN.previous"
mv "$TMP/$BIN" "$DIR/$BIN"
chmod +x "$DIR/$BIN"
bold "Installed $TAG to ./$BIN"

if [ "$UPDATE" -eq 1 ]; then
  bold "Updated. Your config/ and saves/ were left alone."
  exit 0
fi

printf 'Writing the default configuration...\n'
( cd "$DIR" && "./$BIN" --generate-config ) || die "could not generate the configuration"

if [ "$have_tty" -eq 0 ]; then
  bold "No terminal here, so the defaults were kept. Edit ./config/ to change them."
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
printf 'Start it with:  ./%s\n' "$BIN"
START=$(ask "Start it now?" "yes")
# The server reads its console from standard input, which under `curl | sh`
# is the pipe curl is writing into -- already at end of file. Handing it the
# terminal is what makes the console usable; without this the server starts
# and ignores every command typed at it.
case "$START" in
  y|Y|yes|Yes)
    cd "$DIR" || exit 1
    exec "./$BIN" < /dev/tty
    ;;
esac
