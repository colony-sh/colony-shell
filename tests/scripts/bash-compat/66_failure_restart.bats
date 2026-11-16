#!/usr/bin/env bats
# Failure handling and restart: a run that dies or overruns is noticed, cleaned up and
# reported, the next beat fires normally, and a restart loses nothing.

COLONY="${COLONY_BIN}"
IMAGE=localhost/colony-run:latest

setup() {
    command -v python3 >/dev/null || skip "python3 not installed"
    command -v sqlite3 >/dev/null || skip "sqlite3 not installed"
    command -v curl >/dev/null || skip "curl not installed"
    [ -x "$(dirname "$COLONY")/colony-events" ] || skip "colony-events not built next to colony"
    work="$(mktemp -d "$BATS_TEST_DIRNAME/../../../target/bats-XXXXXX")"
    work="$(cd "$work" && pwd -P)"
    mkdir -p "$work/home" "$work/project" "$work/ollama"
    if command -v podman >/dev/null; then
        # The tests' HOME hides podman's connection to its VM: run podman with the real one.
        printf '#!/bin/sh\nHOME=%q exec %q "$@"\n' "$HOME" "$(command -v podman)" > "$work/podman"
        chmod +x "$work/podman"
        export COLONY_PODMAN="$work/podman"
    fi
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
    stop_daemon
    kill "$ollama_pid" 2>/dev/null || true
    if [ -n "${COLONY_PODMAN:-}" ]; then
        container="$(q "SELECT json_extract(data, '$.container') FROM events WHERE kind = 'ContainerStarted'" 2>/dev/null | sort -u)"
        for c in $container; do
            "$COLONY_PODMAN" rm -f -t 0 "$c" "$c-proxy" >/dev/null 2>&1 || true
            "$COLONY_PODMAN" network rm -f "$c-net" >/dev/null 2>&1 || true
        done
    fi
    rm -rf "$work"
}

need_podman() {
    [ -n "${COLONY_PODMAN:-}" ] && "$COLONY_PODMAN" image exists "$IMAGE" 2>/dev/null || skip "$IMAGE not built"
}

stop_daemon() {
    local pid
    pid="$(cat "$HOME/.colony/events/router.pid" 2>/dev/null)" || return 0
    kill "$pid" 2>/dev/null || true
    for _ in $(seq 50); do kill -0 "$pid" 2>/dev/null || return 0; sleep 0.1; done
}

q() { sqlite3 "$HOME/.config/colony/tuple-space.db" "$1"; }

count() { q "SELECT COUNT(*) FROM events WHERE kind = '$1'"; }

wait_for() {
    for _ in $(seq "${3:-300}"); do [ "$(count "$1")" -ge "$2" ] && return 0; sleep 0.1; done
    return 1
}

answer_of() {
    q "SELECT json_extract(data, '$.outcome') || '|' || COALESCE(json_extract(data, '$.status'), '') || '|' || COALESCE(json_extract(data, '$.error'), '') FROM events WHERE kind IN ('HeartbeatAnswered', 'RunResumed') ORDER BY seq LIMIT 1 OFFSET $(($1 - 1))"
}

state() { q "SELECT json_extract(data, '$.to') FROM events WHERE kind = 'AgentStateChanged' ORDER BY seq DESC LIMIT 1"; }

container() { q "SELECT json_extract(data, '$.container') FROM events WHERE kind = 'ContainerStarted' LIMIT 1"; }

@test "a daemon killed mid-run is noticed on restart: the run answers missed, the container goes, the next beat fires" {
    need_podman
    printf '30' > "$work/ollama/delay"
    "$COLONY" -c "agent new lead-miner --every 2s --image run --charter 'Look.'"
    wait_for ContainerStarted 1
    for _ in $(seq 100); do [ -s "$work/ollama/requests.jsonl" ] && break; sleep 0.1; done
    c="$(container)"
    kill -9 "$(cat "$HOME/.colony/events/router.pid")"
    "$COLONY_PODMAN" container exists "$c"
    [ "$(state)" = "running" ]

    rm "$work/ollama/delay"
    printf 'PASS: nothing new' > "$work/ollama/reply"
    "$COLONY" -c 'on --daemon start' 2>/dev/null
    wait_for HeartbeatAnswered 1
    [[ "$(answer_of 1)" == "missed|error|the run died: the daemon stopped while it was running" ]]
    run q "SELECT json_extract(data, '$.status') FROM events WHERE kind = 'ReportPosted'"
    [ "$output" = "blocked" ]
    [ "$(count ContainerRemoved)" -eq 1 ]
    ! "$COLONY_PODMAN" container exists "$c"
    ! "$COLONY_PODMAN" container exists "$c-proxy"
    ! "$COLONY_PODMAN" network exists "$c-net"

    # The next beat fires normally.
    wait_for HeartbeatAnswered 2
    [[ "$(answer_of 2)" == "idle|pass|" ]]
}

