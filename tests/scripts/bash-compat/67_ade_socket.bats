#!/usr/bin/env bats
# The ADE socket: the Colony ADE application's channel to this machine's Colony.

COLONY="${COLONY_BIN}"

setup() {
    command -v python3 >/dev/null || skip "python3 not installed"
    command -v sqlite3 >/dev/null || skip "sqlite3 not installed"
    command -v jq >/dev/null || skip "jq not installed"
    [ -x "$(dirname "$COLONY")/colony-events" ] || skip "colony-events not built next to colony"
    # A short path: a Unix socket's path is at most 104 bytes on macOS.
    work="$(mktemp -d)"
    mkdir -p "$work/home" "$work/project" "$work/ollama"
    python3 -I "$BATS_TEST_DIRNAME/fake_ollama.py" "$work/ollama" &
    ollama_pid=$!
    for _ in $(seq 50); do [ -f "$work/ollama/port" ] && break; sleep 0.1; done
    printf 'HEADLINE: Drafted a reply\nASK: Send it?\nOPTIONS: yes | no' > "$work/ollama/reply"
    export HOME="$work/home"
    export OLLAMA_HOST="127.0.0.1:$(cat "$work/ollama/port")"
    export COLONY_MODEL_PROVIDER=ollama
    export COLONY_MODEL_NAME=fake
    export COLONY_VIEW_ADDR=127.0.0.1:0
    unset ANTHROPIC_API_KEY OPENAI_API_KEY COLONY_API_KEY
    SOCK="$HOME/.colony/events/ade.sock"
    cd "$work/project"
}

teardown() {
    local pid
    pid="$(cat "$HOME/.colony/events/router.pid" 2>/dev/null)" && kill "$pid" 2>/dev/null
    for _ in $(seq 50); do [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null || break; sleep 0.1; done
    kill "$ollama_pid" 2>/dev/null || true
    rm -rf "$work"
}

q() { sqlite3 "$HOME/.config/colony/tuple-space.db" "$1"; }

ade() { python3 -I "$BATS_TEST_DIRNAME/ade_client.py" "$SOCK" "$@"; }

start() {
    "$COLONY" -c 'on --daemon start' 2>/dev/null
    for _ in $(seq 50); do [ -S "$SOCK" ] && return 0; sleep 0.1; done
    return 1
}

parked() {
    "$COLONY" -c "agent new lead-miner --every 1s --charter 'Look.'"
    for _ in $(seq 100); do "$COLONY" -c 'agent list' | grep -q parked && break; sleep 0.1; done
    for _ in $(seq 50); do [ -S "$SOCK" ] && return 0; sleep 0.1; done
}

@test "the daemon serves the socket, only to its own user, and on --daemon status names it" {
    start
    [ "$(stat -f %Lp "$SOCK" 2>/dev/null || stat -c %a "$SOCK")" = "600" ]
    [ "$(stat -f %Lp "$HOME/.colony/events" 2>/dev/null || stat -c %a "$HOME/.colony/events")" = "700" ]
    run "$COLONY" -c 'on --daemon status'
    [[ "$output" == *"ADE socket: $SOCK"* ]]
    run ade '{"id": 1, "method": "hello"}'
    [ "$(jq -r '.id' <<<"$output")" = "1" ]
    [ "$(jq -r '.result.protocol' <<<"$output")" = "1" ]
    [ "$(jq -r '.result.pid' <<<"$output")" = "$(cat "$HOME/.colony/events/router.pid")" ]
}

@test "agents, feed and open read what the log says" {
    parked
    run ade '{"id": 1, "method": "agents"}' '{"id": 2, "method": "feed"}' \
        '{"id": 3, "method": "open", "params": {"address": "colony://local/agent/lead-miner/run/1/output/1"}}'
    [ "$(jq -r '.result[0].name + "|" + .result[0].state + "|" + .result[0].open_asks[0]' <<<"${lines[0]}")" = \
        "lead-miner|parked|colony://local/agent/lead-miner/run/1/ask/1" ]
    [ "$(jq -r '.result.needs_you[0].asks[0].question' <<<"${lines[1]}")" = "Send it?" ]
    [ "$(jq -r '.result.text' <<<"${lines[2]}")" = $'HEADLINE: Drafted a reply\nASK: Send it?\nOPTIONS: yes | no' ]
}

@test "an answer over the socket resumes the run, and the log names the ADE as who answered" {
    parked
    printf 'PASS: sent' > "$work/ollama/reply"
    run ade '{"id": 7, "method": "answer", "params": {"ask": "colony://local/agent/lead-miner/run/1/ask/1", "answer": "maybe"}}' \
        '{"id": 8, "method": "answer", "params": {"ask": "colony://local/agent/lead-miner/run/1/ask/1", "answer": "yes"}}'
    [[ "$(jq -r '.error' <<<"${lines[0]}")" == "answer one of: yes, no" ]]
    [ "$(jq -r '.id' <<<"${lines[1]}")" = "8" ]
    seq="$(jq -r '.result.seq' <<<"${lines[1]}")"
    [ "$(q "SELECT actor || '|' || json_extract(data, '$.answer') FROM events WHERE seq = $seq")" = "ade|yes" ]
    for _ in $(seq 100); do [ "$(q "SELECT COUNT(*) FROM events WHERE kind = 'RunResumed'")" -ge 1 ] && break; sleep 0.1; done
    [ "$(q "SELECT COUNT(*) FROM events WHERE kind = 'RunResumed'")" -eq 1 ]
}

@test "subscribe streams events as they are appended, from where the client says" {
    start
    ade --subscribe 3 > "$work/stream" &
    sub=$!
    sleep 0.5
    "$COLONY" -c "agent new lead-miner --every 1h --charter 'Look.'" >/dev/null
    "$COLONY" -c 'agent pause lead-miner' >/dev/null
    "$COLONY" -c 'agent resume lead-miner' >/dev/null
    wait "$sub"
    run jq -r 'if .result then "subscribed" else .event.kind + " " + .event.subject end' "$work/stream"
    [ "${lines[0]}" = "subscribed" ]
    [ "${lines[1]}" = "AgentCreated colony://local/agent/lead-miner" ]
    [ "${lines[2]}" = "AgentStateChanged colony://local/agent/lead-miner" ]
    [ "${lines[3]}" = "AgentStateChanged colony://local/agent/lead-miner" ]

    # From a given point: replays what came after it.
    run ade --subscribe 1 1
    [ "$(jq -r '.event.seq' <<<"${lines[1]}")" = "2" ]
}

@test "bad requests get an error and the connection stays open" {
    start
    run ade 'not json' '{"id": 2, "method": "nope"}' '{"id": 3, "method": "open", "params": {"address": "colony://local/agent/x/run/9/output/1"}}' '{"id": 4, "method": "hello"}'
    [[ "$(jq -r '.error' <<<"${lines[0]}")" == "not JSON:"* ]]
    [ "$(jq -r '.error' <<<"${lines[1]}")" = "unknown method 'nope'" ]
    [ "$(jq -r '.error' <<<"${lines[2]}")" = "nothing at colony://local/agent/x/run/9/output/1" ]
    [ "$(jq -r '.result.protocol' <<<"${lines[3]}")" = "1" ]
}
