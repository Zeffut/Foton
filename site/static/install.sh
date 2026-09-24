#!/bin/sh
# Install or update Foton.
#
#   curl -fsSL https://foton.zeffut.fr/install.sh | sh
#   curl -fsSL https://foton.zeffut.fr/install.sh | sh -s -- --update
#
# Prompts read /dev/tty, not standard input: under `curl | sh` standard input
# is this script's own text, and a read there answers questions with source
# code. With no terminal the defaults are taken and said out loud. Automated
# callers may explicitly set FOTON_INSTALL_NONINTERACTIVE=1 to take defaults
# even when their runner allocated a pseudo-terminal.
set -eu

REPO=Zeffut/Foton
API="https://api.github.com/repos/$REPO/releases/latest"
UPDATE=0
[ "${1:-}" = "--update" ] && UPDATE=1

# The installed binary's name -- foton.exe on Windows, foton everywhere else.
# Every reference to it below goes through this variable.
BIN=foton
RUNTIME_ASSET=foton-plugin-runtime.tar.gz

red() { printf '\033[31m%s\033[0m\n' "$1" >&2; }
bold() { printf '\033[1m%s\033[0m\n' "$1"; }
die() { red "error: $1"; exit 1; }

case ${FOTON_INSTALL_NONINTERACTIVE-} in
  ''|0) noninteractive=0 ;;
  1) noninteractive=1 ;;
  *) die "FOTON_INSTALL_NONINTERACTIVE must be 0 or 1" ;;
esac

# /dev/tty can exist and still not be openable -- a detached session, a cron
# job, a container without a terminal. `[ -r /dev/tty ]` says yes there and
# the open then fails, so the test is an actual open, not a permission check.
have_tty=0
if [ "$noninteractive" -eq 0 ] && (exec 3< /dev/tty) 2>/dev/null; then
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
need tar

hash_file() {
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  elif command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    die "shasum or sha256sum is required and was not found"
  fi
}

runtime_library_is_expected() {
  case "$1" in
    adventure-api-5.2.0.jar|adventure-key-5.2.0.jar|\
    adventure-text-logger-slf4j-5.2.0.jar|adventure-text-serializer-plain-5.2.0.jar|\
    annotations-26.1.0.jar|brigadier-1.3.10.jar|error_prone_annotations-2.47.0.jar|\
    failureaccess-1.0.3.jar|gson-2.14.0.jar|guava-33.6.0-jre.jar|\
    j2objc-annotations-3.1.jar|joml-1.10.8.jar|jspecify-1.0.0.jar|\
    kotlin-stdlib-1.8.20.jar|kotlin-stdlib-common-1.8.20.jar|\
    kotlin-stdlib-jdk7-1.8.20.jar|kotlin-stdlib-jdk8-1.8.20.jar|\
    netty-buffer-4.2.15.Final.jar|netty-codec-base-4.2.15.Final.jar|\
    netty-common-4.2.15.Final.jar|netty-resolver-4.2.15.Final.jar|\
    netty-transport-4.2.15.Final.jar|\
    slf4j-api-2.0.17.jar|snakeyaml-2.2.jar) return 0 ;;
    *) return 1 ;;
  esac
}

runtime_license_is_expected() {
  case "$1" in
    ADVENTURE-MIT.txt|APACHE-2.0.txt|BRIGADIER-MIT.txt|JOML-MIT.txt|\
    SLF4J-MIT.txt|THIRD-PARTY-NOTICES.txt) return 0 ;;
    *) return 1 ;;
  esac
}

