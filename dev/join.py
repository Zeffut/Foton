#!/usr/bin/env python3
"""Join the server for real: login, configuration, then wait for the play login.

`dev/ping.py` only proves the server answers a status ping, which the whole
configuration and play pipeline can be broken behind. This walks the states a
real client walks and fails loudly if any of them stalls, so a regression that
keeps players out of the world cannot pass unnoticed.

Used by dev/join-test.sh, which boots an offline-mode server for it.
"""

import socket
import struct
import sys
import zlib

HOST = "127.0.0.1"
PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 25566
PLAYER_NAME = "SmokeTester"
PROTOCOL_VERSION = 776
TIMEOUT_SECONDS = 30

# A server with nothing left to say should be reported, not waited on. The play
# phase is where a stall shows up, so it gets its own shorter patience.
PLAY_SILENCE_TIMEOUT_SECONDS = 20

# Packet ids, mirroring steel-registry/src/generated/vanilla_packets.rs.
S_INTENTION = 0x00

LOGIN_C_DISCONNECT = 0
LOGIN_C_HELLO = 1
LOGIN_C_FINISHED = 2
LOGIN_C_COMPRESSION = 3
LOGIN_S_HELLO = 0
LOGIN_S_ACKNOWLEDGED = 3

CONFIG_C_DISCONNECT = 2
CONFIG_C_FINISH = 3
CONFIG_C_KEEP_ALIVE = 4
CONFIG_C_PING = 5
CONFIG_C_SELECT_KNOWN_PACKS = 14
CONFIG_S_CLIENT_INFORMATION = 0
CONFIG_S_FINISH = 3
CONFIG_S_KEEP_ALIVE = 4
CONFIG_S_PONG = 5
CONFIG_S_SELECT_KNOWN_PACKS = 7

PLAY_C_LOGIN = 49
PLAY_C_DISCONNECT = 32
PLAY_C_KEEP_ALIVE = 44
PLAY_C_LEVEL_CHUNK_WITH_LIGHT = 45
PLAY_C_PLAYER_POSITION = 72
PLAY_S_ACCEPT_TELEPORTATION = 0
PLAY_S_KEEP_ALIVE = 28

# A joining client should not need anywhere near this many packets to be placed
# in the world and sent its surroundings; the bound just stops a stall from
# hanging the run until the socket times out.
MAX_PLAY_PACKETS = 4000

# Enough to prove terrain is really streaming, not just that one chunk slipped
# through.
REQUIRED_CHUNKS = 9


def varint(value):
    out = b""
    while True:
        byte = value & 0x7F
        value >>= 7
        out += bytes([byte | (0x80 if value else 0)])
        if not value:
            return out


def string(text):
    raw = text.encode("utf-8")
    return varint(len(raw)) + raw


class Connection:
    """A framed Minecraft connection that can turn compression on mid-stream."""

    def __init__(self, sock):
        self.sock = sock
        self.buffer = b""
        self.compression_threshold = -1

    def _fill(self, count):
        while len(self.buffer) < count:
            chunk = self.sock.recv(65536)
            if not chunk:
                raise EOFError("server closed the connection")
            self.buffer += chunk

    def _read_varint_from_stream(self):
        value = 0
        for index in range(5):
            self._fill(1)
            byte = self.buffer[0]
            self.buffer = self.buffer[1:]
            value |= (byte & 0x7F) << (7 * index)
            if not byte & 0x80:
                return value
        raise ValueError("varint too long")

    def send(self, packet_id, payload=b""):
        body = varint(packet_id) + payload
        if self.compression_threshold < 0:
            self.sock.sendall(varint(len(body)) + body)
            return

        if len(body) >= self.compression_threshold:
            compressed = varint(len(body)) + zlib.compress(body)
        else:
            compressed = varint(0) + body
        self.sock.sendall(varint(len(compressed)) + compressed)

    def receive(self):
        """Returns the next (packet_id, payload) pair."""
        length = self._read_varint_from_stream()
        self._fill(length)
        frame, self.buffer = self.buffer[:length], self.buffer[length:]

        if self.compression_threshold >= 0:
            uncompressed_length, frame = read_varint(frame)
            if uncompressed_length > 0:
                frame = zlib.decompress(frame)

        packet_id, payload = read_varint(frame)
        return packet_id, payload


def read_varint(data):
    """Reads one varint from `data`, returning it and the remaining bytes."""
    value = 0
    for index in range(5):
        byte = data[index]
        value |= (byte & 0x7F) << (7 * index)
        if not byte & 0x80:
            return value, data[index + 1 :]
    raise ValueError("varint too long")


def read_string(data):
    length, rest = read_varint(data)
    return rest[:length].decode("utf-8", "replace"), rest[length:]


def client_information():
    """Builds the settings packet a real client sends during configuration.

    Field order follows `SClientInformation` in steel-protocol.
    """
    return (
        string("en_us")
        + varint(8)  # view distance
        + varint(0)  # chat visibility: full
        + b"\x01"  # chat colors
        + varint(0x7F)  # displayed skin parts
        + varint(1)  # main hand: right
        + b"\x00"  # text filtering
        + b"\x01"  # allows listing
        + varint(0)  # particle status: all
    )


