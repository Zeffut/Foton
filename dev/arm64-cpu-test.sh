#!/bin/bash
# Boot an aarch64 Linux binary on an emulated Cortex-A72 until its first HTTPS
# exchange completes.
#
# The release builders are Neoverse cores, which have every crypto extension,
# so a binary that runs an instruction a Raspberry Pi lacks still passes all
# their tests. 0.16.3 did: aws-lc ran `sha512su0` and the server died in
# SIGILL on a Pi 4 at its first TLS handshake (#21). The Cortex-A72 has neither
# SHA-512 nor SHA-3, the extensions aws-lc selects at run time.
#
# The handshake is the startup fetch of Mojang's service keys, the very call
# that crashed. If it fails for want of network the test has proven nothing,
# so it fails too instead of passing.
#
# Usage: bash dev/arm64-cpu-test.sh <aarch64 binary> [qemu cpu]
# On a non-arm64 host, a glibc binary also needs QEMU_LD_PREFIX pointing at an
# aarch64 sysroot (e.g. /usr/aarch64-linux-gnu).
BIN=$(realpath "${1:?usage: arm64-cpu-test.sh <aarch64 binary> [qemu cpu]}") || exit 1
CPU="${2:-cortex-a72}"
TIMEOUT="${ARM64_CPU_TEST_TIMEOUT:-600}"

command -v qemu-aarch64 >/dev/null || { echo "qemu-aarch64 is not installed (apt install qemu-user)"; exit 1; }

RUN=$(mktemp -d)
trap 'kill "$PID" 2>/dev/null; sleep 2; kill -9 "$PID" 2>/dev/null; rm -rf "$RUN"' EXIT
cd "$RUN" || exit 1

echo "=== Booting $(basename "$BIN") on an emulated $CPU ==="
qemu-aarch64 -cpu "$CPU" "$BIN" > server.log 2>&1 < /dev/null &
PID=$!

for _ in $(seq 1 "$TIMEOUT"); do
  if ! kill -0 "$PID" 2>/dev/null; then
    wait "$PID"
    STATUS=$?
    tail -20 server.log
    # 132 is 128 + SIGILL.
    if [ "$STATUS" -eq 132 ]; then
      echo "FAILED: illegal instruction on $CPU -- the binary runs code this CPU lacks"
    else
      echo "FAILED: the server exited during startup with status $STATUS"
    fi
    exit 1
  fi
  if grep -q "Failed to load Minecraft services public keys" server.log; then
    grep "Failed to load Minecraft services public keys" server.log
    echo "FAILED: the service-key fetch never completed, so no TLS ran on $CPU"
    exit 1
  fi
  if grep -q "Started Foton Server" server.log; then
    echo "########## $CPU: HTTPS AND STARTUP PASSED ##########"
    exit 0
  fi
  sleep 1
done

tail -20 server.log
echo "FAILED: no startup within ${TIMEOUT}s on $CPU"
exit 1
