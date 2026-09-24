#!/bin/bash
# Verify a Foton release locally, then ask the CI matrix to publish it.
#
#   bash dev/release.sh            build/check locally, dispatch CI publication
#   bash dev/release.sh --dry-run  local verification only; no CI dispatch
#
# This is the manual publishing procedure. GitHub Actions builds the platform
# matrix itself and runs the same dev/ci.sh gate before publishing artifacts.
set -euo pipefail
cd "$(dirname "$0")/.." || exit 1

DRY_RUN=0
[ "${1:-}" = "--dry-run" ] && DRY_RUN=1

# Respect $CARGO_TARGET_DIR the way `cargo build` itself does. Hardcoding
# ./target ships whatever an earlier unredirected build left there, which for a
# release means publishing a binary that is not the tag it claims to be.
TARGET_DIR="${CARGO_TARGET_DIR:-$(pwd)/target}"
OUT="$TARGET_DIR/release-artifacts"
say() { printf '\n\033[1m>>> %s\033[0m\n' "$1"; }
die() { printf '\033[31merror:\033[0m %s\n' "$1" >&2; exit 1; }
sha256_file() {
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    sha256sum "$1" | awk '{print $1}'
  fi
}
write_sha256s() {
  for path in "$@"; do
    printf '%s  %s\n' "$(sha256_file "$path")" "$path"
  done
}

say "Checking the tree"
[ -z "$(git status --porcelain)" ] || die "the working tree is dirty; commit or stash first"
BRANCH=$(git rev-parse --abbrev-ref HEAD)
[ "$BRANCH" = "master" ] || die "releases are cut from master, not $BRANCH"
START_HEAD=$(git rev-parse HEAD)

# A dry run never reaches the publish step, so only a real run needs gh.
if [ "$DRY_RUN" -eq 0 ]; then
  command -v gh >/dev/null 2>&1 || die "gh is required to publish; install it or run with --dry-run"
  gh auth status >/dev/null 2>&1 || die "gh is not authenticated; run: gh auth login"
fi
command -v docker >/dev/null 2>&1 || printf 'docker is not installed; the Linux build will be skipped\n' >&2

VERSION=$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)".*/\1/')
[ -n "$VERSION" ] || die "no version in Cargo.toml"
TAG="v$VERSION"
REMOTE_MASTER=$(git ls-remote --heads origin refs/heads/master 2>/dev/null | awk 'NR == 1 {print $1}') \
  || die "the remote could not be reached, so the release commit cannot be verified. Check the network, then rerun."
[ -n "$REMOTE_MASTER" ] || die "origin has no master branch"
if [ "$DRY_RUN" -eq 0 ] && [ "$REMOTE_MASTER" != "$START_HEAD" ]; then
  die "local master is not the exact origin/master commit; push or update it before dispatching a release"
fi
REMOTE_TAGS=$(git ls-remote --tags origin "refs/tags/$TAG" 2>/dev/null) \
  || die "the remote could not be reached, so release immutability cannot be verified. Check the network, then rerun."
if [ -n "$REMOTE_TAGS" ]; then
  die "$TAG is already published and immutable; choose a new version in Cargo.toml"
fi
if git rev-parse -q --verify "refs/tags/$TAG" >/dev/null; then
  die "$TAG already exists locally; keep the tag immutable and choose a new version in Cargo.toml"
fi
say "Releasing $TAG"

say "Running the verification suite"
bash dev/ci.sh || die "dev/ci.sh failed; a release must be green"

say "Building the Java plugin API"
bash dev/build-plugin-api.sh || die "the plugin API build failed"

rm -rf "$OUT" && mkdir -p "$OUT"

say "Building for this machine"
cargo build --release --locked --features stand-alone
HOST_ARCH=$(uname -m)
case "$HOST_ARCH" in
  arm64) HOST_ARCH=aarch64 ;;
  amd64) HOST_ARCH=x86_64 ;;
esac
case "$(uname -s)" in
  Darwin) HOST_NAME="foton-macos-$HOST_ARCH" ;;
  Linux)  HOST_NAME="foton-linux-$HOST_ARCH" ;;
  *) die "unsupported build host: $(uname -s)" ;;
esac
cp "$TARGET_DIR/release/foton" "$OUT/$HOST_NAME"

say "Building the static Linux binary in a container"
if docker info >/dev/null 2>&1; then
  docker build --platform linux/amd64 -f Dockerfile -t foton-release-build . \
    || die "the container build failed"
  CONTAINER=$(docker create --platform linux/amd64 foton-release-build)
  docker cp "$CONTAINER:/foton" "$OUT/foton-linux-x86_64-musl" \
    || die "could not copy the binary out of the image"
  docker rm "$CONTAINER" >/dev/null
else
  printf 'docker is not running; skipping the Linux build\n' >&2
  printf 'the local artifact set will omit foton-linux-x86_64-musl\n' >&2
fi

