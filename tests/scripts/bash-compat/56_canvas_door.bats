#!/usr/bin/env bats
# `canvas` reads a Bellman room through the bridge door, here a fake door on a Unix socket.

COLONY="${COLONY_BIN}"

setup() {
    command -v python3 >/dev/null || skip "python3 not installed"
    # Short path: a Unix socket path is at most 104 bytes.
    door_root="$(mktemp -d /tmp/door.XXXXXX)"
    python3 -I "$BATS_TEST_DIRNAME/fake_door.py" "$door_root" &
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
