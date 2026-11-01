"""A fake Bellman bridge door for the canvas/draw BATS tests.

Serves GET /status, /rooms, /rooms/<id>/surface and PUT/DELETE /rooms/<id>/surface/<key> on
<root>/<pid>.sock, keeping the canvas in memory. A PUT that carries a field outside Bellman's
strict item schema, or a key starting with "refuse", gets the bridge's 422.

Usage: python3 -I fake_door.py <root> [several]   ("several": two seats in the room, writes 409)
"""
import http.server
import json
import os
import socketserver
import sys

ROOM = "qs_test-room"
SEVERAL = len(sys.argv) > 2 and sys.argv[2] == "several"
WRITABLE = {"key", "kind", "title", "body", "ends", "placement", "blob", "shape", "frame", "locked"}
cursor = 2
items = {
    "plan": {"origin": "m_other", "data": {"key": "plan", "kind": "text", "title": "The plan", "body": "b",
                                            "placement": {"x": 10, "y": 20, "w": 300, "h": 400}}},
}
ROOMS = {"rooms": [{"session_id": ROOM, "members": [
    {"member_id": "m_1", "label": "ada", "agent": None, "room_role": "planner", "status": "idle", "active": True}]}]}


def surface():
    return {"session_id": ROOM,
            "room": {"text": {"trust": "untrusted", "data": {"room": "planning"}}},
            "surface": {"cursor": cursor, "items": [
                {"trust": "untrusted", "origin": {"memberId": i["origin"], "label": "x"},
                 # What a read adds, which a write must not send back.
                 "data": {**i["data"], "cursor": cursor, "at": "2026-10-29T00:00:00Z", "approved_by": None}}
                for i in items.values()]}}


class Door(http.server.BaseHTTPRequestHandler):
    def reply(self, status, body):
        data = json.dumps(body).encode()
        self.send_response(status)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        seats = [{"session_id": ROOM, "member_id": "m_me", "retrying": False}]
        if SEVERAL:
            seats.append({"session_id": ROOM, "member_id": "m_two", "retrying": False})
        routes = {"/status": {"version": 1, "seats": seats}, "/rooms": ROOMS,
                  f"/rooms/{ROOM}/surface": surface()}
        body = routes.get(self.path)
        self.reply(200 if body else 404, body or {"error": "not_found"})

    def write(self):
        global cursor
        prefix = f"/rooms/{ROOM}/surface/"
        if not self.path.startswith(prefix):
            return self.reply(404, {"error": "not_found"})
        if SEVERAL:
            return self.reply(409, {"error": "several_seats"})
        key = self.path[len(prefix):]
        if self.command == "DELETE":
            items.pop(key, None)
        else:
            body = json.loads(self.rfile.read(int(self.headers["content-length"])))
            if key.startswith("refuse") or set(body) - WRITABLE:
                return self.reply(422, {"error": "refused"})
            items[key] = {"origin": "m_me", "data": {**body, "key": key}}
        cursor += 1
        self.reply(200, {"cursor": cursor})

    do_PUT = write
    do_DELETE = write

    def log_message(self, format, *args):
        pass

    def address_string(self):
        return "door"


socketserver.UnixStreamServer(os.path.join(sys.argv[1], f"{os.getpid()}.sock"), Door).serve_forever()
