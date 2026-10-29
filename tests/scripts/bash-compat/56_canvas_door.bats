#!/usr/bin/env bats
# `canvas` reads a Bellman room through the bridge door, here a fake door on a Unix socket.

COLONY="${COLONY_BIN}"

setup() {
    command -v python3 >/dev/null || skip "python3 not installed"
    # Short path: a Unix socket path is at most 104 bytes.
    door_root="$(mktemp -d /tmp/door.XXXXXX)"
    cat > "$BATS_TEST_TMPDIR/door.py" <<'EOF'
import http.server, json, os, socketserver, sys

ROOM = "qs_test-room"
SURFACE = {"session_id": ROOM,
           "room": {"text": {"trust": "untrusted", "data": {"room": "planning"}}},
           "surface": {"cursor": 2, "items": [
               {"trust": "untrusted", "data": {"key": "plan", "kind": "text", "title": "The plan",
                                               "placement": {"x": 10, "y": 20, "w": 300, "h": 400}}}]}}
ROOMS = {"rooms": [{"session_id": ROOM, "members": [
    {"member_id": "m_1", "label": "ada", "agent": None, "room_role": "planner", "status": "idle", "active": True}]}]}

class Door(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        body = {"/rooms": ROOMS, f"/rooms/{ROOM}/surface": SURFACE}.get(self.path)
        data = json.dumps(body or {"error": "not_found"}).encode()
        self.send_response(200 if body else 404)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)
    def log_message(self, *args):
        pass
    def address_string(self):
        return "door"

socketserver.UnixStreamServer(os.path.join(sys.argv[1], f"{os.getpid()}.sock"), Door).serve_forever()
EOF
    python3 -I "$BATS_TEST_TMPDIR/door.py" "$door_root" &
    door_pid=$!
    for _ in $(seq 50); do [ -S "$door_root/$door_pid.sock" ] && break; sleep 0.1; done
    export BELLMAN_DOOR_ROOT="$door_root"
    export BELLMAN_ROOM="qs_test-room"
}

teardown() {
    kill "$door_pid" 2>/dev/null || true
    rm -rf "$door_root"
}

@test "canvas status, participants and widgets read the door" {
    run "$COLONY" -c 'canvas status'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "Room: planning (qs_test-room)" ]
    [ "${lines[1]}" = "Items: 1" ]

    run "$COLONY" -c 'canvas participants'
    [ "$status" -eq 0 ]
    [[ "$output" == HUMAN*planner*idle*"ada (m_1)" ]]

    run "$COLONY" -c 'canvas widgets'
    [ "$status" -eq 0 ]
    [[ "${lines[0]}" == KEY* ]]
    [[ "${lines[1]}" == plan*text*10,20*300x400*"The plan" ]]
}

@test "canvas says why it cannot read" {
    run env -u BELLMAN_ROOM "$COLONY" -c 'canvas widgets'
    [ "$status" -eq 1 ]
    [[ "$output" == *"not in a Bellman room"* ]]

    BELLMAN_ROOM=qs_other run "$COLONY" -c 'canvas widgets'
    [ "$status" -eq 1 ]
    [[ "$output" == *"holds no seat in room qs_other"* ]]

    BELLMAN_ROOM=qs_other run "$COLONY" -c 'canvas participants'
    [ "$status" -eq 1 ]
    [[ "$output" == *"holds no seat in room qs_other"* ]]

    BELLMAN_DOOR_ROOT="$BATS_TEST_TMPDIR/empty" run "$COLONY" -c 'canvas status'
    [ "$status" -eq 1 ]
    [[ "$output" == *"no Bellman bridge door found"* ]]
}
