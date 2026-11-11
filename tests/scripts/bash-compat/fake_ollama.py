"""A fake Ollama server for the heartbeat tests. Nothing leaves the machine.

Usage: python3 -I fake_ollama.py <dir>

Listens on 127.0.0.1 on a free port and writes the port to <dir>/port. Each /api/chat
request body is appended to <dir>/requests.jsonl. The reply is the text in <dir>/reply
(default "PASS: ok"), streamed as Ollama NDJSON. If <dir>/delay exists, it waits that many
seconds before answering.
"""

import json
import os
import sys
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

DIR = sys.argv[1]


def read(name, default):
    try:
        with open(os.path.join(DIR, name)) as f:
            return f.read()
    except FileNotFoundError:
        return default


class Handler(BaseHTTPRequestHandler):
    def log_message(self, format, *args):  # noqa: A002 - the base class's name
        pass

    def send_json_lines(self, lines):
        body = "".join(json.dumps(line) + "\n" for line in lines).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/x-ndjson")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path == "/api/tags":
            self.send_json_lines([{"models": [{"name": "fake"}]}])
        else:
            self.send_error(404)

    def do_POST(self):
        if self.path != "/api/chat":
            self.send_error(404)
            return
        request = self.rfile.read(int(self.headers.get("Content-Length", 0)))
        with open(os.path.join(DIR, "requests.jsonl"), "ab") as f:
            f.write(request.strip() + b"\n")
        time.sleep(float(read("delay", "0")))
        reply = read("reply", "PASS: ok")
        self.send_json_lines(
            [{"message": {"role": "assistant", "content": reply}, "done": False}, {"done": True}]
        )


server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
with open(os.path.join(DIR, "port.tmp"), "w") as f:
    f.write(str(server.server_address[1]))
os.rename(os.path.join(DIR, "port.tmp"), os.path.join(DIR, "port"))
server.serve_forever()
