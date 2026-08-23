#!/usr/bin/env python3
"""Join the server for real: login, configuration, then wait for the play login.

`dev/ping.py` only proves the server answers a status ping, which the whole
configuration and play pipeline can be broken behind. This walks the states a
real client walks and fails loudly if any of them stalls, so a regression that
keeps players out of the world cannot pass unnoticed.

Used by dev/join-test.sh, which boots an offline-mode server for it.
"""

import os
import re
import io
import socket
import time
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

PLAY_C_ADD_ENTITY = 1
PLAY_C_CHUNK_BATCH_FINISHED = 11
PLAY_C_LOGIN = 49
PLAY_C_DISCONNECT = 32
PLAY_C_KEEP_ALIVE = 44
PLAY_C_LEVEL_CHUNK_WITH_LIGHT = 45
PLAY_C_PLAYER_POSITION = 72
PLAY_C_OPEN_SCREEN = 59
PLAY_C_UPDATE_MOB_EFFECT = 132

# Vanilla parity: `ClickType`. Only the two this script sends are named.
CLICK_PICKUP = 0
CLICK_QUICK_MOVE = 1
PLAY_C_SET_PASSENGERS = 107
PLAY_C_SYSTEM_CHAT = 121
PLAY_S_ACCEPT_TELEPORTATION = 0
PLAY_S_CHAT_COMMAND = 7
PLAY_S_SET_CARRIED_ITEM = 53
PLAY_S_CONTAINER_CLICK = 18
PLAY_S_CONTAINER_CLOSE = 19
PLAY_S_CONTAINER_SLOT_STATE_CHANGED = 20
PLAY_S_SET_BEACON = 52
PLAY_S_INTERACT = 26
PLAY_S_USE_ITEM_ON = 66
PLAY_S_USE_ITEM = 67
PLAY_S_CHUNK_BATCH_RECEIVED = 11
PLAY_S_KEEP_ALIVE = 28
PLAY_S_PLAYER_LOADED = 44

# A joining client should not need anywhere near this many packets to be placed
# in the world and sent its surroundings; the bound just stops a stall from
# hanging the run until the socket times out.
MAX_PLAY_PACKETS = 4000

# Enough to prove terrain is really streaming, not just that one chunk slipped
# through.
REQUIRED_CHUNKS = 9

# Optional: stay in the world afterwards and report what spawns nearby.
# Natural spawning needs a player present, so watching from inside is the
# only way to see it happen.
WATCH_SECONDS = int(os.environ.get("JOIN_WATCH_SECONDS", "0"))

# Optional: commands to run once in the world, separated by `;;`. An entry
# starting with `!` is a client action rather than a chat command:
# `!hotbar <slot>` selects a hotbar slot, `!useon <x> <y> <z> [face]`
# right-clicks a block face, `!useitem [yaw] [pitch]` right-clicks
# without one, `!useitemx <n> [yaw] [pitch]` does that n times in a row
# without waiting between them -- which is what makes a one-in-eight chance
# testable -- `!close` shuts whatever screen is open, and
# `!useentity <type>` / `!sneakuse <type>` right-click the last entity of
# that type to spawn, with and without sneaking, and `!spawned <type>` reports
# whether the client has ever been told one appeared -- which answers "did this
# exist at all" without depending on it still existing when the question is
# asked. Those are the only way to reach an item's `use_on` and
# `use`, which no command can do. The server
# console is a TUI and only reads a real terminal, so a scripted client is the
# only way to drive the server from a test -- and it is also the honest way,
# because it is the path a player takes. The joining player needs a permission
# group that allows them; see `groups.toml`.
JOIN_COMMANDS = [
    command.strip()
    for command in os.environ.get("JOIN_COMMANDS", "").split(";;")
    if command.strip()
]

# How long to keep reading after each command before sending the next, so the
# server has a tick or two to act on it.
COMMAND_SETTLE_SECONDS = 2.0


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
        # The container id of the screen the server last opened, so `!close`
        # can shut that exact one.
        self.open_container = None
        # The id of the last entity spawned of each type name, so `!useentity`
        # can right-click one without the test having to guess a number.
        self.entities = {}

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