say "Packaging the plugin runtime"
PLUGIN_JARS=(plugin-api/lib/*.jar)
if [ ! -e "${PLUGIN_JARS[0]}" ]; then
  die "plugin-api/lib contains no runtime dependency jars"
fi
if [ "${#PLUGIN_JARS[@]}" -ne 24 ]; then
  die "the plugin runtime must contain exactly 24 pinned dependency jars; found ${#PLUGIN_JARS[@]}"
fi
for plugin_jar in "${PLUGIN_JARS[@]}"; do
  [ -f "$plugin_jar" ] && [ ! -L "$plugin_jar" ] \
    || die "the plugin runtime jar input is not a direct regular file: $plugin_jar"
done
PLUGIN_LICENSE_NAMES=(
  ADVENTURE-MIT.txt
  APACHE-2.0.txt
  BRIGADIER-MIT.txt
  JOML-MIT.txt
  SLF4J-MIT.txt
  THIRD-PARTY-NOTICES.txt
)
PLUGIN_LICENSES=()
for license_name in "${PLUGIN_LICENSE_NAMES[@]}"; do
  license_path="plugin-api/lib/licenses/$license_name"
  [ -f "$license_path" ] && [ ! -L "$license_path" ] \
    || die "the plugin runtime license input is missing or unsafe: $license_path"
  PLUGIN_LICENSES+=("$license_path")
done
if [ "${#PLUGIN_LICENSES[@]}" -ne 6 ]; then
  die "the plugin runtime must contain exactly 6 pinned license/notice texts; found ${#PLUGIN_LICENSES[@]}"
fi
if [ "$(find plugin-api/lib/licenses -type f -name '*.txt' | wc -l | tr -d ' ')" -ne 6 ]; then
  die "plugin-api/lib/licenses contains an unexpected license/notice file"
fi
PLUGIN_RUNTIME="$OUT/plugin-runtime"
mkdir -p "$PLUGIN_RUNTIME/lib" "$PLUGIN_RUNTIME/licenses"
cp plugin-api/build/foton-plugin-api.jar "$PLUGIN_RUNTIME/" \
  || die "the plugin API jar was not produced"
cp "${PLUGIN_JARS[@]}" "$PLUGIN_RUNTIME/lib/"
cp "${PLUGIN_LICENSES[@]}" "$PLUGIN_RUNTIME/licenses/"
( cd "$PLUGIN_RUNTIME" && write_sha256s foton-plugin-api.jar lib/*.jar licenses/*.txt > SHA256SUMS )
tar -czf "$OUT/foton-plugin-runtime.tar.gz" -C "$PLUGIN_RUNTIME" .
jar --create --file "$OUT/foton-plugin-runtime.zip" --no-manifest \
  -C "$PLUGIN_RUNTIME" .
rm -rf "$PLUGIN_RUNTIME"

say "Checksums"
( cd "$OUT" && write_sha256s foton-* > SHA256SUMS && cat SHA256SUMS )

say "Platform coverage"
# A laptop cannot cross-compile to Windows or to the other Mac architecture --
# say so plainly rather than publishing a partial release that looks complete.
ALL_ASSETS=(
  foton-linux-x86_64-musl
  foton-linux-aarch64-musl
  foton-macos-aarch64
  foton-macos-x86_64
  foton-windows-x86_64.exe
  foton-plugin-runtime.tar.gz
  foton-plugin-runtime.zip
)
MISSING=0
for asset in "${ALL_ASSETS[@]}"; do
  if [ -f "$OUT/$asset" ]; then
    printf '  present  %s\n' "$asset"
  else
    printf '  missing  %s\n' "$asset"
    MISSING=1
  fi
done
if [ "$MISSING" -eq 1 ]; then
  printf '\nThis is a partial release. A full release with all seven assets comes\n'
  printf 'from the "Build Release" GitHub Actions workflow, not from this script.\n'
fi

if [ "$DRY_RUN" -eq 1 ]; then
  say "Dry run: stopping before the CI release dispatch"
  printf 'artifacts are in %s\n' "$OUT"
  exit 0
fi

say "Dispatching the complete CI release for $TAG"
[ -z "$(git status --porcelain)" ] || die "the working tree changed during verification; rerun from a stable tree"
[ "$(git rev-parse HEAD)" = "$START_HEAD" ] \
  || die "HEAD changed during verification; rerun for the new commit"
REMOTE_MASTER=$(git ls-remote --heads origin refs/heads/master 2>/dev/null | awk 'NR == 1 {print $1}') \
  || die "the remote could not be reached during the final commit check"
[ "$REMOTE_MASTER" = "$START_HEAD" ] \
  || die "origin/master changed during verification; rerun against the new commit"
REMOTE_TAGS=$(git ls-remote --tags origin "refs/tags/$TAG" 2>/dev/null) \
  || die "the remote could not be reached during the final immutability check"
[ -z "$REMOTE_TAGS" ] || die "$TAG appeared remotely during the build; choose a new version"
if gh release view "$TAG" >/dev/null 2>&1; then
  die "release $TAG already exists; choose a new version"
fi
gh workflow run release.yml --ref master \
  || die "the Build Release workflow could not be dispatched"
say "Queued: the CI matrix will test, build all platforms and publish $TAG"
