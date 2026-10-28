#!/usr/bin/env bats
# Environment set by a parent (Colony Nerve sets BELLMAN_*) must reach programs colony starts.

COLONY="${COLONY_BIN}"

setup() {
    fakebin="$BATS_TEST_TMPDIR/bin"
    mkdir -p "$fakebin"
    # Stand-in for `claude`: prints what the Bellman SessionStart hook would read.
    printf '#!/bin/sh\necho "$BELLMAN_JOIN_CODE|$BELLMAN_ROOM|$BELLMAN_ROLE|$BELLMAN_DOOR_ROOT"\n' > "$fakebin/claude"
    chmod +x "$fakebin/claude"
    export PATH="$fakebin:$PATH"
    export BELLMAN_JOIN_CODE="BELL-7F3K-92-PEER-B"
    export BELLMAN_ROOM="qs_test-room"
    export BELLMAN_ROLE="editor"
    export BELLMAN_DOOR_ROOT="$BATS_TEST_TMPDIR/doors/widget 1"
}

expected() {
    echo "BELL-7F3K-92-PEER-B|qs_test-room|editor|$BATS_TEST_TMPDIR/doors/widget 1"
}

@test "BELLMAN_* reach a child started with -c" {
    run "$COLONY" -c 'claude'
    [ "$status" -eq 0 ]
    [ "$output" = "$(expected)" ]
}

@test "BELLMAN_* reach a child started from a script" {
    printf 'claude\n' > "$BATS_TEST_TMPDIR/s.sh"
    run "$COLONY" "$BATS_TEST_TMPDIR/s.sh"
    [ "$status" -eq 0 ]
    [ "$output" = "$(expected)" ]
}

@test "BELLMAN_* reach a child in a pipeline and a subshell" {
    run "$COLONY" -c 'claude | cat; (claude)'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "$(expected)" ]
    [ "${lines[1]}" = "$(expected)" ]
}
