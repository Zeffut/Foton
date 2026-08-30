#!/usr/bin/env bash
# Builds, verifies and ships Foton to the test server, then says so in chat.
#
# Nothing here is specific to one machine: every host detail comes from the
# environment, so the script is safe to keep in the repository.
#
#   FOTON_DEPLOY_SSH_KEY   private key for the game host
#   FOTON_DEPLOY_HOST      user@host of the game host
#   FOTON_DEPLOY_DIR       where the server lives there (default ~/foton-test)
#   FOTON_DEPLOY_CONTAINER docker container name (default foton-test)
#
# The order matters and is not negotiable:
#
#   1. CI first. A red suite stops everything -- a test server that lies is
#      worse than one that is down.
#   2. Warn the players before taking the server away from them. A session
#      cut mid-sentence loses whatever they were about to report.
#   3. Carry a fingerprint from the compiled binary all the way into the
#      running container. A file-exists check accepts a stale artifact
#      without complaint, which has already shipped the same build twice.
set -u

REPO=$(cd "$(dirname "$0")/.." && pwd)
cd "$REPO" || exit 1

KEY=${FOTON_DEPLOY_SSH_KEY:?FOTON_DEPLOY_SSH_KEY is not set}
HOST=${FOTON_DEPLOY_HOST:?FOTON_DEPLOY_HOST is not set}
DIR=${FOTON_DEPLOY_DIR:-\~/foton-test}
NAME=${FOTON_DEPLOY_CONTAINER:-foton-test}
NOTE=${1:-"a new build"}
GRACE=${FOTON_DEPLOY_GRACE:-30}

say() { printf '\n\033[1m== %s ==\033[0m\n' "$1"; }

# --- 1. the suite decides whether anything ships at all -------------------
# Formatting is mechanical: fixing it is never a judgment call, and a deploy
# that stops for it wastes a ten-minute build on nothing.
cargo fmt --all

say "continuous integration"
if ! bash dev/ci.sh; then
  echo "CI is red. Nothing was built and nothing was deployed."
  exit 1
fi

# --- 2. a static binary, because the game host is not this machine --------
say "release build (static musl)"
rustup target add x86_64-unknown-linux-musl >/dev/null 2>&1
command -v musl-gcc >/dev/null || apt-get install -y -qq musl-tools >/dev/null 2>&1
cargo build --release --target x86_64-unknown-linux-musl || exit 1

BIN=target/x86_64-unknown-linux-musl/release/foton
[ -s "$BIN" ] || { echo "no binary was produced"; exit 1; }
SUM=$(sha256sum "$BIN" | cut -c1-16)
echo "fingerprint: $SUM   size: $(du -h "$BIN" | cut -f1)"

# --- 3. tell the players before taking their server away ------------------
say "warning the players"
ssh -i "$KEY" -o BatchMode=yes "$HOST" \
  "cd $DIR && python3 - \"\$(cat .rcon-pass)\" '$GRACE' '$NOTE'" <<'RCON' || true
import socket, struct, sys, time
password, grace, note = sys.argv[1], int(sys.argv[2]), sys.argv[3]
try:
    sock = socket.create_connection(("127.0.0.1", 25801), timeout=8)
except OSError:
    sys.exit(0)                      # server already down: nothing to warn
counter = 0
def send(kind, body):
    global counter
    counter += 1
    payload = struct.pack("<ii", counter, kind) + body.encode() + b"\x00\x00"
    sock.sendall(struct.pack("<i", len(payload)) + payload)
    size = struct.unpack("<i", sock.recv(4))[0]
    buf = b""
    while len(buf) < size:
        buf += sock.recv(size - len(buf))
    return buf[8:-2].decode("utf8", "replace")
if send(3, password) is None:
    sys.exit(0)
send(2, f"say [deploy] restarting in {grace}s for {note}")
for remaining in (10, 3):
    if grace > remaining:
        time.sleep(grace - remaining)
        grace = remaining
        send(2, f"say [deploy] restarting in {remaining}s")
time.sleep(grace)
send(2, "say [deploy] going down now, back in a moment")
RCON

# --- 4. ship it, checking the fingerprint at every hop --------------------
say "shipping"
scp -i "$KEY" -o BatchMode=yes -q "$BIN" "$HOST:$DIR/bin/foton" || exit 1

ssh -i "$KEY" -o BatchMode=yes "$HOST" "bash -s '$SUM' '$NAME' '$DIR'" <<'REMOTE' || exit 1
set -u
EXPECTED="$1"; NAME="$2"; DIR="$3"
cd "$DIR" || exit 1
chmod +x bin/foton

GOT=$(sha256sum bin/foton | cut -c1-16)
[ "$GOT" = "$EXPECTED" ] || { echo "uploaded binary is $GOT, expected $EXPECTED"; exit 1; }

docker build -q -t foton:test bin/ >/dev/null || exit 1
IN_IMAGE=$(docker run --rm --entrypoint sha256sum foton:test /usr/local/bin/foton | cut -c1-16)
[ "$IN_IMAGE" = "$EXPECTED" ] || { echo "image carries $IN_IMAGE, expected $EXPECTED"; exit 1; }

docker rm -f "$NAME" >/dev/null 2>&1 || true
docker run -d --name "$NAME" \
  --cpus=2 --memory=3g --memory-reservation=2g \
  --restart=unless-stopped \
  --user "$(id -u):$(id -g)" \
  -p 25800:25800/tcp -p 127.0.0.1:25801:25801/tcp \
  -v "$PWD/data:/data" \
  foton:test >/dev/null

for _ in $(seq 1 45); do ss -ltn 2>/dev/null | grep -q ":25800" && break; sleep 2; done
RUNNING=$(docker exec "$NAME" sha256sum /usr/local/bin/foton 2>/dev/null | cut -c1-16)
[ "$RUNNING" = "$EXPECTED" ] || { echo "container runs $RUNNING, expected $EXPECTED"; exit 1; }
echo "running $RUNNING   $(docker ps --filter name=$NAME --format '{{.Status}}')"
REMOTE

# --- 5. say it landed -----------------------------------------------------
say "announcing"
sleep 6
ssh -i "$KEY" -o BatchMode=yes "$HOST" \
  "cd $DIR && python3 - \"\$(cat .rcon-pass)\" '$NOTE'" <<'RCON' || true
import socket, struct, sys
password, note = sys.argv[1], sys.argv[2]
sock = socket.create_connection(("127.0.0.1", 25801), timeout=10)
counter = 0
def send(kind, body):
    global counter
    counter += 1
    payload = struct.pack("<ii", counter, kind) + body.encode() + b"\x00\x00"
    sock.sendall(struct.pack("<i", len(payload)) + payload)
    size = struct.unpack("<i", sock.recv(4))[0]
    buf = b""
    while len(buf) < size:
        buf += sock.recv(size - len(buf))
    return buf[8:-2].decode("utf8", "replace")
send(3, password)
send(2, f"say [deploy] back up: {note}")
send(2, "say [deploy] file anything odd with /bug")
RCON

say "deployed  ($SUM)"
