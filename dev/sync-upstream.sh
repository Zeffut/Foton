#!/bin/bash
# Pull upstream progress into this fork, then verify nothing broke.
# Usage: bash dev/sync-upstream.sh
set -e
cd "$(dirname "$0")/.." || exit 1

if [ -n "$(git status --porcelain)" ]; then
  echo "Working tree is dirty. Commit or stash before syncing."
  git status --short
  exit 1
fi

echo "=== Fetching upstream ==="
git fetch upstream

COUNT=$(git rev-list --count HEAD..upstream/master)
echo "=== $COUNT upstream commit(s) to integrate ==="
git log --oneline HEAD..upstream/master | head -30
[ "$COUNT" -eq 0 ] && { echo "Already up to date."; exit 0; }

echo "=== Merging ==="
git checkout master
git merge upstream/master

echo "=== Verifying after merge ==="
bash dev/ci.sh
