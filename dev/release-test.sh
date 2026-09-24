#!/usr/bin/env bash
# Hermetic check for the manual release's plugin-runtime assets.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/foton-release-test.XXXXXX")"
cleanup() {
  status=$?
  if [ "$status" -ne 0 ]; then
    for log in "$SCRATCH/output" "$SCRATCH/real-output"; do
      [ -f "$log" ] && { printf '\n--- %s ---\n' "$(basename "$log")" >&2; cat "$log" >&2; }
    done
  fi
  rm -rf "$SCRATCH"
  exit "$status"
}
trap cleanup EXIT

FIXTURE="$SCRATCH/repo"
MOCK_BIN="$SCRATCH/bin"
TARGET="$SCRATCH/target"
mkdir -p "$FIXTURE/dev" "$FIXTURE/plugin-api/lib" "$MOCK_BIN"
cp "$REPO/dev/release.sh" "$FIXTURE/dev/release.sh"
cp -R "$REPO/plugin-api/lib/licenses" "$FIXTURE/plugin-api/lib/licenses"
printf '[workspace.package]\nversion = "9.8.7+mc26.2"\n' > "$FIXTURE/Cargo.toml"

for source_jar in "$REPO"/plugin-api/lib/*.jar; do
  printf 'dependency %s\n' "$(basename "$source_jar")" > "$FIXTURE/plugin-api/lib/$(basename "$source_jar")"
done

cat > "$FIXTURE/dev/ci.sh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf 'ci\n' >> "$TEST_SEQUENCE"
EOF
cat > "$FIXTURE/dev/build-plugin-api.sh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
grep -qx 'ci' "$TEST_SEQUENCE"
printf 'plugin-api\n' >> "$TEST_SEQUENCE"
mkdir -p plugin-api/build
printf 'api jar\n' > plugin-api/build/foton-plugin-api.jar
EOF
cat > "$MOCK_BIN/git" <<'EOF'
#!/usr/bin/env bash
case "${1:-}" in
  status) exit 0 ;;
  rev-parse)
    if [ "${2:-}" = "--abbrev-ref" ]; then printf 'master\n'; exit 0; fi
    if [ "${2:-}" = "HEAD" ]; then printf '%s\n' '0123456789012345678901234567890123456789'; exit 0; fi
    exit 1
    ;;
  ls-remote)
    if [ "${2:-}" = "--heads" ]; then
      remote_master='0123456789012345678901234567890123456789'
      if [ -n "${TEST_REMOTE_MASTER_AFTER_FIRST:-}" ]; then
        if [ -e "$TEST_REMOTE_HEAD_MARKER" ]; then
          remote_master=$TEST_REMOTE_MASTER_AFTER_FIRST
        else
          : > "$TEST_REMOTE_HEAD_MARKER"
        fi
      fi
      printf '%s\trefs/heads/master\n' "$remote_master"
      exit 0
    fi
    if [ -n "${TEST_REMOTE_TAG_AFTER_FIRST:-}" ]; then
      if [ -e "$TEST_REMOTE_TAG_MARKER" ]; then
        printf '0123456789012345678901234567890123456789\trefs/tags/v9.8.7+mc26.2\n'
      else
        : > "$TEST_REMOTE_TAG_MARKER"
      fi
    elif [ "${TEST_REMOTE_TAG_EXISTS:-0}" = 1 ]; then
      printf '0123456789012345678901234567890123456789\trefs/tags/v9.8.7+mc26.2\n'
    fi
    exit 0
    ;;
  tag|push) printf 'dry-run attempted git publication\n' >&2; exit 97 ;;
  *) printf 'unexpected git invocation: %s\n' "$*" >&2; exit 98 ;;
esac
EOF
cat > "$MOCK_BIN/cargo" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
[ "${1:-}" = build ]
mkdir -p "$CARGO_TARGET_DIR/release"
printf 'foton binary\n' > "$CARGO_TARGET_DIR/release/foton"
EOF
cat > "$MOCK_BIN/docker" <<'EOF'
#!/usr/bin/env bash
exit 1
EOF
cat > "$MOCK_BIN/uname" <<'EOF'
#!/usr/bin/env bash
case "${1:-}" in
  -s) printf 'Darwin\n' ;;
  -m) printf 'arm64\n' ;;
  *) printf 'Darwin\n' ;;
esac
EOF
cat > "$MOCK_BIN/gh" <<'EOF'
#!/usr/bin/env bash
if [ "${1:-}" = auth ] && [ "${2:-}" = status ]; then exit 0; fi
if [ "${1:-}" = release ] && [ "${2:-}" = view ]; then exit 1; fi
if [ "${1:-}" = workflow ] && [ "${2:-}" = run ] && [ "${3:-}" = release.yml ]; then
  printf 'workflow %s\n' "$*" > "$TEST_GH_CALLED"
  exit 0
fi
printf 'unexpected gh invocation: %s\n' "$*" >&2
exit 96
EOF
chmod +x "$FIXTURE/dev/ci.sh" "$FIXTURE/dev/build-plugin-api.sh" \
  "$MOCK_BIN/git" "$MOCK_BIN/cargo" "$MOCK_BIN/docker" "$MOCK_BIN/uname" \
  "$MOCK_BIN/gh"

export CARGO_TARGET_DIR="$TARGET"
export TEST_SEQUENCE="$SCRATCH/sequence"
export TEST_GH_CALLED="$SCRATCH/gh-called"
export TEST_REMOTE_HEAD_MARKER="$SCRATCH/remote-head-seen"
export TEST_REMOTE_TAG_MARKER="$SCRATCH/remote-tag-seen"
export PATH="$MOCK_BIN:$PATH"

# The publishing workflow is itself part of the supply chain. Manual runs may
# only target master, and every third-party action must be immutable.
WORKFLOW="$REPO/.github/workflows/release.yml"
grep -q 'EVENT_REF:.*github.ref' "$WORKFLOW"
grep -q 'EVENT_REF.*refs/heads/master' "$WORKFLOW"
grep -A2 '^permissions:' "$WORKFLOW" | grep -q 'contents: read'
grep -q 'gh release create' "$WORKFLOW"
! grep -Eq 'action-gh-release|updateRelease|edit release|gh release edit' "$WORKFLOW"
grep -q '^    name: Create immutable GitHub release$' "$WORKFLOW"
grep -q '^  notify-live-forever:$' "$WORKFLOW"
grep -q '^    name: Dispatch live_forever archive$' "$WORKFLOW"
create_job_line=$(grep -n '^  release:$' "$WORKFLOW" | cut -d: -f1)
notify_job_line=$(grep -n '^  notify-live-forever:$' "$WORKFLOW" | cut -d: -f1)
dispatch_line=$(grep -n 'event_type=foton-released' "$WORKFLOW" | cut -d: -f1)
[ "$create_job_line" -lt "$notify_job_line" ]
[ "$notify_job_line" -lt "$dispatch_line" ]
grep -q 'already exists for this exact commit; leaving it immutable' "$WORKFLOW"
grep -q 'partial or unexpected release asset set' "$WORKFLOW"
grep -q 'different SHA256SUMS manifest' "$WORKFLOW"
grep -q 'fails SHA256SUMS' "$WORKFLOW"
[ "$(grep -c 'git ls-remote --tags origin' "$FIXTURE/dev/release.sh")" -eq 2 ]
for workflow in "$WORKFLOW" "$REPO/.github/workflows/test.yml" \
  "$REPO/.github/workflows/docker-build.yml" "$REPO/.github/workflows/docs.yml"; do
  if grep 'uses:' "$workflow" | grep -Ev '@[0-9a-f]{40}([[:space:]]|$)' >/dev/null; then
    echo "$workflow contains an action that is not pinned to a full commit SHA" >&2
    exit 1
  fi
done
grep -A2 '^permissions:' "$REPO/.github/workflows/test.yml" | grep -q 'contents: read'

DOCKER_WORKFLOW="$REPO/.github/workflows/docker-build.yml"
grep -A8 '^  workflow_run:' "$DOCKER_WORKFLOW" | grep -q -- '- Test'
grep -A8 '^  workflow_run:' "$DOCKER_WORKFLOW" | grep -q -- '- Build Release'
grep -A8 '^  workflow_run:' "$DOCKER_WORKFLOW" | grep -q -- '- completed'
grep -A8 '^  workflow_run:' "$DOCKER_WORKFLOW" | grep -q -- '- master'
! grep -q '^  push:' "$DOCKER_WORKFLOW"
! grep -q '^  repository_dispatch:' "$DOCKER_WORKFLOW"
grep -q "workflow_run.conclusion == 'success'" "$DOCKER_WORKFLOW"
grep -q "workflow_run.event == 'push'" "$DOCKER_WORKFLOW"
grep -q 'actions/workflows/${workflow}/runs' "$DOCKER_WORKFLOW"
grep -q 'head_sha=.*source_sha' "$DOCKER_WORKFLOW"
grep -q 'ref:.*needs.gate.outputs.sha' "$DOCKER_WORKFLOW"
grep -Fq "group: docker-\${{ (inputs.release_sha || github.event.workflow_run.name == 'Build Release') && format('release-{0}', inputs.release_sha || github.event.workflow_run.head_sha) || 'nightly' }}" \
  "$DOCKER_WORKFLOW"
grep -Fq "cancel-in-progress: \${{ github.event.workflow_run.name != 'Build Release' && inputs.release_sha == '' }}" \
  "$DOCKER_WORKFLOW"
grep -q '^      release_tag:' "$DOCKER_WORKFLOW"
grep -q '^      release_sha:' "$DOCKER_WORKFLOW"
grep -Fq '^v[0-9]+\.[0-9]+\.[0-9]+\+mc[0-9]+(\.[0-9]+)*$' "$DOCKER_WORKFLOW"
grep -q 'git/ref/tags/${release_tag}' "$DOCKER_WORKFLOW"
grep -q 'commits/${release_tag}' "$DOCKER_WORKFLOW"
grep -q 'releases/tags/${release_tag}' "$DOCKER_WORKFLOW"
grep -q 'target_commitish' "$DOCKER_WORKFLOW"
grep -q 'image_tag=${release_tag/+/-}' "$DOCKER_WORKFLOW"
release_tag_example='v0.15.2+mc26.2'
[ "${release_tag_example/+/-}" = 'v0.15.2-mc26.2' ]
grep -q 'matching-refs/tags/${image_tag}' "$DOCKER_WORKFLOW"
grep -q 'canonical OCI tag is invalid' "$DOCKER_WORKFLOW"
grep -q 'release recovery requires both release_tag and release_sha' "$DOCKER_WORKFLOW"
grep -q 'actions/runs/${run_id}/jobs' "$DOCKER_WORKFLOW"
grep -q 'jobs?filter=all&per_page=100' "$DOCKER_WORKFLOW"
grep -q 'Create immutable GitHub release' "$DOCKER_WORKFLOW"
grep -q 'approved_release_run' "$DOCKER_WORKFLOW"
grep -q 'release_tag_for_sha' "$DOCKER_WORKFLOW"
grep -Fq '$0 == "[workspace.package]"' "$DOCKER_WORKFLOW"
grep -q 'does not match workspace version' "$DOCKER_WORKFLOW"
grep -q 'Build Release had no version bump; Docker publication is a clean no-op' \
  "$DOCKER_WORKFLOW"
grep -q "needs.gate.outputs.publish == 'true'" "$DOCKER_WORKFLOW"
manual_guard_line=$(grep -n 'manual Docker runs must target refs/heads/master' \
  "$DOCKER_WORKFLOW" | cut -d: -f1)
case_line=$(grep -n '^          case "${EVENT_NAME}" in' "$DOCKER_WORKFLOW" | cut -d: -f1)
[ "$manual_guard_line" -lt "$case_line" ]
[ "$(grep -c "github.event_name == 'workflow_run' || github.ref == 'refs/heads/master'" \
  "$DOCKER_WORKFLOW")" -eq 2 ]
grep -q "type=raw,value=nightly,enable=.*needs.gate.outputs.is_release != 'true'" \
  "$DOCKER_WORKFLOW"
[ "$(grep -c 'org.opencontainers.image.revision=.*needs.gate.outputs.sha' "$DOCKER_WORKFLOW")" -ge 3 ]
grep -q 'commits/master' "$DOCKER_WORKFLOW"
grep -q 'refusing to publish stale' "$DOCKER_WORKFLOW"
grep -q 'imagetools inspect --raw' "$DOCKER_WORKFLOW"
grep -q 'already publishes ${SOURCE_SHA}; recovery is complete' "$DOCKER_WORKFLOW"
grep -q 'application/vnd.oci.image.index.v1+json' "$DOCKER_WORKFLOW"
grep -q 'application/vnd.oci.image.manifest.v1+json' "$DOCKER_WORKFLOW"
grep -q 'manifests | length == 2' "$DOCKER_WORKFLOW"
grep -q 'linux/amd64.*linux/arm64' "$DOCKER_WORKFLOW"
grep -q 'exact approved amd64/arm64 index' "$DOCKER_WORKFLOW"
revision_line=$(grep -n 'index:org.opencontainers.image.revision=${SOURCE_SHA}' \
  "$DOCKER_WORKFLOW" | cut -d: -f1)
revalidate_line=$(grep -n 'repos/${GITHUB_REPOSITORY}/commits/master' \
  "$DOCKER_WORKFLOW" | cut -d: -f1)
merge_line=$(grep -n 'docker buildx imagetools create' "$DOCKER_WORKFLOW" | cut -d: -f1)
[ "$revalidate_line" -lt "$merge_line" ]
[ "$revision_line" -gt "$merge_line" ]
grep -q 'image: ghcr.io/zeffut/foton:nightly' "$REPO/docker-compose.yml"
grep -q '^FROM rustlang/rust:nightly-alpine3.23-2026-07-23@sha256:e4a0ce16a94f2585bc5fe1d852d70f77befdc89860da2d1afd89fd40d6ce830c AS builder$' \
  "$REPO/Dockerfile"
! grep -Eq '^FROM [^ @]+:[^ @]+ AS builder$' "$REPO/Dockerfile"

# Durable notices are required inputs, not files synthesized at publication.
[ "$(find "$REPO/plugin-api/lib/licenses" -maxdepth 1 -type f -name '*.txt' | wc -l)" -eq 6 ]
for license in ADVENTURE-MIT.txt APACHE-2.0.txt BRIGADIER-MIT.txt JOML-MIT.txt \
  SLF4J-MIT.txt THIRD-PARTY-NOTICES.txt; do
  [ -f "$REPO/plugin-api/lib/licenses/$license" ]
done
grep -q 'END OF TERMS AND CONDITIONS' "$REPO/plugin-api/lib/licenses/APACHE-2.0.txt"
for dependency in adventure-api adventure-key adventure-text-logger-slf4j \
  adventure-text-serializer-plain annotations brigadier gson guava \
  failureaccess jspecify error_prone_annotations j2objc-annotations joml \
  kotlin-stdlib-jdk8 kotlin-stdlib-jdk7 kotlin-stdlib kotlin-stdlib-common \
  netty-buffer netty-codec-base netty-common netty-resolver netty-transport \
  slf4j-api snakeyaml; do
  grep -qi "$dependency" "$REPO/plugin-api/lib/licenses/THIRD-PARTY-NOTICES.txt" \
    || { echo "third-party notices omit $dependency" >&2; exit 1; }
done

OUTPUT="$SCRATCH/output"
bash "$FIXTURE/dev/release.sh" --dry-run > "$OUTPUT" 2>&1
OUT="$TARGET/release-artifacts"

[ "$(cat "$TEST_SEQUENCE")" = $'ci\nplugin-api' ]
[ ! -e "$TEST_GH_CALLED" ]
[ -f "$OUT/foton-plugin-runtime.tar.gz" ]
[ -f "$OUT/foton-plugin-runtime.zip" ]
tar -tzf "$OUT/foton-plugin-runtime.tar.gz" > "$SCRATCH/tar-list"
jar tf "$OUT/foton-plugin-runtime.zip" > "$SCRATCH/zip-list"
tar -xOzf "$OUT/foton-plugin-runtime.tar.gz" ./SHA256SUMS > "$SCRATCH/runtime-sums"
[ "$(grep -c '^[0-9a-f]\{64\}  ' "$SCRATCH/runtime-sums")" -eq 31 ]
[ "$(grep -c '\.jar$' "$SCRATCH/tar-list")" -eq 25 ]
[ "$(grep -c '\.jar$' "$SCRATCH/zip-list")" -eq 25 ]
[ "$(grep -c '^\./licenses/.*\.txt$' "$SCRATCH/tar-list")" -eq 6 ]
[ "$(grep -c '^licenses/.*\.txt$' "$SCRATCH/zip-list")" -eq 6 ]
grep -qx './foton-plugin-api.jar' "$SCRATCH/tar-list"
grep -qx './lib/snakeyaml-2.2.jar' "$SCRATCH/tar-list"
grep -qx './lib/netty-codec-base-4.2.15.Final.jar' "$SCRATCH/tar-list"
grep -qx 'foton-plugin-api.jar' "$SCRATCH/zip-list"
grep -qx 'lib/snakeyaml-2.2.jar' "$SCRATCH/zip-list"
grep -qx 'lib/netty-transport-4.2.15.Final.jar' "$SCRATCH/zip-list"
grep -qx './SHA256SUMS' "$SCRATCH/tar-list"
grep -qx 'SHA256SUMS' "$SCRATCH/zip-list"
grep -qx './licenses/THIRD-PARTY-NOTICES.txt' "$SCRATCH/tar-list"
grep -qx 'licenses/APACHE-2.0.txt' "$SCRATCH/zip-list"
grep -q 'foton-plugin-api.jar' "$SCRATCH/runtime-sums"
grep -q 'licenses/THIRD-PARTY-NOTICES.txt' "$SCRATCH/runtime-sums"
mkdir "$SCRATCH/unpacked-runtime"
tar -xzf "$OUT/foton-plugin-runtime.tar.gz" -C "$SCRATCH/unpacked-runtime"
( cd "$SCRATCH/unpacked-runtime" && sha256sum --check SHA256SUMS >/dev/null )
grep -q 'foton-plugin-runtime.tar.gz' "$OUT/SHA256SUMS"
grep -q 'foton-plugin-runtime.zip' "$OUT/SHA256SUMS"
grep -Eq 'present +foton-plugin-runtime.tar.gz' "$OUTPUT"
grep -Eq 'present +foton-plugin-runtime.zip' "$OUTPUT"
grep -q 'Dry run: stopping before the CI release dispatch' "$OUTPUT"

# A published tag is immutable even in dry-run mode: release tooling must ask
# for a new version, never suggest deleting or replacing the remote tag.
export TEST_REMOTE_TAG_EXISTS=1
if bash "$FIXTURE/dev/release.sh" --dry-run > "$SCRATCH/existing-tag.log" 2>&1; then
  echo 'manual release accepted an already-published tag' >&2
  exit 1
fi
unset TEST_REMOTE_TAG_EXISTS
grep -q 'already published and immutable' "$SCRATCH/existing-tag.log"
! grep -Eq 'push origin :refs/tags|tag -d' "$SCRATCH/existing-tag.log"

rm "$FIXTURE/plugin-api/lib/licenses/SLF4J-MIT.txt"
if bash "$FIXTURE/dev/release.sh" --dry-run > "$SCRATCH/missing-license-release.log" 2>&1; then
  echo 'manual release accepted a missing exact license file' >&2
  exit 1
fi
grep -q 'license input is missing or unsafe' "$SCRATCH/missing-license-release.log"
cp "$REPO/plugin-api/lib/licenses/SLF4J-MIT.txt" "$FIXTURE/plugin-api/lib/licenses/"

# The final check closes changes that occur during the build: neither a moved
# master branch nor a concurrently-created tag may be dispatched over.
rm -f "$TEST_GH_CALLED" "$TEST_REMOTE_HEAD_MARKER"
export TEST_REMOTE_MASTER_AFTER_FIRST=1111111111111111111111111111111111111111
if bash "$FIXTURE/dev/release.sh" > "$SCRATCH/moved-master.log" 2>&1; then
  echo 'manual release dispatched after origin/master moved during its build' >&2
  exit 1
fi
unset TEST_REMOTE_MASTER_AFTER_FIRST
grep -q 'origin/master changed during verification' "$SCRATCH/moved-master.log"
[ ! -e "$TEST_GH_CALLED" ]

rm -f "$TEST_GH_CALLED" "$TEST_REMOTE_TAG_MARKER"
export TEST_REMOTE_TAG_AFTER_FIRST=1
if bash "$FIXTURE/dev/release.sh" > "$SCRATCH/tag-race.log" 2>&1; then
  echo 'manual release dispatched after its tag appeared during the build' >&2
  exit 1
fi
unset TEST_REMOTE_TAG_AFTER_FIRST
grep -q 'appeared remotely during the build' "$SCRATCH/tag-race.log"
[ ! -e "$TEST_GH_CALLED" ]

# The manual path can build fewer platforms than CI. A stable real run must
# dispatch the matrix for the exact origin/master commit instead of reaching
# dead local tag/upload code with a permanently partial set.
rm -f "$TEST_GH_CALLED"
if ! bash "$FIXTURE/dev/release.sh" > "$SCRATCH/real-output" 2>&1; then
  echo "manual release did not dispatch the complete CI matrix" >&2
  exit 1
fi
grep -q 'Queued: the CI matrix will test, build all platforms and publish' "$SCRATCH/real-output"
grep -qx 'workflow workflow run release.yml --ref master' "$TEST_GH_CALLED"
! grep -Eq 'push origin :refs/tags|tag -d' "$FIXTURE/dev/release.sh"

# `--check` is diagnostic: it must neither delete a changed tracked jar nor
# accept a nested jar that the old recursive API classpath could consume.
FETCH_FIXTURE="$SCRATCH/fetch-repo"
mkdir -p "$FETCH_FIXTURE/dev" "$FETCH_FIXTURE/plugin-api/lib"
cp "$REPO/dev/fetch-plugin-api-libs.sh" "$FETCH_FIXTURE/dev/"
cp "$REPO"/plugin-api/lib/*.jar "$FETCH_FIXTURE/plugin-api/lib/"
bash "$FETCH_FIXTURE/dev/fetch-plugin-api-libs.sh" --check
CHECK_JAR="$FETCH_FIXTURE/plugin-api/lib/failureaccess-1.0.3.jar"
printf 'locally changed\n' > "$CHECK_JAR"
if bash "$FETCH_FIXTURE/dev/fetch-plugin-api-libs.sh" --check > "$SCRATCH/stale-check.log" 2>&1; then
  echo '--check accepted a changed pinned jar' >&2
  exit 1
fi
grep -qx 'locally changed' "$CHECK_JAR"
cp "$REPO/plugin-api/lib/failureaccess-1.0.3.jar" "$CHECK_JAR"
mkdir "$FETCH_FIXTURE/plugin-api/lib/nested"
printf 'not pinned\n' > "$FETCH_FIXTURE/plugin-api/lib/nested/injected.jar"
if bash "$FETCH_FIXTURE/dev/fetch-plugin-api-libs.sh" --check > "$SCRATCH/nested-check.log" 2>&1; then
  echo '--check accepted a nested unpinned jar' >&2
  exit 1
fi
grep -q 'nested jar' "$SCRATCH/nested-check.log"

# Keep the compiler classpath direct-only and its javac argfile paths quoted.
grep -q 'for library in .*plugin-api/lib/\*.jar' "$REPO/dev/build-plugin-api.sh"
grep -q "printf '\"%s\"\\\\n'" "$REPO/dev/build-plugin-api.sh"
ARGFILE_DIR="$SCRATCH/javac sources with spaces"
mkdir -p "$ARGFILE_DIR/classes"
printf 'public final class SpacedSource {}\n' > "$ARGFILE_DIR/SpacedSource.java"
argfile_source="$ARGFILE_DIR/SpacedSource.java"
argfile_source=${argfile_source//\\/\\\\}
argfile_source=${argfile_source//\"/\\\"}
printf '"%s"\n' "$argfile_source" > "$ARGFILE_DIR/sources.txt"
javac -d "$ARGFILE_DIR/classes" "@$ARGFILE_DIR/sources.txt"
[ -f "$ARGFILE_DIR/classes/SpacedSource.class" ]

# Reusing an immutable GitHub release is allowed only when all eight assets
# exist and every downloaded payload matches the published checksum manifest.
RELEASE_REUSE_SCRIPT="$SCRATCH/release-reuse.sh"
tr -d '\r' < "$WORKFLOW" | awk '
  /^      - name: Create immutable release$/ { named = 1; next }
  named && /^        run: \|$/ { found = 1; next }
  found && /^  notify-live-forever:$/ { exit }
  found { sub(/^          /, ""); print }
' > "$RELEASE_REUSE_SCRIPT"
bash -n "$RELEASE_REUSE_SCRIPT"

REUSE_FIXTURE="$SCRATCH/reuse-release"
REUSE_REMOTE="$SCRATCH/reuse-remote"
REUSE_BIN="$SCRATCH/reuse-bin"
mkdir -p "$REUSE_FIXTURE/release" "$REUSE_REMOTE" "$REUSE_BIN"
for asset in foton-linux-aarch64-musl foton-linux-x86_64-musl \
  foton-macos-aarch64 foton-macos-x86_64 foton-plugin-runtime.tar.gz \
  foton-plugin-runtime.zip foton-windows-x86_64.exe; do
  printf 'asset %s\n' "$asset" > "$REUSE_FIXTURE/release/$asset"
done
( cd "$REUSE_FIXTURE/release" && shasum -a 256 foton-* > SHA256SUMS )
cp "$REUSE_FIXTURE"/release/* "$REUSE_REMOTE/"

cat > "$REUSE_BIN/gh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [ "${1:-}" = api ]; then
  endpoint=${2:-}
  case "$endpoint" in
    */commits/*)
      printf '%s\n' "$TEST_RELEASE_SHA"
      ;;
    */releases/tags/*)
      assets='[{"name":"SHA256SUMS"},{"name":"foton-linux-aarch64-musl"},{"name":"foton-linux-x86_64-musl"},{"name":"foton-macos-aarch64"},{"name":"foton-macos-x86_64"},{"name":"foton-plugin-runtime.tar.gz"},{"name":"foton-plugin-runtime.zip"},{"name":"foton-windows-x86_64.exe"}]'
      if [ "$TEST_RELEASE_MODE" = partial ]; then
        assets='[{"name":"SHA256SUMS"},{"name":"foton-linux-aarch64-musl"}]'
      fi
      printf '{"tag_name":"v9.8.7+mc26.2","target_commitish":"%s","draft":false,"assets":%s}\n' \
        "$TEST_RELEASE_SHA" "$assets"
      ;;
    *) exit 96 ;;
  esac
  exit 0
fi
if [ "${1:-}" = release ] && [ "${2:-}" = download ]; then
  destination=
  while [ "$#" -gt 0 ]; do
    if [ "$1" = --dir ]; then destination=$2; break; fi
    shift
  done
  [ -n "$destination" ]
  cp "$TEST_RELEASE_REMOTE"/* "$destination/"
  exit 0
fi
printf 'unexpected release reuse gh invocation: %s\n' "$*" >&2
exit 97
EOF
chmod +x "$REUSE_BIN/gh"

TEST_RELEASE_SHA=0123456789012345678901234567890123456789
export TEST_RELEASE_SHA TEST_RELEASE_REMOTE="$REUSE_REMOTE"
run_release_reuse() {
  mode=$1
  log=$2
  ( cd "$REUSE_FIXTURE" && PATH="$REUSE_BIN:$PATH" TEST_RELEASE_MODE="$mode" \
    GITHUB_REPOSITORY=example/foton GITHUB_SHA="$TEST_RELEASE_SHA" \
    RELEASE_TAG=v9.8.7+mc26.2 bash -euo pipefail "$RELEASE_REUSE_SCRIPT" ) \
    > "$log" 2>&1
}

if run_release_reuse partial "$SCRATCH/reuse-partial.log"; then
  echo 'release reuse accepted a partial remote asset set' >&2
  exit 1
fi
grep -q 'partial or unexpected release asset set' "$SCRATCH/reuse-partial.log"

printf 'corrupt remote payload\n' > "$REUSE_REMOTE/foton-windows-x86_64.exe"
if run_release_reuse mismatch "$SCRATCH/reuse-mismatch.log"; then
  echo 'release reuse accepted an asset that failed SHA256SUMS' >&2
  exit 1
fi
grep -q 'fails SHA256SUMS' "$SCRATCH/reuse-mismatch.log"

cp "$REUSE_FIXTURE/release/foton-windows-x86_64.exe" "$REUSE_REMOTE/"
run_release_reuse exact "$SCRATCH/reuse-exact.log"
grep -q 'leaving it immutable' "$SCRATCH/reuse-exact.log"

# Exercise the Docker gate with GitHub API fixtures. A skipped release job is
# a clean no-op, while a successful creation job remains authoritative even
# when the later archival dispatch made the overall workflow fail.
DOCKER_GATE="$SCRATCH/docker-gate.sh"
tr -d '\r' < "$DOCKER_WORKFLOW" \
  | awk '
      found && /^  docker:$/ { exit }
      !found && /^        run: \|$/ { found = 1; next }
      found { sub(/^          /, ""); print }
    ' > "$DOCKER_GATE"
bash -n "$DOCKER_GATE"

DOCKER_MOCK_BIN="$SCRATCH/docker-bin"
mkdir "$DOCKER_MOCK_BIN"
cat > "$DOCKER_MOCK_BIN/gh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
[ -z "${TEST_DOCKER_GH_MARKER:-}" ] || : > "$TEST_DOCKER_GH_MARKER"
endpoint=
for argument in "$@"; do
  case "$argument" in repos/*) endpoint=$argument ;; esac
done
case "$endpoint" in
  */actions/runs/*/jobs*)
    printf '%s\n' "${TEST_RELEASE_JOB_CONCLUSION:-success}"
    ;;
  */actions/workflows/release.yml/runs)
    printf '{"workflow_runs":[{"id":42,"head_sha":"%s","head_branch":"master","status":"completed"}]}\n' \
      "$TEST_DOCKER_SHA"
    ;;
  */contents/Cargo.toml\?ref=*)
    printf '[workspace.package]\nversion = "9.8.7+mc26.2"\n' | base64 -w0
    printf '\n'
    ;;
  */git/ref/tags/*)
    printf 'refs/tags/%s\n' "${endpoint##*/}"
    ;;
  */commits/*)
    printf '%s\n' "$TEST_DOCKER_SHA"
    ;;
  */releases/tags/*)
    printf '{"tag_name":"v9.8.7+mc26.2","target_commitish":"%s","draft":false}\n' \
      "$TEST_DOCKER_SHA"
    ;;
  */git/matching-refs/tags/*)
    printf '[]\n'
    ;;
  *)
    printf 'unexpected Docker gate gh invocation: %s\n' "$*" >&2
    exit 96
    ;;
esac
EOF
chmod +x "$DOCKER_MOCK_BIN/gh"

TEST_DOCKER_SHA=0123456789012345678901234567890123456789
export TEST_DOCKER_SHA
run_docker_gate() {
  output=$1
  conclusion=$2
  job_conclusion=$3
  : > "$output"
  PATH="$DOCKER_MOCK_BIN:$PATH" \
    EVENT_NAME=workflow_run EVENT_REF=refs/heads/master \
    SOURCE_WORKFLOW='Build Release' SOURCE_BRANCH=master \
    SOURCE_CONCLUSION="$conclusion" SOURCE_RUN_ID=42 \
    TESTED_SHA="$TEST_DOCKER_SHA" MANUAL_RELEASE_SHA= MANUAL_RELEASE_TAG= \
    GITHUB_REPOSITORY=example/foton GITHUB_SHA="$TEST_DOCKER_SHA" \
    GITHUB_OUTPUT="$output" TEST_RELEASE_JOB_CONCLUSION="$job_conclusion" \
    bash -euo pipefail "$DOCKER_GATE"
}

run_docker_gate "$SCRATCH/docker-skipped-output" success skipped
grep -qx 'publish=false' "$SCRATCH/docker-skipped-output"
run_docker_gate "$SCRATCH/docker-success-output" success success
grep -qx 'publish=true' "$SCRATCH/docker-success-output"
grep -qx 'release_tag=v9.8.7+mc26.2' "$SCRATCH/docker-success-output"
run_docker_gate "$SCRATCH/docker-downstream-failed-output" failure success
grep -qx 'publish=true' "$SCRATCH/docker-downstream-failed-output"

if PATH="$DOCKER_MOCK_BIN:$PATH" \
  EVENT_NAME=workflow_dispatch EVENT_REF=refs/heads/master SOURCE_WORKFLOW= \
  SOURCE_BRANCH= SOURCE_CONCLUSION= SOURCE_RUN_ID= TESTED_SHA= \
  MANUAL_RELEASE_SHA="$TEST_DOCKER_SHA" MANUAL_RELEASE_TAG='v9.8.8+mc26.2' \
  GITHUB_REPOSITORY=example/foton GITHUB_SHA="$TEST_DOCKER_SHA" \
  GITHUB_OUTPUT="$SCRATCH/docker-version-mismatch-output" \
  TEST_RELEASE_JOB_CONCLUSION=success \
  bash -euo pipefail "$DOCKER_GATE" > "$SCRATCH/docker-version-mismatch.log" 2>&1; then
  echo 'Docker recovery accepted a tag that does not match Cargo.toml at its SHA' >&2
  exit 1
fi
grep -q 'does not match workspace version v9.8.7+mc26.2' \
  "$SCRATCH/docker-version-mismatch.log"

printf 'manual release dispatch, packaging and classpath guards checked\n'