@test "a run whose container is killed is stopped at once, answers missed, and the next beat fires" {
    need_podman
    printf '30' > "$work/ollama/delay"
    "$COLONY" -c "agent new lead-miner --every 2s --image run --charter 'Look.'"
    wait_for ContainerStarted 1
    for _ in $(seq 100); do [ -s "$work/ollama/requests.jsonl" ] && break; sleep 0.1; done
    rm "$work/ollama/delay"
    printf 'PASS: nothing new' > "$work/ollama/reply"
    "$COLONY_PODMAN" kill "$(container)" >/dev/null

    wait_for HeartbeatAnswered 1 100
    [[ "$(answer_of 1)" == "missed|error|"*"container stopped mid-run"* ]]
    [ "$(count ContainerRemoved)" -ge 1 ]
    run q "SELECT json_extract(data, '$.status') FROM events WHERE kind = 'ReportPosted' LIMIT 1"
    [ "$output" = "blocked" ]
    wait_for HeartbeatAnswered 2
    [[ "$(answer_of 2)" == "idle|pass|" ]]
}

@test "a run past its time budget is stopped, answers missed, and posts a blocked report" {
    printf '5' > "$work/ollama/delay"
    "$COLONY" -c "agent new lead-miner --every 1s --timeout 1s --charter 'Look.'"
    wait_for HeartbeatAnswered 1 100
    "$COLONY" -c 'agent pause lead-miner' >/dev/null
    [[ "$(answer_of 1)" == "missed|timeout|" ]]
    run q "SELECT json_extract(data, '$.status') || '|' || json_extract(data, '$.headline') FROM events WHERE kind = 'ReportPosted'"
    [ "$output" = "blocked|stopped: no answer within 1s" ]
}

@test "a run past its token budget is stopped, answers missed, and posts a blocked report" {
    python3 -I -c 'print("WARN: " + "x" * 600, end="")' > "$work/ollama/reply"
    "$COLONY" -c "agent new lead-miner --every 1s --run-tokens 10 --charter 'Look.'"
    wait_for HeartbeatAnswered 1
    "$COLONY" -c 'agent pause lead-miner' >/dev/null
    [[ "$(answer_of 1)" == "missed|error|" ]]
    run q "SELECT json_extract(data, '$.status') || '|' || json_extract(data, '$.body') FROM events WHERE kind = 'ReportPosted' LIMIT 1"
    [[ "$output" == "blocked|"*"[stopped: token budget of 10 exceeded]" ]]
}

@test "a restart keeps a parked ask: no beats fire, and an answer from the viewport still resumes the run" {
    printf 'HEADLINE: Drafted a reply\nASK: Send it?\nOPTIONS: yes | no' > "$work/ollama/reply"
    "$COLONY" -c "agent new lead-miner --every 1s --charter 'Look.'"
    wait_for ReportPosted 1
    for _ in $(seq 50); do [ "$(state)" = "parked" ] && break; sleep 0.1; done
    stop_daemon
    "$COLONY" -c 'on --daemon start' 2>/dev/null
    sleep 2.5
    [ "$(count HeartbeatFired)" -eq 1 ]
    run "$COLONY" -c 'agent list'
    [[ "${lines[1]}" =~ ^lead-miner\ +parked ]]

    printf 'PASS: sent' > "$work/ollama/reply"
    for _ in $(seq 50); do [ -s "$HOME/.colony/events/viewport.url" ] && break; sleep 0.1; done
    url="$(cat "$HOME/.colony/events/viewport.url")"
    run curl -s -o /dev/null -w '%{http_code}' -H "Origin: ${url%/}" -H 'Content-Type: application/json' \
        -d '{"ask":"colony://local/agent/lead-miner/run/1/ask/1","answer":"yes"}' "${url}api/answer"
    [ "$output" = "200" ]
    wait_for RunResumed 1 100
}

@test "secret files in the workdir never enter the container, and the skipped paths are recorded" {
    need_podman
    mkdir -p "$work/project/.ssh" "$work/project/certs"
    printf 'TOKEN=hunter2' > "$work/project/.env"
    printf 'k' > "$work/project/certs/tls.key"
    printf 'k' > "$work/project/.ssh/id_ed25519"
    printf 'ok' > "$work/project/.env.example"
    python3 -I -c 'import json; print(json.dumps([{"tool_calls": [{"name": "shell", "arguments": {"command": "ls -AR /work /base; test -e /work/.env || echo no-env; grep -rq hunter2 /work /base; echo grep=$?"}}]}, "WARN: listed"]))' > "$work/ollama/script"
    "$COLONY" -c "agent new lead-miner --every 2s --image run --charter 'Look.'"
    wait_for HeartbeatAnswered 1
    "$COLONY" -c 'agent pause lead-miner' >/dev/null
    run q "SELECT json_extract(data, '$.content') FROM events WHERE subject = 'colony://local/agent/lead-miner/run/1/output/2'"
    [[ "$output" == *".env.example"* ]]
    [[ "$output" == *"no-env"* ]]
    [[ "$output" == *"grep=1"* ]]
    [[ "$output" != *"id_ed25519"* ]]
    [[ "$output" != *"tls.key"* ]]
    run q "SELECT json_extract(data, '$.skipped') FROM events WHERE kind = 'ContainerStarted'"
    [ "$output" = '[".env",".ssh","certs/tls.key"]' ]
    # The workdir itself is untouched, and the staged copy is gone.
    [ -f "$work/project/.env" ]
    [ ! -e "$HOME/.colony/runs/$(container)" ]
}