# How many chunks the client claims it can take each tick. The server throttles
# the next batch on this, and a client that never answers is sent nothing more
# after the first batch -- which also means it is never told about the entities
# standing in every chunk it did not get.
DESIRED_CHUNKS_PER_TICK = 64.0


def acknowledge_chunk_batch(connection):
    connection.send(
        PLAY_S_CHUNK_BATCH_RECEIVED,
        struct.pack(">f", DESIRED_CHUNKS_PER_TICK),
    )


def read_add_entity(payload):
    """Returns `(entity id, type id)` out of a play AddEntity packet.

    The packet opens with the entity id, a raw sixteen-byte UUID, then the type;
    the position and rotation that follow are ignored.
    """
    entity_id, rest = read_varint(payload)
    entity_type, _rest = read_varint(rest[16:])
    return entity_id, entity_type


def entity_names():
    """Maps entity type ids to names by reading the generated registry.

    The ids are registration order in `vanilla_entities.rs`, which is the same
    order the server assigns, so this needs no protocol support and no extra
    dependency. A bare number is still printed if the file moves.
    """
    path = os.path.join(
        os.path.dirname(os.path.abspath(__file__)),
        "..",
        "steel-registry",
        "src",
        "generated",
        "vanilla_entities.rs",
    )
    try:
        with io.open(path, encoding="utf-8") as handle:
            source = handle.read()
    except OSError:
        return {}
    return {
        index: name
        for index, name in enumerate(
            re.findall(r'Identifier :: vanilla_static \("([a-z_]+)"\)', source)
        )
    }


ENTITY_NAMES = entity_names()


def mob_effect_names():
    """Maps mob-effect registry ids to names, the same way entity ids are read.

    The ids are registration order in `vanilla_mob_effects.rs`, which is what
    the `SetBeacon` packet and the effect packet both carry.
    """
    path = os.path.join(
        os.path.dirname(os.path.abspath(__file__)),
        "..",
        "steel-registry",
        "src",
        "generated",
        "vanilla_mob_effects.rs",
    )
    try:
        with io.open(path, encoding="utf-8") as handle:
            source = handle.read()
    except OSError:
        return {}
    return {
        index: name
        for index, name in enumerate(
            re.findall(r'Identifier :: vanilla_static \("([a-z_]+)"\)', source)
        )
    }


MOB_EFFECT_NAMES = mob_effect_names()
MOB_EFFECT_IDS = {name: index for index, name in MOB_EFFECT_NAMES.items()}


def describe_spawns(spawned):
    if not spawned:
        return "nothing"
    return ", ".join(
        f"{ENTITY_NAMES.get(entity_type, f'type {entity_type}')} x{count}"
        for entity_type, count in sorted(spawned.items(), key=lambda kv: -kv[1])
    )


def describe(seen):
    """Renders what arrived, busiest first, so a stall says what it was doing."""
    return ", ".join(
        f"id {packet_id}x{count}"
        for packet_id, count in sorted(seen.items(), key=lambda kv: -kv[1])[:8]
    )