runtime_manifest_path_is_expected() {
  case "$1" in
    foton-plugin-api.jar) return 0 ;;
    lib/*.jar) runtime_library_is_expected "${1#lib/}" ;;
    licenses/*) runtime_license_is_expected "${1#licenses/}" ;;
    *) return 1 ;;
  esac
}

# Inspect names and entry types before extraction. Validation after extraction
# is too late for an archive whose .release-tag is a link outside the temporary
# directory: the installer itself writes that path before starting the binary.
runtime_archive_is_safe() {
  runtime_archive=$1
  runtime_archive_list=$2
  runtime_archive_verbose=$3
  tar -tzf "$runtime_archive" > "$runtime_archive_list" 2>/dev/null || return 1
  tar -tvzf "$runtime_archive" > "$runtime_archive_verbose" 2>/dev/null || return 1

  while IFS= read -r runtime_verbose_line; do
    case "$runtime_verbose_line" in
      [-d]*) ;;
      *) return 1 ;;
    esac
  done < "$runtime_archive_verbose"

  runtime_archive_seen=''
  runtime_archive_files=0
  runtime_archive_directories=0
  while IFS= read -r runtime_member; do
    runtime_member=${runtime_member#./}
    [ -n "$runtime_member" ] || {
      runtime_archive_directories=$((runtime_archive_directories + 1))
      continue
    }
    printf '%s\n' "$runtime_archive_seen" | grep -Fqx "$runtime_member" && return 1
    runtime_archive_seen=$(printf '%s\n%s' "$runtime_archive_seen" "$runtime_member")
    case "$runtime_member" in
      lib/|licenses/) runtime_archive_directories=$((runtime_archive_directories + 1)) ;;
      SHA256SUMS|foton-plugin-api.jar) runtime_archive_files=$((runtime_archive_files + 1)) ;;
      lib/*.jar)
        runtime_library_is_expected "${runtime_member#lib/}" || return 1
        runtime_archive_files=$((runtime_archive_files + 1))
        ;;
      licenses/*)
        runtime_license_is_expected "${runtime_member#licenses/}" || return 1
        runtime_archive_files=$((runtime_archive_files + 1))
        ;;
      *) return 1 ;;
    esac
  done < "$runtime_archive_list"
  [ "$runtime_archive_files" -eq 32 ] && [ "$runtime_archive_directories" -eq 3 ]
}

runtime_is_complete() {
  runtime_dir=$1
  runtime_tag=$2
  runtime_require_tag=${3:-1}
  [ -f "$runtime_dir/foton-plugin-api.jar" ] && [ ! -L "$runtime_dir/foton-plugin-api.jar" ] || return 1
  [ -d "$runtime_dir/lib" ] && [ ! -L "$runtime_dir/lib" ] || return 1
  [ -d "$runtime_dir/licenses" ] && [ ! -L "$runtime_dir/licenses" ] || return 1
  [ -f "$runtime_dir/SHA256SUMS" ] && [ ! -L "$runtime_dir/SHA256SUMS" ] || return 1
  if [ "$runtime_require_tag" -eq 1 ]; then
    [ -f "$runtime_dir/.release-tag" ] && [ ! -L "$runtime_dir/.release-tag" ] || return 1
    [ "$(cat "$runtime_dir/.release-tag")" = "$runtime_tag" ] || return 1
    runtime_expected_root_entries=5
  else
    [ ! -e "$runtime_dir/.release-tag" ] && [ ! -L "$runtime_dir/.release-tag" ] || return 1
    runtime_expected_root_entries=4
  fi
  runtime_root_entries=0
  for runtime_entry in "$runtime_dir"/* "$runtime_dir"/.[!.]* "$runtime_dir"/..?*; do
    [ -e "$runtime_entry" ] || [ -L "$runtime_entry" ] || continue
    runtime_root_entries=$((runtime_root_entries + 1))
  done
  [ "$runtime_root_entries" -eq "$runtime_expected_root_entries" ] || return 1
  for runtime_entry in "$runtime_dir/lib"/* "$runtime_dir/lib"/.[!.]* \
    "$runtime_dir/lib"/..?* "$runtime_dir/licenses"/* \
    "$runtime_dir/licenses"/.[!.]* "$runtime_dir/licenses"/..?*; do
    [ -e "$runtime_entry" ] || [ -L "$runtime_entry" ] || continue
    [ -f "$runtime_entry" ] && [ ! -L "$runtime_entry" ] || return 1
  done

  runtime_api_seen=0
  runtime_libs_seen=0
  runtime_licenses_seen=0
  runtime_manifest_entries=0
  runtime_seen_paths=''
  while read -r runtime_expected runtime_path runtime_extra; do
    [ -n "$runtime_expected" ] || continue
    [ -z "${runtime_extra:-}" ] || return 1
    runtime_path=${runtime_path#\*}
    printf '%s\n' "$runtime_seen_paths" | grep -Fqx "$runtime_path" && return 1
    runtime_seen_paths=$(printf '%s\n%s' "$runtime_seen_paths" "$runtime_path")
    runtime_manifest_path_is_expected "$runtime_path" || return 1
    case "$runtime_path" in
      foton-plugin-api.jar) runtime_api_seen=1 ;;
      lib/*) runtime_libs_seen=$((runtime_libs_seen + 1)) ;;
      licenses/*) runtime_licenses_seen=$((runtime_licenses_seen + 1)) ;;
    esac
    [ -f "$runtime_dir/$runtime_path" ] && [ ! -L "$runtime_dir/$runtime_path" ] || return 1
    [ "$(hash_file "$runtime_dir/$runtime_path")" = "$runtime_expected" ] || return 1
    runtime_manifest_entries=$((runtime_manifest_entries + 1))
  done < "$runtime_dir/SHA256SUMS"
  runtime_actual_entries=$(find "$runtime_dir" -type f ! -name SHA256SUMS ! -name .release-tag | wc -l | tr -d ' ')
  runtime_actual_libs=$(find "$runtime_dir/lib" -type f -name '*.jar' | wc -l | tr -d ' ')
  runtime_all_lib_files=$(find "$runtime_dir/lib" -type f | wc -l | tr -d ' ')
  runtime_license_files=$(find "$runtime_dir/licenses" -type f | wc -l | tr -d ' ')
  for runtime_license in ADVENTURE-MIT.txt APACHE-2.0.txt BRIGADIER-MIT.txt \
    JOML-MIT.txt SLF4J-MIT.txt THIRD-PARTY-NOTICES.txt; do
    [ -f "$runtime_dir/licenses/$runtime_license" ] \
      && [ ! -L "$runtime_dir/licenses/$runtime_license" ] || return 1
  done
  [ "$runtime_api_seen" -eq 1 ] \
    && [ "$runtime_libs_seen" -eq 24 ] \
    && [ "$runtime_actual_libs" -eq 24 ] \
    && [ "$runtime_all_lib_files" -eq 24 ] \
    && [ "$runtime_licenses_seen" -eq 6 ] \
    && [ "$runtime_license_files" -eq 6 ] \
    && [ "$runtime_manifest_entries" -eq "$runtime_actual_entries" ]
}

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

DIR=.
TMP_META=$(mktemp)
TMP=""
STAGED=""
STAGED_RUNTIME=""
LOCK_DIR="$DIR/.foton-install.lock"
TRANSACTION_DIR="$DIR/.foton-install-transaction"
LOCK_HELD=0
LOCK_CLAIM=""
RECOVERY_DETAIL=""

direct_file() { [ -f "$1" ] && [ ! -L "$1" ]; }
direct_directory() { [ -d "$1" ] && [ ! -L "$1" ]; }

validate_transaction_journal() {
  direct_directory "$TRANSACTION_DIR" || {
    RECOVERY_DETAIL="$TRANSACTION_DIR is not a direct transaction directory"
    return 1
  }
  direct_file "$TRANSACTION_DIR/owner" || {
    RECOVERY_DETAIL="$TRANSACTION_DIR has no safe ownership marker"
    return 1
  }
  transaction_owner=$(cat "$TRANSACTION_DIR/owner" 2>/dev/null || printf '')
  printf '%s\n' "$transaction_owner" | grep -Eq '^[0-9]+:claim\.[A-Za-z0-9]+$' || {
    RECOVERY_DETAIL="$TRANSACTION_DIR has an invalid ownership marker"
    return 1
  }
  for transaction_entry in "$TRANSACTION_DIR"/* "$TRANSACTION_DIR"/.[!.]* "$TRANSACTION_DIR"/..?*; do
    [ -e "$transaction_entry" ] || [ -L "$transaction_entry" ] || continue
    transaction_name=${transaction_entry##*/}
    case "$transaction_name" in
      owner|committed|replace-binary-started|replace-runtime-started)
        direct_file "$transaction_entry" || { RECOVERY_DETAIL="unsafe journal marker $transaction_entry"; return 1; }
        [ "$transaction_name" = owner ] || [ ! -s "$transaction_entry" ] \
          || { RECOVERY_DETAIL="journal marker is not empty: $transaction_entry"; return 1; }
        ;;
      new-binary|previous-binary)
        direct_file "$transaction_entry" || { RECOVERY_DETAIL="unsafe journal binary $transaction_entry"; return 1; }
        ;;
      new-runtime|previous-runtime)
        direct_directory "$transaction_entry" || { RECOVERY_DETAIL="unsafe journal runtime $transaction_entry"; return 1; }
        if find "$transaction_entry" -type l -print -quit | grep -q . \
          || find "$transaction_entry" ! -type f ! -type d -print -quit | grep -q .; then
          RECOVERY_DETAIL="journal runtime contains a link or special file: $transaction_entry"
          return 1
        fi
        ;;
      *) RECOVERY_DETAIL="unexpected transaction entry $transaction_entry"; return 1 ;;
    esac
  done
}

