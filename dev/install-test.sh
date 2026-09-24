#!/usr/bin/env bash
# Hermetic transaction checks for the POSIX installer.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/foton-install-test.XXXXXX")"
cleanup() {
  status=$?
  if [ "$status" -ne 0 ]; then
    for log in "$SCRATCH"/*.log; do
      [ -f "$log" ] && { printf '\n--- %s ---\n' "$(basename "$log")" >&2; cat "$log" >&2; }
    done
  fi
  rm -rf "$SCRATCH"
  exit "$status"
}
trap cleanup EXIT

ASSETS="$SCRATCH/assets"
MOCK_BIN="$SCRATCH/bin"
INSTALL="$SCRATCH/install"
mkdir -p "$ASSETS/runtime/lib" "$MOCK_BIN" "$INSTALL/config" "$INSTALL/plugins" "$INSTALL/saves"

write_binary() {
  path=$1
  version=$2
  cat > "$path" <<EOF
#!/bin/sh
case "\${1:-}" in
  --version) printf 'foton $version\n' ;;
  --generate-config) mkdir -p config; exit 0 ;;
  *) exit 0 ;;
esac
EOF
  chmod +x "$path"
}

runtime_libraries() {
  cat <<'EOF'
adventure-api-5.2.0.jar
adventure-key-5.2.0.jar
adventure-text-logger-slf4j-5.2.0.jar
adventure-text-serializer-plain-5.2.0.jar
annotations-26.1.0.jar
brigadier-1.3.10.jar
error_prone_annotations-2.47.0.jar
failureaccess-1.0.3.jar
gson-2.14.0.jar
guava-33.6.0-jre.jar
j2objc-annotations-3.1.jar
joml-1.10.8.jar
jspecify-1.0.0.jar
kotlin-stdlib-1.8.20.jar
kotlin-stdlib-common-1.8.20.jar
kotlin-stdlib-jdk7-1.8.20.jar
kotlin-stdlib-jdk8-1.8.20.jar
netty-buffer-4.2.15.Final.jar
netty-codec-base-4.2.15.Final.jar
netty-common-4.2.15.Final.jar
netty-resolver-4.2.15.Final.jar
netty-transport-4.2.15.Final.jar
slf4j-api-2.0.17.jar
snakeyaml-2.2.jar
EOF
}

write_runtime() {
  directory=$1
  tag=$2
  marker=$3
  installed=${4:-0}
  rm -rf "$directory"
  mkdir -p "$directory/lib" "$directory/licenses"
  printf 'api %s\n' "$marker" > "$directory/foton-plugin-api.jar"
  for library in $(runtime_libraries); do
    printf 'library %s %s\n' "$marker" "$library" > "$directory/lib/$library"
  done
  for license in ADVENTURE-MIT.txt APACHE-2.0.txt BRIGADIER-MIT.txt JOML-MIT.txt \
    SLF4J-MIT.txt THIRD-PARTY-NOTICES.txt; do
    printf 'license fixture %s\n' "$license" > "$directory/licenses/$license"
  done
  (
    cd "$directory"
    sha256sum foton-plugin-api.jar lib/*.jar licenses/*.txt > SHA256SUMS
  )
  if [ "$installed" -eq 1 ]; then
    printf '%s\n' "$tag" > "$directory/.release-tag"
  fi
}

package_release() {
  rm -f "$ASSETS/foton-plugin-runtime.tar.gz" "$ASSETS/SHA256SUMS"
  tar -czf "$ASSETS/foton-plugin-runtime.tar.gz" -C "$ASSETS/runtime" .
  (
    cd "$ASSETS"
    printf '%s  foton-linux-x86_64-musl\n' "$(sha256sum foton | awk '{print $1}')" > SHA256SUMS
    printf '%s  foton-plugin-runtime.tar.gz\n' "$(sha256sum foton-plugin-runtime.tar.gz | awk '{print $1}')" >> SHA256SUMS
  )
}

cat > "$MOCK_BIN/uname" <<'EOF'
#!/bin/sh
case "${1:-}" in
  -s) printf 'Linux\n' ;;
  -m) printf 'x86_64\n' ;;
  *) printf 'Linux\n' ;;
esac
EOF

cat > "$MOCK_BIN/curl" <<'EOF'
#!/bin/sh
out=""
url=""
previous=""
for argument in "$@"; do
  if [ "$previous" = output ]; then out=$argument; previous=""; continue; fi
  case "$argument" in
    -o) previous=output ;;
    http://*|https://*) url=$argument ;;
  esac
done
printf '%s\n' "$url" >> "$FOTON_INSTALL_CURL_LOG"
case "$url" in
  */releases/latest)
    printf '{"tag_name":"v9.8.7"}\n' > "$out"
    printf '200'
    ;;
  */foton-linux-x86_64-musl) cp "$FOTON_INSTALL_ASSETS/foton" "$out" ;;
  */foton-plugin-runtime.tar.gz) cp "$FOTON_INSTALL_ASSETS/foton-plugin-runtime.tar.gz" "$out" ;;
  */SHA256SUMS) cp "$FOTON_INSTALL_ASSETS/SHA256SUMS" "$out" ;;
  *) exit 22 ;;