def run_play(connection, watch_seconds=0):
    """Plays far enough to prove the player is really in a loaded world.

    Joining is not enough on its own: the server has to place the player and
    stream the terrain around them, so this waits for the position sync and for
    actual chunk data before it calls the join a success.
    """
    joined = False
    positioned = False
    chunks = 0
    seen = {}
    spawned = {}

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

        if packet_id == PLAY_C_ADD_ENTITY:
            entity_id, spawn_type = read_add_entity(payload)
            spawned[spawn_type] = spawned.get(spawn_type, 0) + 1
            name = ENTITY_NAMES.get(spawn_type)
            if name:
                connection.entities[name] = entity_id

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
        elif packet_id == PLAY_C_CHUNK_BATCH_FINISHED:
            acknowledge_chunk_batch(connection)
        elif packet_id == PLAY_C_PLAYER_POSITION:
            # A `/teleport` sends one of these, and the server holds the player
            # at the old position until it is confirmed -- which puts anything
            # they then click out of interaction range.
            teleport_id, _ = read_varint(payload)
            connection.send(PLAY_S_ACCEPT_TELEPORTATION, varint(teleport_id))
        elif packet_id == PLAY_C_OPEN_SCREEN:
            # A container that opens is a container whose behavior ran. No
            # command can right-click a block, so this is the only way to see
            # it happen.
            connection.open_container, _ = read_varint(payload)
            print("  a screen opened")
        elif packet_id == PLAY_C_UPDATE_MOB_EFFECT:
            note_mob_effect(payload)
        elif packet_id == PLAY_C_SET_PASSENGERS:
            # Who is riding what. Nothing else says a player actually boarded.
            report_passengers(payload)
        elif packet_id == PLAY_C_SYSTEM_CHAT:
            note_system_chat(payload)
        elif packet_id == PLAY_C_KEEP_ALIVE:
            connection.send(PLAY_S_KEEP_ALIVE, payload)
        elif packet_id == PLAY_C_DISCONNECT:
            fail(f"disconnected on join: {payload[:200]!r}")

        if joined and positioned and chunks >= REQUIRED_CHUNKS:
            print(f"  placed in the world and sent {chunks} chunks")
            # A real client says when it has finished loading, and the server
            # holds back interactions until it does.
            connection.send(PLAY_S_PLAYER_LOADED)
            if JOIN_COMMANDS:
                if not run_commands(connection, JOIN_COMMANDS, spawned):
                    return
                # Whatever was around the join point is not what the commands
                # were run to look at -- a teleport across worlds especially --
                # so the watch below reports only what arrives after them.
                print(f"  before the commands: {describe_spawns(spawned)}")
                spawned.clear()
            if watch_seconds > 0:
                watch_for_spawns(connection, watch_seconds, spawned)
            return

    fail(
        f"never settled into the world "
        f"(joined={joined}, positioned={positioned}, chunks={chunks}; "
        f"got {describe(seen)})"
    )


def note_system_chat(payload):
    """Prints anything the server says in chat.

    A command's own output is the only way a scripted client can read the world
    back -- `execute if block ... run tellraw @s` turns a block query into a
    line here -- so every test that checks an effect rather than a packet count
    goes through this.
    """
    # The component is NBT, and the readable part of a plain string component
    # is the string itself; pulling the printable runs out is enough to assert
    # on without a full NBT reader.
    text = bytes(byte if 32 <= byte < 127 else 0x20 for byte in payload).decode("ascii")
    words = [word for word in text.split() if len(word) >= 4]
    if words:
        print(f"  server says: {' '.join(words)}")


def pump(connection, seconds, spawned):
    """Keeps the connection alive for `seconds`, recording anything that spawns."""
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        connection.sock.settimeout(max(0.1, deadline - time.monotonic()))
        try:
            packet_id, payload = connection.receive()
        except socket.timeout:
            break
        except (OSError, EOFError):
            break

        if packet_id == PLAY_C_ADD_ENTITY:
            entity_id, spawn_type = read_add_entity(payload)
            spawned[spawn_type] = spawned.get(spawn_type, 0) + 1
            name = ENTITY_NAMES.get(spawn_type)
            if name:
                connection.entities[name] = entity_id
        elif packet_id == PLAY_C_CHUNK_BATCH_FINISHED:
            acknowledge_chunk_batch(connection)
        elif packet_id == PLAY_C_PLAYER_POSITION:
            # A `/teleport` sends one of these, and the server holds the player
            # at the old position until it is confirmed -- which puts anything
            # they then click out of interaction range.
            teleport_id, _ = read_varint(payload)
            connection.send(PLAY_S_ACCEPT_TELEPORTATION, varint(teleport_id))
        elif packet_id == PLAY_C_OPEN_SCREEN:
            # A container that opens is a container whose behavior ran. No
            # command can right-click a block, so this is the only way to see
            # it happen.
            connection.open_container, _ = read_varint(payload)
            print("  a screen opened")
        elif packet_id == PLAY_C_UPDATE_MOB_EFFECT:
            note_mob_effect(payload)
        elif packet_id == PLAY_C_SET_PASSENGERS:
            # Who is riding what. Nothing else says a player actually boarded.
            report_passengers(payload)
        elif packet_id == PLAY_C_SYSTEM_CHAT:
            note_system_chat(payload)
        elif packet_id == PLAY_C_KEEP_ALIVE:
            connection.send(PLAY_S_KEEP_ALIVE, payload)
        elif packet_id == PLAY_C_DISCONNECT:
            fail(f"disconnected: {payload[:200]!r}")
            return False
    connection.sock.settimeout(TIMEOUT_SECONDS)
    return True


