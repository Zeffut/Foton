#!/usr/bin/env python3
"""Wait until HOST:PORT accepts a TCP connection."""

import argparse
import errno
import os
import socket
import sys
import time


def process_is_alive(pid):
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True


def wait_for_tcp(host, port, timeout, pid=None):
    deadline = time.monotonic() + timeout
    while True:
        if pid is not None and not process_is_alive(pid):
            print(f"process {pid} died before {host}:{port} became ready", file=sys.stderr)
            return 2

        remaining = deadline - time.monotonic()
        if remaining <= 0:
            print(f"timeout waiting for TCP connection to {host}:{port}", file=sys.stderr)
            return 1

        try:
            addresses = socket.getaddrinfo(host, port, type=socket.SOCK_STREAM)
        except socket.gaierror:
            addresses = []

        for family, socktype, protocol, _, address in addresses:
            try:
                with socket.socket(family, socktype, protocol) as connection:
                    connection.settimeout(min(0.2, remaining))
                    connection.connect(address)
                return 0
            except OSError as error:
                if error.errno not in {
                    errno.ECONNREFUSED,
                    errno.ETIMEDOUT,
                    errno.EHOSTUNREACH,
                    errno.ENETUNREACH,
                }:
                    continue

        time.sleep(min(0.05, max(0, deadline - time.monotonic())))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("host")
    parser.add_argument("port", type=int)
    parser.add_argument("timeout", type=float)
    parser.add_argument("pid", type=int, nargs="?")
    args = parser.parse_args()
    if args.timeout < 0 or not 0 < args.port < 65536:
        parser.error("PORT must be 1..65535 and TIMEOUT_SECONDS must be non-negative")
    return wait_for_tcp(args.host, args.port, args.timeout, args.pid)


if __name__ == "__main__":
    raise SystemExit(main())
