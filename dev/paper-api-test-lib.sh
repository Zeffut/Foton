#!/usr/bin/env bash
# One pinned Paper API input shared by compatibility probes.
PAPER_API_SHA256=c577b181c11a8674310e56c92a91e31c010b7f04c9bd10b91c3be18374401070
PAPER_API_URL=https://repo.papermc.io/repository/maven-public/io/papermc/paper/paper-api/1.21.11-R0.1-SNAPSHOT/paper-api-1.21.11-R0.1-20260511.115010-91.jar

paper_api_prepare() {
  local requested="$1" scratch="$2" path actual_sha256
  path="$requested"
  if [ -z "$path" ]; then
    path="$scratch/paper-api-1.21.11-91.jar"
    curl -fsSL --retry 3 --max-time 180 -o "$path" "$PAPER_API_URL" || return 1
  fi
  [ -f "$path" ] || { echo "Paper API JAR is missing: $path" >&2; return 2; }
  actual_sha256="$(sha256sum "$path" | cut -d' ' -f1)"
  if [ "$actual_sha256" != "$PAPER_API_SHA256" ]; then
    echo "not the pinned official Paper 1.21.11 API: $path" >&2
    echo "expected SHA-256 $PAPER_API_SHA256, got $actual_sha256" >&2
    return 2
  fi
  printf '%s\n' "$path"
}