# Vanilla `Direction.get3DDataValue`.
FACES = {"down": 0, "up": 1, "north": 2, "south": 3, "west": 4, "east": 5}


def packed_block_pos(x, y, z):
    """Packs a position the way the protocol carries one."""
    return ((x & 0x3FFFFFF) << 38) | ((z & 0x3FFFFFF) << 12) | (y & 0xFFF)


def send_set_carried_item(connection, slot):
    """Selects a hotbar slot, which is what decides the item in hand."""
    connection.send(PLAY_S_SET_CARRIED_ITEM, struct.pack(">h", slot))


def send_use_item_on(connection, x, y, z, face):
    """Right-clicks a block face, the way a player does.

    This is the only way to reach an item's `use_on`: a command can place a
    block but cannot use an item on one, so anything that happens through a
    player's hand -- a spawn egg, a bucket, shears -- is unverifiable without
    it.
    """
    payload = (
        varint(0)  # main hand
        + struct.pack(">q", packed_block_pos(x, y, z))
        + varint(FACES[face])
        + struct.pack(">fff", 0.5, 1.0, 0.5)  # cursor, middle of the face
        + b"\x00"  # not inside the block
        + b"\x00"  # no world border hit
        + varint(0)  # sequence
    )
    connection.send(PLAY_S_USE_ITEM_ON, payload)


def send_use_item(connection, yaw, pitch):
    """Right-clicks holding an item, without a block under the cursor.

    This is the `use` half of an item, as opposed to `use_on`: what a boat item
    or a bow does. The server does its own ray cast from the rotation given
    here, so the two angles decide where the boat lands.
    """
    payload = varint(0) + varint(0) + struct.pack(">ff", yaw, pitch)
    connection.send(PLAY_S_USE_ITEM, payload)


def run_directive(connection, directive):
    """Runs one `!`-prefixed instruction. Returns False if it is not one."""
    if not directive.startswith("!"):
        return False

    parts = directive[1:].split()
    if parts[0] == "hotbar":
        send_set_carried_item(connection, int(parts[1]))
    elif parts[0] == "useitem":
        yaw = float(parts[1]) if len(parts) > 1 else 0.0
        pitch = float(parts[2]) if len(parts) > 2 else 40.0
        send_use_item(connection, yaw, pitch)
    elif parts[0] == "useitemx":
        count = int(parts[1])
        yaw = float(parts[2]) if len(parts) > 2 else 0.0
        pitch = float(parts[3]) if len(parts) > 3 else 40.0
        for _ in range(count):
            send_use_item(connection, yaw, pitch)
        print(f"  used the held item {count} times")
    elif parts[0] == "useon":
        x, y, z = (int(part) for part in parts[1:4])
        face = parts[4] if len(parts) > 4 else "up"
        send_use_item_on(connection, x, y, z, face)
    elif parts[0] == "close":
        send_container_close(connection)
    elif parts[0] == "click":
        send_container_click(connection, int(parts[1]), CLICK_PICKUP)
        print(f"  clicked slot {parts[1]}")
    elif parts[0] == "shiftclick":
        # Repeats so a whole stack can be spread out slot by slot, which is how
        # a crafter grid gets filled: the crafter only takes one item per slot
        # until every square is level, so one shift-click is not enough.
        count = int(parts[2]) if len(parts) > 2 else 1
        for _ in range(count):
            send_container_click(connection, int(parts[1]), CLICK_QUICK_MOVE)
        print(f"  shift-clicked slot {parts[1]} {count} time(s)")
    elif parts[0] == "wait":
        # Some blocks only act on a slow beat -- a beacon recounts its pyramid
        # every four seconds -- and a settle between commands is far shorter
        # than that.
        seconds = float(parts[1])
        if not pump(connection, seconds, {}):
            fail("the connection dropped while waiting")
        print(f"  waited {seconds}s")
    elif parts[0] == "setbeacon":
        secondary = parts[2] if len(parts) > 2 else None
        send_set_beacon(connection, parts[1], secondary)
        print(f"  asked the beacon for {parts[1]} and {secondary}")
    elif parts[0] == "slotstate":
        send_slot_state_changed(connection, int(parts[1]), parts[2] == "on")
        print(f"  switched slot {parts[1]} {parts[2]}")
    elif parts[0] == "spawned":
        name = parts[1]
        if name in connection.entities:
            print(f"  the client saw a {name} spawn")
        else:
            print(f"  no {name} has spawned")
    elif parts[0] == "useentity":
        send_interact(connection, parts[1], secondary=False)
    elif parts[0] == "sneakuse":
        send_interact(connection, parts[1], secondary=True)
    else:
        fail(f"unknown directive {directive}")
    return True


