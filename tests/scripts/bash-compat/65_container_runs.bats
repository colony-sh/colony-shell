#!/usr/bin/env bats
# Runs in a container: commands execute there through Colony's shell, writes stay in the
# container, only granted hosts are reachable, and a blocked call is recorded.

COLONY="${COLONY_BIN}"
IMAGE=localhost/colony-run:latest

setup() {
    command -v python3 >/dev/null || skip "python3 not installed"
    command -v sqlite3 >/dev/null || skip "sqlite3 not installed"
    command -v podman >/dev/null || skip "podman not installed"
    podman image exists "$IMAGE" 2>/dev/null || skip "$IMAGE not built (scripts/build-run-image.sh)"
    [ -x "$(dirname "$COLONY")/colony-events" ] || skip "colony-events not built next to colony"
    work="$(mktemp -d "$BATS_TEST_DIRNAME/../../../target/bats-XXXXXX")"
    work="$(cd "$work" && pwd -P)"
    mkdir -p "$work/home" "$work/project" "$work/ollama"
    # The tests' HOME hides podman's connection to its VM: run podman with the real one.
    printf '#!/bin/sh\nHOME=%q exec %q "$@"\n' "$HOME" "$(command -v podman)" > "$work/podman"
    chmod +x "$work/podman"
    export COLONY_PODMAN="$work/podman"
    printf 'old\n' > "$work/project/notes.txt"
    python3 -I "$BATS_TEST_DIRNAME/fake_ollama.py" "$work/ollama" &
    ollama_pid=$!
    for _ in $(seq 50); do [ -f "$work/ollama/port" ] && break; sleep 0.1; done
    export HOME="$work/home"
    export OLLAMA_HOST="127.0.0.1:$(cat "$work/ollama/port")"
    export COLONY_MODEL_PROVIDER=ollama
    export COLONY_MODEL_NAME=fake
    export COLONY_VIEW_ADDR=127.0.0.1:0
    unset ANTHROPIC_API_KEY OPENAI_API_KEY COLONY_API_KEY
    cd "$work/project"
}

