#!/usr/bin/env python3
"""A scripted Source RCON client, used by dev/rcon-test.sh.

Speaks the wire format a real client speaks -- little-endian length, request
id and type, a NUL-terminated body and one more NUL -- so what this proves is
what mcrcon or a control panel would see, not what Foton believes it sent.

The transcript it prints is one line per event so a shell can assert on it:

    RCON_EVENT <tag> id=<request id> kind=<type> body=<python repr>

`repr` rather than the raw text because command output contains newlines, and
an assertion that only matches when the answer happens to be one line is not
an assertion about anything.

Usage: python3 dev/rcon.py <port> <password>
"""

import socket
import struct
import sys

SERVERDATA_AUTH = 3
SERVERDATA_EXECCOMMAND = 2

CONNECT_TIMEOUT_SECONDS = 15
REPLY_TIMEOUT_SECONDS = 30


def send(sock, request_id, kind, body):
    payload = body.encode("utf-8")
    sock.sendall(struct.pack("<iii", len(payload) + 10, request_id, kind) + payload + b"\x00\x00")


def recv_exact(sock, count):
    chunks = []
    remaining = count
    while remaining > 0:
        chunk = sock.recv(remaining)
        if not chunk:
            raise EOFError("the server closed the connection")
        chunks.append(chunk)
        remaining -= len(chunk)
    return b"".join(chunks)


def recv(sock):
    length = struct.unpack("<i", recv_exact(sock, 4))[0]
    contents = recv_exact(sock, length)
    request_id, kind = struct.unpack("<ii", contents[:8])
    body = contents[8:].split(b"\x00")[0].decode("utf-8", "replace")
    return request_id, kind, body


def event(tag, request_id, kind, body):
    print(f"RCON_EVENT {tag} id={request_id} kind={kind} body={body!r}", flush=True)


def connect(port):
    sock = socket.create_connection(("127.0.0.1", port), CONNECT_TIMEOUT_SECONDS)
    sock.settimeout(REPLY_TIMEOUT_SECONDS)
    return sock


def exchange(sock, tag, request_id, kind, body):
    send(sock, request_id, kind, body)
    reply_id, reply_kind, reply_body = recv(sock)
    event(tag, reply_id, reply_kind, reply_body)
    return reply_body


def main():
    port = int(sys.argv[1])
    password = sys.argv[2]

    # A command before the password. The answer has to be the auth failure,
    # never the command's output.
    with connect(port) as sock:
        exchange(sock, "preauth", 11, SERVERDATA_EXECCOMMAND, "seed")

    # The wrong password. Vanilla answers request id -1, and a client reads
    # that -1 rather than the body to know it was rejected.
    with connect(port) as sock:
        exchange(sock, "badpass", 12, SERVERDATA_AUTH, password + "-wrong")

    # An empty password, which must never open anything.
    with connect(port) as sock:
        exchange(sock, "emptypass", 13, SERVERDATA_AUTH, "")

    with connect(port) as sock:
        exchange(sock, "auth", 4711, SERVERDATA_AUTH, password)
        # Something with a value in it that survives any translation.
        exchange(sock, "seed", 20, SERVERDATA_EXECCOMMAND, "seed")
        # A command that fails: its error is output too, and dropping it would
        # leave an administrator unable to tell a typo from a silent success.
        exchange(sock, "bogus", 21, SERVERDATA_EXECCOMMAND, "definitelynotacommand")
        # Two in a row on one connection, to show the second is answered.
        exchange(sock, "time", 22, SERVERDATA_EXECCOMMAND, "time set noon")
        exchange(sock, "list", 23, SERVERDATA_EXECCOMMAND, "list")

    print("RCON_DONE", flush=True)


if __name__ == "__main__":
    main()