esac
EOF
cat > "$MOCK_BIN/mv" <<'EOF'
#!/bin/sh
if [ "${FOTON_INSTALL_INTERRUPT:-0}" = 1 ]; then
  case "${1:-}" in
    */new-binary)
      /bin/mv "$@" || exit $?
      kill -TERM "$PPID"
      sleep 1
      exit 0
      ;;
  esac
fi
exec /bin/mv "$@"
EOF
cat > "$MOCK_BIN/cp" <<'EOF'
#!/bin/sh
if [ "${FOTON_INSTALL_FAIL_RESTORE:-0}" = 1 ]; then
  case "${2:-}" in */previous-binary) exit 73 ;; esac
fi
exec /usr/bin/cp "$@"
EOF
chmod +x "$MOCK_BIN/uname" "$MOCK_BIN/curl" "$MOCK_BIN/mv" "$MOCK_BIN/cp"

export FOTON_INSTALL_ASSETS="$ASSETS"
export FOTON_INSTALL_CURL_LOG="$SCRATCH/curl.log"
export PATH="$MOCK_BIN:$PATH"

# Automation must opt out explicitly when its runner owns a pseudo-terminal.
# Reject misspellings instead of silently changing prompt behavior.
: > "$FOTON_INSTALL_CURL_LOG"
if FOTON_INSTALL_NONINTERACTIVE=yes bash "$REPO/site/static/install.sh" --update \
  > "$SCRATCH/noninteractive-value.log" 2>&1; then
  echo 'installer accepted an invalid non-interactive mode' >&2
  exit 1
fi
grep -q 'FOTON_INSTALL_NONINTERACTIVE must be 0 or 1' "$SCRATCH/noninteractive-value.log"
[ ! -s "$FOTON_INSTALL_CURL_LOG" ]
export FOTON_INSTALL_NONINTERACTIVE=1

# A matching binary with a missing runtime is not "already current": update
# repairs the complete, checksummed runtime and preserves user directories.
write_binary "$INSTALL/foton" 9.8.7
printf 'keep config\n' > "$INSTALL/config/sentinel"
printf 'keep plugin\n' > "$INSTALL/plugins/sentinel"
printf 'keep save\n' > "$INSTALL/saves/sentinel"
mkdir -p "$INSTALL/plugin-runtime"
printf 'incomplete\n' > "$INSTALL/plugin-runtime/foton-plugin-api.jar"
write_binary "$ASSETS/foton" 9.8.7
write_runtime "$ASSETS/runtime" v9.8.7 new
package_release
(
  cd "$INSTALL"
  bash "$REPO/site/static/install.sh" --update > "$SCRATCH/repair.log" 2>&1
)
grep -q 'Repairing the plugin runtime' "$SCRATCH/repair.log"
grep -q '^new adventure-api-5.2.0.jar$' <(sed -n 's/^library //p' "$INSTALL/plugin-runtime/lib/adventure-api-5.2.0.jar")
grep -q '^new netty-codec-base-4.2.15.Final.jar$' \
  <(sed -n 's/^library //p' "$INSTALL/plugin-runtime/lib/netty-codec-base-4.2.15.Final.jar")
