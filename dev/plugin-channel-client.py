#!/usr/bin/env python3
"""Live Minecraft client for the REGISTER -> payload -> UNREGISTER contract."""

import socket
import struct
import sys
import time

import join


SERVERBOUND_CUSTOM_PAYLOAD = 22  # extracted play::S_CUSTOM_PAYLOAD, MC 26.2
CLIENTBOUND_CUSTOM_PAYLOAD = 24  # extracted play::C_CUSTOM_PAYLOAD, MC 26.2
VOICE_CHANNEL = "fixture:voice"
CLIENT_CHANNEL = "fixture:client"
RESPONSE = b"\x55\x66"


def custom_payload(connection, channel, data):
    connection.send(SERVERBOUND_CUSTOM_PAYLOAD, join.string(channel) + data)


def command(connection):
    encoded = b"channelprobe"
    connection.send(join.PLAY_S_CHAT_COMMAND, join.varint(len(encoded)) + encoded)


def receive(connection, seconds):
    """Collect plugin payloads while answering play's keepalive/control packets."""
    found = []
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        connection.sock.settimeout(max(0.1, deadline - time.monotonic()))
        try:
            packet_id, payload = connection.receive()
        except socket.timeout:
            break
        if packet_id == CLIENTBOUND_CUSTOM_PAYLOAD:
            channel, data = join.read_string(payload)
            found.append((channel, data))
        elif packet_id == join.PLAY_C_KEEP_ALIVE:
            connection.send(join.PLAY_S_KEEP_ALIVE, payload)
        elif packet_id == join.PLAY_C_CHUNK_BATCH_FINISHED:
            join.acknowledge_chunk_batch(connection)
        elif packet_id == join.PLAY_C_PLAYER_POSITION:
            teleport_id, _ = join.read_varint(payload)
            connection.send(join.PLAY_S_ACCEPT_TELEPORTATION, join.varint(teleport_id))
        elif packet_id == join.PLAY_C_DISCONNECT:
            raise AssertionError(f"server disconnected: {payload[:200]!r}")
    return found


def main():
    port = int(sys.argv[1])
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
    initial_payloads = []
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
        elif packet_id == CLIENTBOUND_CUSTOM_PAYLOAD:
            initial_payloads.append(join.read_string(payload))
        elif packet_id == join.PLAY_C_DISCONNECT:
            raise AssertionError(f"disconnected on join: {payload[:200]!r}")
        if joined and positioned and chunks >= join.REQUIRED_CHUNKS:
            break
    else:
        raise AssertionError("client did not finish joining")
    connection.send(join.PLAY_S_PLAYER_LOADED)

    command(connection)
    before = initial_payloads + receive(connection, 1)
    supported = [data for channel, data in before if channel == "minecraft:register"]
    if not any(CLIENT_CHANNEL.encode() in data.split(b"\x00") for data in supported):
        raise AssertionError(f"server did not advertise its incoming channel: {before}")
    if any(channel == VOICE_CHANNEL for channel, _ in before):
        raise AssertionError("server sent to a client that had not registered the channel")

    custom_payload(connection, "minecraft:register",
        b"fixture:voice\x00fixture:client\x00")
    after_register = receive(connection, 2)
    if (VOICE_CHANNEL, RESPONSE) not in after_register:
        raise AssertionError(f"REGISTER did not produce outgoing payload: {after_register}")

    custom_payload(connection, CLIENT_CHANNEL, b"\x33\x44")
    receive(connection, 1)
    custom_payload(connection, "minecraft:unregister", b"fixture:voice\x00")
    command(connection)
    after_unregister = receive(connection, 2)
    if any(channel == VOICE_CHANNEL for channel, _ in after_unregister):
        raise AssertionError("UNREGISTER did not suppress outgoing payload")

    print("CHANNEL PROBE: REGISTER -> payload -> UNREGISTER passed")
    sock.close()


if __name__ == "__main__":
    main()