def note_mob_effect(payload):
    """Prints an effect the server just gave someone.

    Nothing else says a beacon worked: the effect is server-side state, and no
    command reads it back.
    """
    _entity_id, rest = read_varint(payload)
    effect_id, rest = read_varint(rest)
    amplifier, _rest = read_varint(rest)
    name = MOB_EFFECT_NAMES.get(effect_id, f"effect {effect_id}")
    print(f"  got the effect {name} at amplifier {amplifier}")


def report_passengers(payload):
    """Prints the vehicle and its riders out of a SetPassengers packet."""
    vehicle, rest = read_varint(payload)
    count, rest = read_varint(rest)
    riders = []
    for _ in range(count):
        rider, rest = read_varint(rest)
        riders.append(rider)
    if riders:
        print(f"  entity {vehicle} is carrying {riders}")
    else:
        print(f"  entity {vehicle} is carrying nobody")


def send_interact(connection, name, secondary):
    """Right-clicks the last entity spawned of type `name`.

    An entity interaction has no command behind it, so a boat that cannot be
    boarded and a boat that can look exactly alike from the outside without
    this. `secondary` is the sneak that vanilla reads to tell "ride this" from
    "open this".
    """
    entity_id = connection.entities.get(name)
    if entity_id is None:
        fail(f"no {name} has spawned, so there is nothing to right-click")
    payload = varint(entity_id)
    payload += varint(0)  # main hand
    payload += b"\x00"  # a zero first byte is the whole of an empty LpVec3
    payload += b"\x01" if secondary else b"\x00"
    connection.send(PLAY_S_INTERACT, payload)
    verb = "sneak-right-clicked" if secondary else "right-clicked"
    print(f"  {verb} the {name} (entity {entity_id})")


def send_container_click(connection, slot, click_type, button=0):
    """Clicks a slot in the open screen.

    The state id is sent as zero. The server only compares it to decide whether
    to resend the whole screen afterwards, so a stale one costs a resync and
    never rejects the click -- which is what lets this script click without
    tracking the id.

    The changed-slots map is left empty and the carried item absent: both are
    the client's prediction of the outcome, and the server recomputes them.
    """
    if connection.open_container is None:
        fail("nothing is open to click")
    payload = (
        varint(connection.open_container)
        + varint(0)
        + struct.pack(">hb", slot, button)
        + varint(click_type)
        + varint(0)
        + b"\x00"
    )
    connection.send(PLAY_S_CONTAINER_CLICK, payload)


def send_slot_state_changed(connection, slot, enabled):
    """Switches one slot of the open screen on or off.

    Only the crafter has slots that can be switched off; nothing else answers
    this packet.
    """
    if connection.open_container is None:
        fail("nothing is open to toggle")
    payload = (
        varint(slot)
        + varint(connection.open_container)
        + (b"\x01" if enabled else b"\x00")
    )
    connection.send(PLAY_S_CONTAINER_SLOT_STATE_CHANGED, payload)