validate_lock_directory() {
  direct_directory "$LOCK_DIR" || die "$LOCK_DIR is not a direct lock directory"
  for lock_entry in "$LOCK_DIR"/* "$LOCK_DIR"/.[!.]* "$LOCK_DIR"/..?*; do
    [ -e "$lock_entry" ] || [ -L "$lock_entry" ] || continue
    lock_name=${lock_entry##*/}
    printf '%s\n' "$lock_name" | grep -Eq '^claim\.[A-Za-z0-9]+$' \
      || die "unexpected installer lock entry $lock_entry"
    direct_directory "$lock_entry" || die "installer lock claim is not a direct directory: $lock_entry"
    for claim_entry in "$lock_entry"/* "$lock_entry"/.[!.]* "$lock_entry"/..?*; do
      [ -e "$claim_entry" ] || [ -L "$claim_entry" ] || continue
      claim_name=${claim_entry##*/}
      case "$claim_name" in owner|ticket|ticket.new) ;; *) die "unexpected lock claim entry $claim_entry" ;; esac
      direct_file "$claim_entry" || die "installer lock marker is not a direct file: $claim_entry"
      claim_value=$(cat "$claim_entry" 2>/dev/null || printf '')
      case "$claim_value" in ''|*[!0-9]*) die "invalid installer lock marker $claim_entry" ;; esac
    done
  done
}