grep -qx 'v9.8.7' "$INSTALL/plugin-runtime/.release-tag"
grep -qx 'keep config' "$INSTALL/config/sentinel"
grep -qx 'keep plugin' "$INSTALL/plugins/sentinel"
grep -qx 'keep save' "$INSTALL/saves/sentinel"

# A complete runtime really is a no-op: only release metadata is fetched.
: > "$FOTON_INSTALL_CURL_LOG"
(
  cd "$INSTALL"
  bash "$REPO/site/static/install.sh" --update > "$SCRATCH/current.log" 2>&1
)
grep -q 'Already on v9.8.7' "$SCRATCH/current.log"
[ "$(wc -l < "$FOTON_INSTALL_CURL_LOG")" -eq 1 ]

# Pre-existing control paths are untrusted. Links must be rejected before
# recovery and must never make cleanup remove content outside the destination.
OUTSIDE="$SCRATCH/outside-control-target"
mkdir "$OUTSIDE"
printf 'outside stays\n' > "$OUTSIDE/sentinel"
ln -s "$OUTSIDE" "$INSTALL/.foton-install-transaction"
if (
  cd "$INSTALL"
  bash "$REPO/site/static/install.sh" --update > "$SCRATCH/journal-symlink.log" 2>&1
); then
  echo 'installer accepted a symlink transaction journal' >&2
  exit 1
fi
grep -qx 'outside stays' "$OUTSIDE/sentinel"
[ -e "$INSTALL/.foton-install-transaction" ] || [ -L "$INSTALL/.foton-install-transaction" ]
rm -rf "$INSTALL/.foton-install-transaction"

ln -s "$OUTSIDE" "$INSTALL/.foton-install.lock"
if (
  cd "$INSTALL"
  bash "$REPO/site/static/install.sh" --update > "$SCRATCH/lock-symlink.log" 2>&1
); then
  echo 'installer accepted a symlink lock root' >&2
  exit 1
fi
grep -qx 'outside stays' "$OUTSIDE/sentinel"
[ -e "$INSTALL/.foton-install.lock" ] || [ -L "$INSTALL/.foton-install.lock" ]
rm -rf "$INSTALL/.foton-install.lock"

mkdir "$INSTALL/.foton-install-transaction"
printf 'not-an-owner\n' > "$INSTALL/.foton-install-transaction/owner"
if (
  cd "$INSTALL"
  bash "$REPO/site/static/install.sh" --update > "$SCRATCH/journal-owner.log" 2>&1
); then
  echo 'installer accepted an invalid transaction ownership marker' >&2
  exit 1
fi
grep -q 'invalid ownership marker' "$SCRATCH/journal-owner.log"
rm -rf "$INSTALL/.foton-install-transaction"

# A runtime-only collision is still an existing installation surface. Fresh
# install requires explicit consent for replacing the binary/runtime pair and
# preserves unrelated data when the non-interactive default declines.
COLLISION="$SCRATCH/runtime-only-collision"
mkdir -p "$COLLISION/plugin-runtime" "$COLLISION/saves"
printf 'keep runtime\n' > "$COLLISION/plugin-runtime/sentinel"
printf 'keep save\n' > "$COLLISION/saves/sentinel"
: > "$FOTON_INSTALL_CURL_LOG"
if command -v script >/dev/null 2>&1 && command -v timeout >/dev/null 2>&1; then
  set +e
  (
    cd "$COLLISION"
    # `yes` would authorize replacement if the installer touched /dev/tty.
    # Explicit non-interactive mode must ignore it and take the safe "no".
    printf 'yes\n' | timeout 10 script -qec \
      "bash \"$REPO/site/static/install.sh\" > \"$SCRATCH/collision.log\" 2>&1" \
      /dev/null >/dev/null
  )
  collision_status=$?
  set -e
  [ "$collision_status" -ne 124 ] || {
    echo 'non-interactive installer tried to read from its pseudo-terminal' >&2
    exit 1
  }
