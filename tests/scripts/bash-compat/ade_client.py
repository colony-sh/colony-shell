"""A client for the ADE socket, for the tests.

Usage: python3 -I ade_client.py <socket> '<request json>' [<request json>...]
       python3 -I ade_client.py <socket> --subscribe <n> [<after>]

Sends each request on one connection and prints each reply line. With --subscribe, prints
the first n event lines of a subscription, then exits.
"""

import json
import socket
import sys

sock = socket.socket(socket.AF_UNIX)
sock.settimeout(20)
sock.connect(sys.argv[1])
reader = sock.makefile("r")

if sys.argv[2] == "--subscribe":
    params = {"after": int(sys.argv[4])} if len(sys.argv) > 4 else {}
    sock.sendall((json.dumps({"id": 1, "method": "subscribe", "params": params}) + "\n").encode())
    print(reader.readline(), end="", flush=True)
    for _ in range(int(sys.argv[3])):
        print(reader.readline(), end="", flush=True)
else:
    for request in sys.argv[2:]:
        sock.sendall((request + "\n").encode())
        print(reader.readline(), end="", flush=True)