recover_transaction() {
  [ -e "$TRANSACTION_DIR" ] || [ -L "$TRANSACTION_DIR" ] || return 0
  validate_transaction_journal || return 1
  if [ -f "$TRANSACTION_DIR/committed" ]; then
    rm -rf "$TRANSACTION_DIR" || {
      RECOVERY_DETAIL="could not remove committed transaction $TRANSACTION_DIR"
      return 1
    }
    return 0
  fi

  recovery_errors=""
  previous_runtime="$TRANSACTION_DIR/previous-runtime"
  previous_binary="$TRANSACTION_DIR/previous-binary"
  if [ -e "$previous_runtime" ] || [ -f "$TRANSACTION_DIR/replace-runtime-started" ]; then
    if [ -e "$DIR/plugin-runtime" ] && ! rm -rf "$DIR/plugin-runtime"; then
      recovery_errors="$recovery_errors current-runtime"
    fi
    if [ -e "$previous_runtime" ] && [ ! -e "$DIR/plugin-runtime" ] \
      && ! cp -Rp "$previous_runtime" "$DIR/plugin-runtime"; then
      recovery_errors="$recovery_errors runtime-backup=$previous_runtime"
    fi
  fi
  if [ -e "$previous_binary" ] || [ -f "$TRANSACTION_DIR/replace-binary-started" ]; then
    if [ -e "$DIR/$BIN" ] && ! rm -f "$DIR/$BIN"; then
      recovery_errors="$recovery_errors current-binary"
    fi
    if [ -e "$previous_binary" ] && [ ! -e "$DIR/$BIN" ] \
      && ! cp -p "$previous_binary" "$DIR/$BIN"; then
      recovery_errors="$recovery_errors binary-backup=$previous_binary"
    fi
  fi
  if [ -n "$recovery_errors" ]; then
    RECOVERY_DETAIL="rollback was incomplete:$recovery_errors"
    return 1
  fi
  if ! rm -rf "$TRANSACTION_DIR"; then
    RECOVERY_DETAIL="previous files were restored but $TRANSACTION_DIR could not be removed"
    return 1
  fi
  return 0
}

