#!/bin/bash
# Cut a Foton release: check, build, checksum, publish.
#
#   bash dev/release.sh            build, check and publish
#   bash dev/release.sh --dry-run  everything except the tag and the upload
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

say "Checking the tree"
[ -z "$(git status --porcelain)" ] || die "the working tree is dirty; commit or stash first"
BRANCH=$(git rev-parse --abbrev-ref HEAD)
[ "$BRANCH" = "master" ] || die "releases are cut from master, not $BRANCH"

# A dry run never reaches the publish step, so only a real run needs gh.
if [ "$DRY_RUN" -eq 0 ]; then
  command -v gh >/dev/null 2>&1 || die "gh is required to publish; install it or run with --dry-run"
  gh auth status >/dev/null 2>&1 || die "gh is not authenticated; run: gh auth login"
fi
command -v docker >/dev/null 2>&1 || printf 'docker is not installed; the Linux build will be skipped\n' >&2

VERSION=$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)".*/\1/')
[ -n "$VERSION" ] || die "no version in Cargo.toml"
TAG="v$VERSION"
if git rev-parse -q --verify "refs/tags/$TAG" >/dev/null; then
  # ls-remote exits non-zero both for "no such tag" and for "could not reach
  # the remote", and telling someone to delete a tag that is in fact published
  # is worse advice than admitting we do not know. So the three cases are kept
  # apart: found, absent, and unreachable.
  REMOTE_TAGS=$(git ls-remote --tags origin "$TAG" 2>/dev/null) && REMOTE_QUERY=ok || REMOTE_QUERY=failed
  if [ "$REMOTE_QUERY" = failed ]; then
    die "$TAG exists locally and the remote could not be reached, so whether it was already published is unknown. Check the network, then rerun."
  fi
  if [ -n "$REMOTE_TAGS" ]; then
    die "$TAG is already published. Bump the version in Cargo.toml, or if that release failed part-way: git push origin :refs/tags/$TAG && git tag -d $TAG"
  fi
  die "$TAG exists locally but was never pushed -- a previous run stopped part-way. Remove it and try again: git tag -d $TAG"
fi
say "Releasing $TAG"

say "Running the verification suite"
bash dev/ci.sh || die "dev/ci.sh failed; a release must be green"

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
  printf 'the release will publish without foton-linux-x86_64-musl\n' >&2
fi

say "Checksums"
( cd "$OUT" && shasum -a 256 foton-* > SHA256SUMS && cat SHA256SUMS )

say "Platform coverage"
# A laptop cannot cross-compile to Windows or to the other Mac architecture --
# say so plainly rather than publishing a partial release that looks complete.
ALL_ASSETS=(foton-linux-x86_64-musl foton-linux-aarch64-musl foton-macos-aarch64 foton-macos-x86_64 foton-windows-x86_64.exe)
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
  printf '\nThis is a partial release. A full release with all five assets comes\n'
  printf 'from the "Build Release" GitHub Actions workflow, not from this script.\n'
fi

if [ "$DRY_RUN" -eq 1 ]; then
  say "Dry run: stopping before the tag and the upload"
  printf 'artifacts are in %s\n' "$OUT"
  exit 0
fi

say "Publishing $TAG"
git tag -a "$TAG" -m "Foton $VERSION"
git push origin "$TAG" \
  || die "the tag $TAG was created locally but could not be pushed. Remove it and try again: git tag -d $TAG"
gh release create "$TAG" "$OUT"/* \
  --title "Foton $VERSION" \
  --notes "Install: \`curl -fsSL https://foton.zeffut.fr/install.sh | sh\`" \
  || die "the tag $TAG was pushed but the release was not created. Retry with: gh release create $TAG $OUT/* --title \"Foton $VERSION\""
say "Done: $TAG"
