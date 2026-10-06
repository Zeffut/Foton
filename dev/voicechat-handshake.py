#!/usr/bin/env python3
"""A Simple Voice Chat client's handshake, as far as the Minecraft server is in it.

The voice itself travels over UDP, between the client mod and the plugin's own
Java socket; Foton is not on that path. What Foton carries is the start: the
client registers the plugin's channels and asks for a secret, and the plugin
answers with the secret and the UDP port to use. If that answer never arrives
the client mod shows voice chat as unavailable, so this is the part to prove.

Usage: python3 dev/voicechat-handshake.py <port> [compatibility-version]

The compatibility version defaults to 20, the one Simple Voice Chat 2.6.x
speaks (BuildConstants.COMPATIBILITY_VERSION).
"""

import importlib.util
import pathlib
import socket
import struct
import sys
import time
import uuid

import join

_client_path = pathlib.Path(__file__).with_name("plugin-channel-client.py")
_spec = importlib.util.spec_from_file_location("plugin_channel_client", _client_path)
channels = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(channels)

CHANNELS = [
    "voicechat:secret",
    "voicechat:player_state",
    "voicechat:player_states",
    "voicechat:joined_group",
    "voicechat:add_group",
    "voicechat:remove_group",
    "voicechat:add_category",
    "voicechat:remove_category",
]
CODECS = ["VOIP", "AUDIO", "RESTRICTED_LOWDELAY"]


def join_play(port):
    """Logs in and waits until the player stands in the world."""
    sock = socket.create_connection((join.HOST, port), timeout=join.TIMEOUT_SECONDS)
    connection = join.Connection(sock)
    address = join.HOST.encode()
    connection.send(join.S_INTENTION,
        join.varint(join.PROTOCOL_VERSION) + join.varint(len(address)) + address
        + struct.pack(">H", port) + join.varint(2))
    join.run_login(connection)
    connection.send(join.LOGIN_S_ACKNOWLEDGED)
    join.run_configuration(connection)

    joined = positioned = False
    chunks = 0
    sock.settimeout(join.PLAY_SILENCE_TIMEOUT_SECONDS)
    for _ in range(join.MAX_PLAY_PACKETS):
        packet_id, payload = connection.receive()
        if packet_id == join.PLAY_C_LOGIN:
            joined = True
        elif packet_id == join.PLAY_C_PLAYER_POSITION:
            teleport_id, _ = join.read_varint(payload)
            connection.send(join.PLAY_S_ACCEPT_TELEPORTATION, join.varint(teleport_id))
            positioned = True
        elif packet_id == join.PLAY_C_LEVEL_CHUNK_WITH_LIGHT:
            chunks += 1
        elif packet_id == join.PLAY_C_CHUNK_BATCH_FINISHED:
            join.acknowledge_chunk_batch(connection)
        elif packet_id == join.PLAY_C_KEEP_ALIVE:
            connection.send(join.PLAY_S_KEEP_ALIVE, payload)
        elif packet_id == join.PLAY_C_DISCONNECT:
            raise AssertionError(f"disconnected on join: {payload[:200]!r}")
        if joined and positioned and chunks >= join.REQUIRED_CHUNKS:
            break
    else:
        raise AssertionError("client did not finish joining")
    connection.send(join.PLAY_S_PLAYER_LOADED)
    return connection


def read_uuid(data):
    return uuid.UUID(bytes=data[:16]), data[16:]


def decode_secret(data):
    """Simple Voice Chat's SecretPacket.toBytes, field by field."""
    secret, data = read_uuid(data)
    port, = struct.unpack(">i", data[:4])
    player, data = read_uuid(data[4:])
    codec = data[0]
    mtu, distance, keep_alive = struct.unpack(">idi", data[1:17])
    groups = data[17] != 0
    host, data = join.read_string(data[18:])
    recording = data[0] != 0
    return {
        "secret": str(secret), "port": port, "player": str(player),
        "codec": CODECS[codec] if codec < len(CODECS) else codec, "mtu": mtu,
        "distance": distance, "keep_alive": keep_alive, "groups": groups,
        "host": host.decode() if isinstance(host, bytes) else host, "recording": recording,
    }


def main():
    port = int(sys.argv[1])
    version = int(sys.argv[2]) if len(sys.argv) > 2 else 20
    connection = join_play(port)
    channels.custom_payload(connection, "minecraft:register",
        b"\x00".join(name.encode() for name in CHANNELS) + b"\x00")
    channels.custom_payload(connection, "voicechat:request_secret", struct.pack(">i", version))

    deadline = time.monotonic() + 5
    secret = None
    seen = []
    while secret is None and time.monotonic() < deadline:
        for channel, data in channels.receive(connection, 1):
            seen.append(channel)
            if channel == "voicechat:secret":
                secret = decode_secret(data)
    if secret is None:
        raise AssertionError(f"no voicechat:secret answered the request; saw {seen}")
    for key, value in secret.items():
        print(f"secret {key} = {value}")
    print("VOICECHAT HANDSHAKE: secret received")
    connection.sock.close()


if __name__ == "__main__":
    main()