cleanup() {
  cleanup_status=$?
  trap - EXIT HUP INT TERM
  if [ "$LOCK_HELD" -eq 1 ] && [ -d "$TRANSACTION_DIR" ] \
    && [ ! -f "$TRANSACTION_DIR/committed" ]; then
    if ! recover_transaction; then
      red "error: interrupted installation; $RECOVERY_DETAIL"
      cleanup_status=1
    fi
  fi
  rm -f "$TMP_META"
  [ -n "$TMP" ] && rm -rf "$TMP"
  [ -n "$STAGED" ] && [ -e "$STAGED" ] && rm -f "$STAGED"
  [ -n "$STAGED_RUNTIME" ] && [ -e "$STAGED_RUNTIME" ] && rm -rf "$STAGED_RUNTIME"
  if [ -n "$LOCK_CLAIM" ] && [ -d "$LOCK_CLAIM" ] && [ ! -L "$LOCK_CLAIM" ]; then
    rm -rf "$LOCK_CLAIM" || red "warning: installer claim remains at $LOCK_CLAIM"
  fi
  # Removing an empty root is safe: rmdir cannot remove another owner's
  # non-empty lock. The unique claim is the only object an owner ever deletes.
  rmdir "$LOCK_DIR" 2>/dev/null || :
  exit "$cleanup_status"
}
trap cleanup EXIT
trap 'exit 130' HUP INT TERM

# Each invocation owns a uniquely-created claim. Stale cleanup can therefore
# never unlink a path that a newer process has acquired. Tickets plus the
# claim-name tie-break implement a small bakery lock using only POSIX-atomic
# mkdir/rename operations.
mkdir "$LOCK_DIR" 2>/dev/null || :
[ -d "$LOCK_DIR" ] && [ ! -L "$LOCK_DIR" ] \
  || die "$LOCK_DIR exists but is not a safe installer lock directory"
validate_lock_directory
LOCK_CLAIM=$(mktemp -d "$LOCK_DIR/claim.XXXXXX") \
  || die "could not create an installer lock claim in $LOCK_DIR"
printf '%s\n' "$$" > "$LOCK_CLAIM/owner"

lock_max_ticket=0
for lock_other in "$LOCK_DIR"/claim.*; do
  [ -d "$lock_other" ] && [ ! -L "$lock_other" ] || continue
  lock_ticket=$(cat "$lock_other/ticket" 2>/dev/null || printf '')
  case "$lock_ticket" in
    ''|*[!0-9]*) ;;
    *) [ "$lock_ticket" -le "$lock_max_ticket" ] || lock_max_ticket=$lock_ticket ;;
  esac
done
lock_ticket=$((lock_max_ticket + 1))
printf '%s\n' "$lock_ticket" > "$LOCK_CLAIM/ticket.new"
mv "$LOCK_CLAIM/ticket.new" "$LOCK_CLAIM/ticket"

# A claim without a ticket is still choosing. Give a concurrently starting
# owner time to publish, while collecting abandoned unique claims safely.
lock_round=0
while :; do
  lock_choosing=0
  for lock_other in "$LOCK_DIR"/claim.*; do
    [ "$lock_other" != "$LOCK_CLAIM" ] || continue
    [ -d "$lock_other" ] && [ ! -L "$lock_other" ] || continue
    [ -f "$lock_other/ticket" ] && [ ! -L "$lock_other/ticket" ] && continue
    lock_pid=$(cat "$lock_other/owner" 2>/dev/null || printf '')
    case "$lock_pid" in
      ''|*[!0-9]*) lock_alive=0 ;;
      *) if kill -0 "$lock_pid" 2>/dev/null; then lock_alive=1; else lock_alive=0; fi ;;
    esac
    if [ "$lock_alive" -eq 1 ]; then
      lock_choosing=1
    else
      rm -rf "$lock_other"
    fi
  done
  [ "$lock_choosing" -eq 0 ] && break
  lock_round=$((lock_round + 1))
  [ "$lock_round" -lt 3 ] || die "another installer is already choosing a lock for $DIR"
  sleep 1
done