def fail(message):
    print(f"JOIN FAILED: {message}")
    sys.exit(1)


def run_login(connection):
    """Walks the login state and returns once the server accepts the player."""
    connection.send(
        LOGIN_S_HELLO,
        string(PLAYER_NAME) + b"\x00" * 16,
    )

    while True:
        packet_id, payload = connection.receive()
        if packet_id == LOGIN_C_COMPRESSION:
            threshold, _ = read_varint(payload)
            connection.compression_threshold = threshold
            print(f"  compression enabled at {threshold} bytes")
        elif packet_id == LOGIN_C_FINISHED:
            print("  login accepted")
            return
        elif packet_id == LOGIN_C_HELLO:
            fail("server asked for encryption; it is not in offline mode")
        elif packet_id == LOGIN_C_DISCONNECT:
            reason, _ = read_string(payload)
            fail(f"disconnected during login: {reason}")
        else:
            print(f"  (login packet {packet_id} ignored)")


def run_configuration(connection):
    """Walks the configuration state until the server hands over to play."""
    connection.send(CONFIG_S_CLIENT_INFORMATION, client_information())

    registry_packets = 0
    while True:
        packet_id, payload = connection.receive()
        if packet_id == CONFIG_C_SELECT_KNOWN_PACKS:
            # Claim no known packs, so the server sends everything itself.
            connection.send(CONFIG_S_SELECT_KNOWN_PACKS, varint(0))
        elif packet_id == CONFIG_C_FINISH:
            connection.send(CONFIG_S_FINISH)
            print(f"  configuration finished after {registry_packets} packets")
            return
        elif packet_id == CONFIG_C_KEEP_ALIVE:
            connection.send(CONFIG_S_KEEP_ALIVE, payload)
        elif packet_id == CONFIG_C_PING:
            connection.send(CONFIG_S_PONG, payload)
        elif packet_id == CONFIG_C_DISCONNECT:
            fail(f"disconnected during configuration: {payload[:200]!r}")
        else:
            registry_packets += 1


def describe(seen):
    """Renders what arrived, busiest first, so a stall says what it was doing."""
    return ", ".join(
        f"id {packet_id}x{count}"
        for packet_id, count in sorted(seen.items(), key=lambda kv: -kv[1])[:8]
    )


def run_play(connection):
    """Plays far enough to prove the player is really in a loaded world.

    Joining is not enough on its own: the server has to place the player and
    stream the terrain around them, so this waits for the position sync and for
    actual chunk data before it calls the join a success.
    """
    joined = False
    positioned = False
    chunks = 0
    seen = {}

    for _ in range(MAX_PLAY_PACKETS):
        try:
            packet_id, payload = connection.receive()
        except (OSError, EOFError) as error:
            fail(
                f"stopped receiving after {type(error).__name__}: {error} "
                f"(joined={joined}, positioned={positioned}, chunks={chunks}; "
                f"got {describe(seen)})"
            )
            return
        seen[packet_id] = seen.get(packet_id, 0) + 1

        if packet_id == PLAY_C_LOGIN:
            entity_id = struct.unpack(">i", payload[:4])[0]
            print(f"  joined the world as entity {entity_id}")
            joined = True
        elif packet_id == PLAY_C_PLAYER_POSITION:
            # Confirm the teleport, the way a real client does, or the server
            # keeps the player pending and resends it.
            teleport_id, _ = read_varint(payload)
            connection.send(PLAY_S_ACCEPT_TELEPORTATION, varint(teleport_id))
            positioned = True
        elif packet_id == PLAY_C_LEVEL_CHUNK_WITH_LIGHT:
            chunks += 1
        elif packet_id == PLAY_C_KEEP_ALIVE:
            connection.send(PLAY_S_KEEP_ALIVE, payload)
        elif packet_id == PLAY_C_DISCONNECT:
            fail(f"disconnected on join: {payload[:200]!r}")

        if joined and positioned and chunks >= REQUIRED_CHUNKS:
            print(f"  placed in the world and sent {chunks} chunks")
            return

    fail(
        f"never settled into the world "
        f"(joined={joined}, positioned={positioned}, chunks={chunks}; "
        f"got {describe(seen)})"
    )


def main():
    sock = socket.create_connection((HOST, PORT), timeout=TIMEOUT_SECONDS)
    connection = Connection(sock)

    address = HOST.encode()
    connection.send(
        S_INTENTION,
        varint(PROTOCOL_VERSION)
        + varint(len(address))
        + address
        + struct.pack(">H", PORT)
        + varint(2),  # next state: login
    )

    print("=== Login ===")
    run_login(connection)
    connection.send(LOGIN_S_ACKNOWLEDGED)

    print("=== Configuration ===")
    run_configuration(connection)

    print("=== Play ===")
    sock.settimeout(PLAY_SILENCE_TIMEOUT_SECONDS)
    run_play(connection)

    sock.close()
    print("JOIN STATUS: OK")


if __name__ == "__main__":
    try:
        main()
    except (OSError, EOFError, ValueError, IndexError) as error:
        fail(f"{type(error).__name__}: {error}")