else
  set +e
  (
    cd "$COLLISION"
    bash "$REPO/site/static/install.sh" > "$SCRATCH/collision.log" 2>&1
  )
  collision_status=$?
  set -e
fi
if [ "$collision_status" -eq 0 ]; then
  echo 'fresh installer replaced a runtime-only collision without consent' >&2
  exit 1
fi
grep -q 'stopping, nothing was changed' "$SCRATCH/collision.log"
grep -qx 'keep runtime' "$COLLISION/plugin-runtime/sentinel"
grep -qx 'keep save' "$COLLISION/saves/sentinel"
[ ! -e "$COLLISION/foton" ]
[ "$(wc -l < "$FOTON_INSTALL_CURL_LOG")" -eq 1 ]

# A checksum file that names only part of an otherwise plausible runtime is
# rejected before the installed pair is moved. The outer archive checksum is
# regenerated so this specifically exercises the internal-manifest invariant.
write_binary "$INSTALL/foton" 9.8.6
write_runtime "$INSTALL/plugin-runtime" v9.8.6 old 1
write_runtime "$ASSETS/runtime" v9.8.7 truncated
sed -i '$d' "$ASSETS/runtime/SHA256SUMS"
package_release
if (
  cd "$INSTALL"
  bash "$REPO/site/static/install.sh" --update > "$SCRATCH/truncated.log" 2>&1
); then
  echo 'installer accepted a truncated runtime manifest' >&2
  exit 1
fi
grep -q 'incomplete or invalid runtime manifest' "$SCRATCH/truncated.log"
[ "$("$INSTALL/foton" --version)" = 'foton 9.8.6' ]
grep -q '^library old adventure-api-5.2.0.jar$' "$INSTALL/plugin-runtime/lib/adventure-api-5.2.0.jar"

# A self-consistent but shortened bundle is also invalid: the supported host
# runtime is the API plus exactly twenty-four dependency jars.
write_runtime "$ASSETS/runtime" v9.8.7 shortened
rm "$ASSETS/runtime/lib/netty-codec-base-4.2.15.Final.jar"
sed -i '/lib\/netty-codec-base-4\.2\.15\.Final\.jar$/d' "$ASSETS/runtime/SHA256SUMS"
package_release
if (
  cd "$INSTALL"
  bash "$REPO/site/static/install.sh" --update > "$SCRATCH/shortened.log" 2>&1
); then
  echo 'installer accepted a runtime with only twenty-three dependency jars' >&2
  exit 1
fi
grep -q 'unsafe or unexpected archive entry' "$SCRATCH/shortened.log"
[ "$("$INSTALL/foton" --version)" = 'foton 9.8.6' ]

# Required license filenames are part of the bundle contract. A publisher
# cannot silently omit one while keeping a self-consistent checksum manifest.
write_runtime "$ASSETS/runtime" v9.8.7 missing-license
rm "$ASSETS/runtime/licenses/SLF4J-MIT.txt"
sed -i '/licenses\/SLF4J-MIT\.txt$/d' "$ASSETS/runtime/SHA256SUMS"
package_release
if (
  cd "$INSTALL"
  bash "$REPO/site/static/install.sh" --update > "$SCRATCH/missing-license.log" 2>&1
); then
  echo 'installer accepted a runtime missing an exact license input' >&2
  exit 1
fi
[ "$("$INSTALL/foton" --version)" = 'foton 9.8.6' ]