def send_set_beacon(connection, primary, secondary):
    """Picks the two effects in an open beacon menu.

    The ids are plain mob-effect registry ids -- the beacon's *data slots* use
    the same ids offset by one, which is a distinction worth keeping straight.
    """
    if connection.open_container is None:
        fail("nothing is open to set effects on")

    def holder(name):
        if name is None:
            return b"\x00"
        if name not in MOB_EFFECT_IDS:
            fail(f"unknown mob effect {name}")
        return b"\x01" + varint(MOB_EFFECT_IDS[name])

    connection.send(PLAY_S_SET_BEACON, holder(primary) + holder(secondary))


def send_container_close(connection):
    """Shuts the open screen, as pressing escape does.

    A container only stops counting a player as an opener when the client says
    it closed, so nothing that depends on that count -- a chest lid dropping, a
    trapped chest going quiet -- can be tested without sending this.
    """
    if connection.open_container is None:
        fail("nothing is open to close")
    connection.send(PLAY_S_CONTAINER_CLOSE, varint(connection.open_container))
    print("  the screen was closed")
    connection.open_container = None


def run_commands(connection, commands, spawned):
    """Runs each command as the player would type it, and lets it settle."""
    for command in commands:
        print(f"  {command}" if command.startswith("!") else f"  /{command}")
        if not run_directive(connection, command):
            encoded = command.encode()
            connection.send(PLAY_S_CHAT_COMMAND, varint(len(encoded)) + encoded)
        if not pump(connection, COMMAND_SETTLE_SECONDS, spawned):
            return False
    return True


def watch_for_spawns(connection, seconds, spawned):
    """Stays in the world and reports what the server spawns around the player.

    Natural spawning only runs near a player, so the only way to see whether it
    works is to be one. This keeps answering keep-alives so the server holds the
    connection, and counts every entity it is told about.
    """
    print(f"  watching for spawns for {seconds}s")
    deadline = time.monotonic() + seconds

    while time.monotonic() < deadline:
        try:
            packet_id, payload = connection.receive()
        except (OSError, EOFError):
            break

        if packet_id == PLAY_C_ADD_ENTITY:
            entity_id, spawn_type = read_add_entity(payload)
            spawned[spawn_type] = spawned.get(spawn_type, 0) + 1
            name = ENTITY_NAMES.get(spawn_type)
            if name:
                connection.entities[name] = entity_id
        elif packet_id == PLAY_C_CHUNK_BATCH_FINISHED:
            acknowledge_chunk_batch(connection)
        elif packet_id == PLAY_C_PLAYER_POSITION:
            # A `/teleport` sends one of these, and the server holds the player
            # at the old position until it is confirmed -- which puts anything
            # they then click out of interaction range.
            teleport_id, _ = read_varint(payload)
            connection.send(PLAY_S_ACCEPT_TELEPORTATION, varint(teleport_id))
        elif packet_id == PLAY_C_OPEN_SCREEN:
            # A container that opens is a container whose behavior ran. No
            # command can right-click a block, so this is the only way to see
            # it happen.
            connection.open_container, _ = read_varint(payload)
            print("  a screen opened")
        elif packet_id == PLAY_C_UPDATE_MOB_EFFECT:
            note_mob_effect(payload)
        elif packet_id == PLAY_C_SET_PASSENGERS:
            # Who is riding what. Nothing else says a player actually boarded.
            report_passengers(payload)
        elif packet_id == PLAY_C_SYSTEM_CHAT:
            note_system_chat(payload)
        elif packet_id == PLAY_C_KEEP_ALIVE:
            connection.send(PLAY_S_KEEP_ALIVE, payload)
        elif packet_id == PLAY_C_DISCONNECT:
            fail(f"disconnected while watching: {payload[:200]!r}")
            return

    print(f"  spawned around the player: {describe_spawns(spawned)}")


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
    run_play(connection, watch_seconds=WATCH_SECONDS)

    sock.close()
    print("JOIN STATUS: OK")


if __name__ == "__main__":
    try:
        main()
    except (OSError, EOFError, ValueError, IndexError) as error:
        fail(f"{type(error).__name__}: {error}")