for lock_other in "$LOCK_DIR"/claim.*; do
  [ "$lock_other" != "$LOCK_CLAIM" ] || continue
  [ -d "$lock_other" ] && [ ! -L "$lock_other" ] || continue
  lock_pid=$(cat "$lock_other/owner" 2>/dev/null || printf '')
  lock_other_ticket=$(cat "$lock_other/ticket" 2>/dev/null || printf '')
  case "$lock_pid:$lock_other_ticket" in
    *[!0-9:]*|:*|*:) lock_alive=0 ;;
    *) if kill -0 "$lock_pid" 2>/dev/null; then lock_alive=1; else lock_alive=0; fi ;;
  esac
  if [ "$lock_alive" -eq 0 ]; then
    rm -rf "$lock_other"
    continue
  fi
  lock_other_name=${lock_other##*/}
  lock_claim_name=${LOCK_CLAIM##*/}
  lock_first=$(printf '%s\n%s\n' "$lock_other_name" "$lock_claim_name" | LC_ALL=C sort | head -1)
  if [ "$lock_other_ticket" -lt "$lock_ticket" ] \
    || { [ "$lock_other_ticket" -eq "$lock_ticket" ] && [ "$lock_first" = "$lock_other_name" ]; }; then
    die "another installer is already modifying $DIR (pid $lock_pid)"
  fi
done
LOCK_HELD=1

if [ -e "$TRANSACTION_DIR" ] || [ -L "$TRANSACTION_DIR" ]; then
  recover_transaction || die "could not recover an interrupted installation: $RECOVERY_DETAIL"
  bold "Recovered an interrupted installation before continuing."
fi

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
  if [ "v$CURRENT" = "$TAG" ] && runtime_is_complete "./plugin-runtime" "$TAG"; then
    bold "Already on $TAG. Nothing to do."
    exit 0
  fi
  if [ "v$CURRENT" = "$TAG" ]; then
    printf 'Repairing the plugin runtime for %s\n' "$TAG"
  else
    printf 'Updating from %s to %s\n' "$CURRENT" "$TAG"
  fi
  DIR=.
else
  DIR=.
  if [ -e "$DIR/$BIN" ] || [ -L "$DIR/$BIN" ] \
    || [ -e "$DIR/plugin-runtime" ] || [ -L "$DIR/plugin-runtime" ]; then
    OVERWRITE=$(ask "This directory already has a Foton binary or plugin runtime. Replace the binary/runtime pair? Config, plugins and saves stay untouched." "no")
    case "$OVERWRITE" in y|Y|yes|Yes) ;; *) die "stopping, nothing was changed" ;; esac
  fi
fi

TMP=$(mktemp -d)

printf 'Downloading...\n'
curl -fsSL "$BASE/$ASSET" -o "$TMP/$BIN" || die "could not download $ASSET from $TAG"
curl -fsSL "$BASE/$RUNTIME_ASSET" -o "$TMP/$RUNTIME_ASSET" || die "could not download $RUNTIME_ASSET from $TAG"
curl -fsSL "$BASE/SHA256SUMS" -o "$TMP/SHA256SUMS" || die "could not download SHA256SUMS"

printf 'Verifying...\n'
verify_download() {
  expected=$(awk -v name="$2" '$2 == name || $2 == "*" name { print $1; exit }' "$TMP/SHA256SUMS")
  [ -n "$expected" ] || die "$2 is not listed in SHA256SUMS"
  actual=$(hash_file "$1")
  if [ "$expected" != "$actual" ]; then
    rm -f "$1"
    die "checksum mismatch for $2 -- the download does not match the published release"
  fi
}
verify_download "$TMP/$BIN" "$ASSET"
verify_download "$TMP/$RUNTIME_ASSET" "$RUNTIME_ASSET"

mkdir "$TMP/plugin-runtime"
runtime_archive_is_safe "$TMP/$RUNTIME_ASSET" "$TMP/runtime-members" "$TMP/runtime-members.verbose" \
  || die "$RUNTIME_ASSET contains an unsafe or unexpected archive entry"
tar -xzf "$TMP/$RUNTIME_ASSET" -C "$TMP/plugin-runtime" || die "could not unpack $RUNTIME_ASSET"
[ -f "$TMP/plugin-runtime/foton-plugin-api.jar" ] || die "$RUNTIME_ASSET has no plugin API jar"
[ -d "$TMP/plugin-runtime/lib" ] || die "$RUNTIME_ASSET has no runtime library directory"
runtime_is_complete "$TMP/plugin-runtime" "$TAG" 0 \
  || die "$RUNTIME_ASSET has an incomplete or invalid runtime manifest"
printf '%s\n' "$TAG" > "$TMP/plugin-runtime/.release-tag"
runtime_is_complete "$TMP/plugin-runtime" "$TAG" || die "$RUNTIME_ASSET has an incomplete or invalid runtime manifest"