# A directory or symbolic link whose name ends in .jar is not a jar and must
# never enter the JVM classpath, even when the archive checksum is valid.
write_runtime "$ASSETS/runtime" v9.8.7 special-jar
mkdir "$ASSETS/runtime/lib/directory.jar"
package_release
if (
  cd "$INSTALL"
  bash "$REPO/site/static/install.sh" --update > "$SCRATCH/special-jar.log" 2>&1
); then
  echo 'installer accepted a special filesystem entry named as a jar' >&2
  exit 1
fi

# The count alone is not the runtime contract. A self-consistent archive with
# twenty-four jars but one unexpected name must be rejected before replacement.
write_runtime "$ASSETS/runtime" v9.8.7 renamed-jar
mv "$ASSETS/runtime/lib/failureaccess-1.0.3.jar" "$ASSETS/runtime/lib/unexpected-1.0.jar"
(
  cd "$ASSETS/runtime"
  sha256sum foton-plugin-api.jar lib/*.jar licenses/*.txt > SHA256SUMS
)
package_release
if (
  cd "$INSTALL"
  bash "$REPO/site/static/install.sh" --update > "$SCRATCH/renamed-jar.log" 2>&1
); then
  echo 'installer accepted an unexpected runtime jar name' >&2
  exit 1
fi
[ "$("$INSTALL/foton" --version)" = 'foton 9.8.6' ]

# A release archive must not be able to make the installer follow its own
# .release-tag write outside the extraction directory.
write_runtime "$ASSETS/runtime" v9.8.7 linked-tag
ARCHIVE_VICTIM="$SCRATCH/archive-victim"
printf 'outside stays\n' > "$ARCHIVE_VICTIM"
ln -s ../../archive-victim "$ASSETS/runtime/.release-tag"
package_release
if (
  cd "$INSTALL"
  bash "$REPO/site/static/install.sh" --update > "$SCRATCH/archive-link.log" 2>&1
); then
  echo 'installer accepted a linked release-tag archive entry' >&2
  exit 1
fi
grep -qx 'outside stays' "$ARCHIVE_VICTIM"
[ "$("$INSTALL/foton" --version)" = 'foton 9.8.6' ]

if ln -s adventure-api-5.2.0.jar "$ASSETS/runtime/lib/linked.jar" 2>/dev/null; then
  package_release
  if (
    cd "$INSTALL"
    bash "$REPO/site/static/install.sh" --update > "$SCRATCH/symlink-jar.log" 2>&1
  ); then
    echo 'installer accepted a symbolic link named as a jar' >&2
    exit 1
  fi
fi

# Validation happens while both backups still exist. A downloaded binary that
# reports the wrong version is removed and the prior binary/runtime pair wins.
write_binary "$ASSETS/foton" 0.0.0
write_runtime "$ASSETS/runtime" v9.8.7 new
package_release
if (
  cd "$INSTALL"
  bash "$REPO/site/static/install.sh" --update > "$SCRATCH/rollback.log" 2>&1
); then
  echo 'installer accepted a binary with the wrong version' >&2
  exit 1
fi
grep -q 'previous installation was restored' "$SCRATCH/rollback.log"
[ "$("$INSTALL/foton" --version)" = 'foton 9.8.6' ]
grep -q '^library old adventure-api-5.2.0.jar$' "$INSTALL/plugin-runtime/lib/adventure-api-5.2.0.jar"
if [ -e "$INSTALL/.foton-install-transaction" ]; then
  echo 'a completed rollback left transaction backups behind' >&2
  exit 1
fi

# Restoration errors are never swallowed or described as a successful
# rollback. The untouched backups are named so an operator can recover them.
export FOTON_INSTALL_FAIL_RESTORE=1
if (
  cd "$INSTALL"
  bash "$REPO/site/static/install.sh" --update > "$SCRATCH/rollback-failed.log" 2>&1
); then
  echo 'installer ignored a rollback failure' >&2
  exit 1
fi
unset FOTON_INSTALL_FAIL_RESTORE
grep -q 'rollback was incomplete:' "$SCRATCH/rollback-failed.log"
grep -q 'binary-backup=' "$SCRATCH/rollback-failed.log"
[ -f "$INSTALL/.foton-install-transaction/previous-binary" ]
[ -d "$INSTALL/.foton-install-transaction/previous-runtime" ]
grep -q '^library old adventure-api-5.2.0.jar$' "$INSTALL/plugin-runtime/lib/adventure-api-5.2.0.jar"

# The next launch recognizes the fixed transaction journal, restores it under
# the destination lock, and only then starts a new attempt.
if (
  cd "$INSTALL"
  bash "$REPO/site/static/install.sh" --update > "$SCRATCH/recovery.log" 2>&1
); then
  echo 'wrong-version fixture unexpectedly succeeded after recovery' >&2
  exit 1
fi
grep -q 'Recovered an interrupted installation' "$SCRATCH/recovery.log"
[ "$("$INSTALL/foton" --version)" = 'foton 9.8.6' ]
grep -q '^library old adventure-api-5.2.0.jar$' "$INSTALL/plugin-runtime/lib/adventure-api-5.2.0.jar"
[ ! -e "$INSTALL/.foton-install-transaction" ]

# An owner PID that is still alive makes the lock non-stale. A concurrent
# installer must stop before downloading or moving either installed file.
mkdir -p "$INSTALL/.foton-install.lock/claim.active"
printf '%s\n' "$$" > "$INSTALL/.foton-install.lock/claim.active/owner"
printf '1\n' > "$INSTALL/.foton-install.lock/claim.active/ticket"
if (
  cd "$INSTALL"
  bash "$REPO/site/static/install.sh" --update > "$SCRATCH/concurrent.log" 2>&1
); then
  echo 'installer ignored an active destination lock' >&2
  exit 1
fi
grep -q 'another installer is already modifying' "$SCRATCH/concurrent.log"
[ "$("$INSTALL/foton" --version)" = 'foton 9.8.6' ]
rm -rf "$INSTALL/.foton-install.lock"

# Stale claims are unique and never cause deletion of the shared lock root.
# Even with one beside it, a newer live claimant must survive unchanged.
mkdir -p "$INSTALL/.foton-install.lock/claim.stale" "$INSTALL/.foton-install.lock/claim.new-owner"
printf '99999999\n' > "$INSTALL/.foton-install.lock/claim.stale/owner"
printf '1\n' > "$INSTALL/.foton-install.lock/claim.stale/ticket"
printf '%s\n' "$$" > "$INSTALL/.foton-install.lock/claim.new-owner/owner"
printf '2\n' > "$INSTALL/.foton-install.lock/claim.new-owner/ticket"
if (
  cd "$INSTALL"
  bash "$REPO/site/static/install.sh" --update > "$SCRATCH/stale-race.log" 2>&1
); then
  echo 'installer ignored the newer live lock claim' >&2
  exit 1
fi
[ -d "$INSTALL/.foton-install.lock/claim.new-owner" ]
rm -rf "$INSTALL/.foton-install.lock"

# SIGTERM after the new binary is installed runs the EXIT rollback trap. The
# old pair returns and both the journal and lock disappear.
write_binary "$ASSETS/foton" 9.8.7
write_runtime "$ASSETS/runtime" v9.8.7 new
package_release
export FOTON_INSTALL_INTERRUPT=1
if (
  cd "$INSTALL"
  bash "$REPO/site/static/install.sh" --update > "$SCRATCH/interrupted.log" 2>&1
); then
  echo 'interrupted installer returned success' >&2
  exit 1
fi
unset FOTON_INSTALL_INTERRUPT
[ "$("$INSTALL/foton" --version)" = 'foton 9.8.6' ]
grep -q '^library old adventure-api-5.2.0.jar$' "$INSTALL/plugin-runtime/lib/adventure-api-5.2.0.jar"
[ ! -e "$INSTALL/.foton-install-transaction" ]
[ ! -e "$INSTALL/.foton-install.lock" ]

printf 'POSIX installer repair and rollback checked\n'
