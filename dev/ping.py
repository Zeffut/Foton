#!/usr/bin/env python3
"""Minimal Minecraft status ping used by dev/smoke-test.sh."""
import socket, struct, json, sys

def varint(n):
    b = b''
    while True:
        x = n & 0x7F
        n >>= 7
        b += bytes([x | (0x80 if n else 0)])
        if not n:
            return b

def read_varint(sock):
    n = 0
    for i in range(5):
        byte = sock.recv(1)
        if not byte:
            raise EOFError("connection closed")
        b = byte[0]
        n |= (b & 0x7F) << (7 * i)
        if not b & 0x80:
            return n
    raise ValueError("varint too long")

HOST, PORT = "127.0.0.1", 25565
s = socket.create_connection((HOST, PORT), timeout=10)

# Handshake: next_state = 1 (status)
addr = HOST.encode()
payload = b'\x00' + varint(772) + varint(len(addr)) + addr + struct.pack('>H', PORT) + varint(1)
s.sendall(varint(len(payload)) + payload)

# Status request
s.sendall(varint(1) + b'\x00')

length = read_varint(s)
pid = read_varint(s)
jlen = read_varint(s)
data = b''
while len(data) < jlen:
    chunk = s.recv(jlen - len(data))
    if not chunk:
        break
    data += chunk
s.close()

resp = json.loads(data.decode('utf-8'))
print("=== SERVER STATUS RESPONSE ===")
print(json.dumps(resp, indent=2, ensure_ascii=False)[:1200])
print()
v = resp.get('version', {})
print(f"Version : {v.get('name')}  (protocol {v.get('protocol')})")
pl = resp.get('players', {})
print(f"Players : {pl.get('online')}/{pl.get('max')}")
print("HANDSHAKE STATUS: OK")