# The fixed transaction directory is both the staging area and the recovery
# journal. Its path is deterministic so the next invocation can roll an
# interrupted update back before touching the network.
mkdir "$TRANSACTION_DIR" || die "could not create transaction journal $TRANSACTION_DIR"
printf '%s:%s\n' "$$" "${LOCK_CLAIM##*/}" > "$TRANSACTION_DIR/owner"
STAGED="$TRANSACTION_DIR/new-binary"
mv "$TMP/$BIN" "$STAGED" || die "could not stage the downloaded binary"
chmod +x "$STAGED"
STAGED_RUNTIME="$TRANSACTION_DIR/new-runtime"
mkdir "$STAGED_RUNTIME"
cp -R "$TMP/plugin-runtime/." "$STAGED_RUNTIME/" || die "could not stage the plugin runtime"

rollback_installation() {
  rollback_reason=$1
  if [ -e "$TRANSACTION_DIR/previous-binary" ] \
    || [ -e "$TRANSACTION_DIR/previous-runtime" ]; then
    rollback_had_previous=1
  else
    rollback_had_previous=0
  fi
  if ! recover_transaction; then
    die "$rollback_reason; $RECOVERY_DETAIL"
  fi
  if [ "$rollback_had_previous" -eq 1 ]; then
    die "$rollback_reason; the previous installation was restored"
  fi
  die "$rollback_reason; the incomplete new installation was removed"
}
if [ -e "$DIR/$BIN" ] || [ -L "$DIR/$BIN" ]; then
  direct_file "$DIR/$BIN" || die "existing binary is not a direct regular file: $DIR/$BIN"
  mv "$DIR/$BIN" "$TRANSACTION_DIR/previous-binary" \
    || die "could not stage the existing binary for replacement"
fi
if [ -e "$DIR/plugin-runtime" ] || [ -L "$DIR/plugin-runtime" ]; then
  direct_directory "$DIR/plugin-runtime" \
    || rollback_installation "existing plugin runtime is not a direct directory"
  if ! mv "$DIR/plugin-runtime" "$TRANSACTION_DIR/previous-runtime"; then
    rollback_installation "could not stage the existing plugin runtime for replacement"
  fi
fi
: > "$TRANSACTION_DIR/replace-binary-started"
if ! mv "$STAGED" "$DIR/$BIN"; then
  rollback_installation "could not replace the existing binary"
fi
STAGED=""
: > "$TRANSACTION_DIR/replace-runtime-started"
if ! mv "$STAGED_RUNTIME" "$DIR/plugin-runtime"; then
  rollback_installation "could not replace the plugin runtime"
fi
STAGED_RUNTIME=""

INSTALLED_VERSION=$("$DIR/$BIN" --version 2>/dev/null | awk '{print $2}') \
  || rollback_installation "the new binary failed its version check"
[ "v$INSTALLED_VERSION" = "$TAG" ] \
  || rollback_installation "the new binary reports version $INSTALLED_VERSION instead of ${TAG#v}"

: > "$TRANSACTION_DIR/committed"
if ! rm -rf "$TRANSACTION_DIR"; then
  red "warning: committed transaction cleanup remains at $TRANSACTION_DIR"
fi
bold "Installed $TAG to ./$BIN with the optional plugin runtime in ./plugin-runtime/"

if [ "$UPDATE" -eq 1 ]; then
  bold "Updated. Your config/, plugins/ and saves/ were left alone."
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

# These values are written into TOML, not passed to a shell.  Keep the small
# interactive surface strict so malformed input cannot leave a server that
# immediately fails to start.  The forbidden punctuation would also be
# significant to sed's replacement syntax below.
newline='
'
case "$NAME" in
  ''|*'"'*|*'\\'*|*'&'*|*'|'*|*"$newline"*) die 'server name cannot contain quotes, backslashes, &, |, or line breaks' ;;
esac
case "$PORT" in ''|*[!0-9]*) die 'port must be a number from 1 to 65000' ;; esac
[ "${#PORT}" -le 5 ] && [ "$PORT" -ge 1 ] && [ "$PORT" -le 65000 ] || die 'port must be a number from 1 to 65000'
case "$PLAYERS" in ''|*[!0-9]*) die 'maximum players must be a number from 1 to 2147483647' ;; esac
[ "${#PLAYERS}" -le 10 ] && [ "$PLAYERS" -ge 1 ] && [ "$PLAYERS" -le 2147483647 ] || die 'maximum players must be a number from 1 to 2147483647'
case "$DIFFICULTY" in peaceful|easy|normal|hard) ;; *) die 'difficulty must be peaceful, easy, normal, or hard' ;; esac

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