teardown() {
    local pid
    pid="$(cat "$HOME/.colony/events/router.pid" 2>/dev/null)" && kill "$pid" 2>/dev/null
    for _ in $(seq 50); do [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null || break; sleep 0.1; done
    kill "$ollama_pid" 2>/dev/null || true
    podman rm -f -t 0 colony-lead-miner-1 colony-lead-miner-1-proxy >/dev/null 2>&1 || true
    podman network rm -f colony-lead-miner-1-net >/dev/null 2>&1 || true
    rm -rf "$work"
}

q() { sqlite3 "$HOME/.config/colony/tuple-space.db" "$1"; }

wait_for() {
    for _ in $(seq 300); do [ "$(q "SELECT COUNT(*) FROM events WHERE kind = '$1'")" -ge "$2" ] && return 0; sleep 0.1; done
    return 1
}

# One beat whose model runs $1 in the shell tool, then answers $2.
one_beat() {
    python3 -I -c 'import json, sys; print(json.dumps([{"tool_calls": [{"name": "shell", "arguments": {"command": sys.argv[1]}}]}, sys.argv[2]]))' "$1" "$2" > "$work/ollama/script"
    shift 2
    "$COLONY" -c "agent new lead-miner --every 2s --image run $* --charter 'Find threads and draft replies.'"
    wait_for HeartbeatAnswered 1
    "$COLONY" -c 'agent pause lead-miner' >/dev/null
}

@test "a run's commands execute in its container, and its writes stay there as a diff" {
    one_beat 'echo new > notes.txt; echo drafted > draft.md; uname -s; pwd; echo "sandbox=$COLONY_SANDBOX"' \
        $'HEADLINE: Drafted one reply\nWARN: one draft in draft.md'

    run q "SELECT json_extract(data, '$.content') FROM events WHERE subject = 'colony://local/agent/lead-miner/run/1/output/2'"
    [[ "$output" == *Linux* ]]
    [[ "$output" == */work* ]]
    [[ "$output" == *sandbox=1* ]]

    # Nothing outside the container changed.
    [ "$(cat "$work/project/notes.txt")" = "old" ]
    [ ! -e "$work/project/draft.md" ]

    # The diff is recorded, is evidence, and opens.
    run "$COLONY" -c 'colony open colony://local/agent/lead-miner/run/1/diff | cat'
    [ "$status" -eq 0 ]
    [[ "$output" == *"-old"* ]]
    [[ "$output" == *"+new"* ]]
    [[ "$output" == *"+drafted"* ]]
    run q "SELECT json_extract(data, '$.evidence') FROM events WHERE kind = 'ReportPosted'"
    [[ "$output" == *'colony://local/agent/lead-miner/run/1/diff'* ]]

    # The container came and went, with the log saying so.
    run q "SELECT kind FROM events WHERE kind LIKE 'Container%' AND subject = 'colony://local/agent/lead-miner/run/1' ORDER BY seq"
    [ "${lines[0]}" = "ContainerStarted" ]
    [ "${lines[1]}" = "ContainerRemoved" ]
    [ -z "$(podman ps -aq --filter name=colony-lead-miner-1)" ]
    ! podman network exists colony-lead-miner-1-net
}

@test "a call to a host that was not granted is blocked and recorded; there is no other way out" {
    one_beat 'curl -sS -m 5 -o /dev/null https://example.com/; echo "proxied=$?"; curl -sS -m 3 --noproxy "*" -o /dev/null http://192.0.2.1/; echo "direct=$?"' \
        'WARN: could not reach example.com' --allow-host news.ycombinator.com

    run q "SELECT json_extract(data, '$.host') || ':' || json_extract(data, '$.port') FROM events WHERE kind = 'NetworkBlocked'"
    [ "$output" = "example.com:443" ]
    run q "SELECT json_extract(data, '$.content') FROM events WHERE subject = 'colony://local/agent/lead-miner/run/1/output/2'"
    [[ "$output" == *proxied=56* ]]
    [[ "$output" != *direct=0* ]]
    run q "SELECT json_extract(data, '$.grants[0]') FROM events WHERE kind = 'ContainerStarted'"
    [ "$output" = "news.ycombinator.com" ]
    # The block is evidence in the report.
    seq="$(q "SELECT seq FROM events WHERE kind = 'NetworkBlocked'")"
    run q "SELECT json_extract(data, '$.evidence') FROM events WHERE kind = 'ReportPosted'"
    [[ "$output" == *"colony://local/event/$seq"* ]]
}

@test "a container that does not start fails the run; it never runs on this machine" {
    printf 'PASS: ran' > "$work/ollama/reply"
    "$COLONY" -c "agent new lead-miner --every 2s --image localhost/colony-missing:1 --charter 'Look.'"
    wait_for HeartbeatAnswered 1
    "$COLONY" -c 'agent pause lead-miner' >/dev/null
    run q "SELECT json_extract(data, '$.outcome') || '|' || json_extract(data, '$.error') FROM events WHERE kind = 'HeartbeatAnswered'"
    [[ "$output" == "missed|"*"container did not start"* ]]
    [ ! -s "$work/ollama/requests.jsonl" ]
    run q "SELECT json_extract(data, '$.status') FROM events WHERE kind = 'ReportPosted'"
    [ "$output" = "blocked" ]
}

@test "agent new checks its grants" {
    run "$COLONY" -c 'agent new a --every 1h --charter x --allow-host example.com'
    [ "$status" -eq 1 ]
    [[ "$output" == *"--allow-host needs --image"* ]]
    run "$COLONY" -c 'agent new a --every 1h --charter x --image run --allow-host http://x.com'
    [ "$status" -eq 1 ]
    [[ "$output" == *"bad host 'http://x.com'"* ]]
}
