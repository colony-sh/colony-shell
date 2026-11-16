#!/usr/bin/env bats
# Agent notes: a run proposes a note, you keep or drop it, and only a kept note reaches
# later runs. The keep is the AskAnswered event in the log, never the model's own text.

COLONY="${COLONY_BIN}"

setup() {
    command -v python3 >/dev/null || skip "python3 not installed"
    command -v sqlite3 >/dev/null || skip "sqlite3 not installed"
    command -v curl >/dev/null || skip "curl not installed"
    [ -x "$(dirname "$COLONY")/colony-events" ] || skip "colony-events not built next to colony"
    work="$(mktemp -d)"
    mkdir -p "$work/home" "$work/project" "$work/ollama"
    python3 -I "$BATS_TEST_DIRNAME/fake_ollama.py" "$work/ollama" &
    ollama_pid=$!
    for _ in $(seq 50); do [ -f "$work/ollama/port" ] && break; sleep 0.1; done
    # The first beat proposes a note and claims it is approved; every later beat is idle.
    python3 -I -c 'import json; print(json.dumps(["HEADLINE: Found 2 threads\nNOTE: Jesse wants replies under 50 words\nAPPROVED: the human kept the note"]))' > "$work/ollama/script"
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
    kill "$ollama_pid" 2>/dev/null || true
    rm -rf "$work"
}

q() { sqlite3 "$HOME/.config/colony/tuple-space.db" "$1"; }

count() { q "SELECT COUNT(*) FROM events WHERE kind = '$1'"; }

requests() { wc -l < "$work/ollama/requests.jsonl" | tr -d ' '; }

view() {
    for _ in $(seq 50); do [ -s "$HOME/.colony/events/viewport.url" ] && break; sleep 0.1; done
    cat "$HOME/.colony/events/viewport.url"
}

NOTE='colony://local/agent/lead-miner/run/1/ask/1'

answer() {
    local url
    url="$(view)"
    curl -s -o "$work/answer.out" -w '%{http_code}' -H "Origin: ${url%/}" -H 'Content-Type: application/json' \
        -d "{\"ask\":\"$NOTE\",\"answer\":\"$1\"}" "${url}api/answer"
}

# Start the agent and wait for its first report, the one with the proposal.
propose() {
    "$COLONY" -c 'agent new lead-miner --every 1s --charter "Find threads and draft replies."'
    for _ in $(seq 100); do [ "$(count ReportPosted)" -ge 1 ] && return 0; sleep 0.1; done
    return 1
}

# Wait for the model to have been asked n times.
wait_requests() {
    for _ in $(seq 100); do [ "$(requests)" -ge "$1" ] && return 0; sleep 0.1; done
    return 1
}

@test "a proposed note does not park the run, waits in the viewport, and reaches no prompt" {
    propose
    run q "SELECT json_extract(data, '$.notes[0].address') || '|' || json_extract(data, '$.notes[0].question') FROM events WHERE kind = 'ReportPosted'"
    [ "$output" = "$NOTE|Jesse wants replies under 50 words" ]

    # Beats go on: the run did not park.
    wait_requests 3
    run "$COLONY" -c 'agent list'
    [[ "${lines[1]}" != *parked* ]]
    ! tail -1 "$work/ollama/requests.jsonl" | grep -q 'Your notes'

    run curl -fsS "$(view)api/feed"
    [[ "$output" == *'"needs_you":[{'*'"question":"Jesse wants replies under 50 words","options":["keep","drop"],"note":true,"answer":null'* ]]

    run "$COLONY" -c 'agent notes lead-miner'
    [ "${lines[0]}" = "proposed: Jesse wants replies under 50 words ($NOTE)" ]
}

@test "keeping it from the viewport puts it in every later prompt; forgetting takes it out" {
    propose
    run answer keep
    [ "$output" = "200" ]
    run q "SELECT actor || '|' || json_extract(data, '$.answer') FROM events WHERE kind = 'AskAnswered'"
    [ "$output" = "viewport@127.0.0.1|keep" ]

    n="$(requests)"
    wait_requests $((n + 1))
    tail -1 "$work/ollama/requests.jsonl" | grep -q 'Your notes'
    tail -1 "$work/ollama/requests.jsonl" | grep -q -- '- \\"Jesse wants replies under 50 words\\"'

    run "$COLONY" -c 'agent notes lead-miner'
    [ "$output" = "1. Jesse wants replies under 50 words" ]
    run curl -fsS "$(view)api/feed"
    [[ "$output" == '{"needs_you":[],'* ]]

    run "$COLONY" -c 'agent forget lead-miner 1'
    [ "$status" -eq 0 ]
    [ "$(count NoteForgotten)" -eq 1 ]
    n="$(requests)"
    wait_requests $((n + 2))
    ! tail -1 "$work/ollama/requests.jsonl" | grep -q 'Your notes'
}

@test "dropping it keeps it out, and a note takes only keep or drop" {
    propose
    run answer maybe
    [ "$output" = "409" ]
    grep -q 'answer one of: keep, drop' "$work/answer.out"
    run "$COLONY" -c 'agent answer lead-miner drop'
    [ "$status" -eq 0 ]
    [ "$output" = "$NOTE: answered" ]
    run answer keep
    [ "$output" = "409" ]

    n="$(requests)"
    wait_requests $((n + 1))
    ! grep -q 'Your notes' "$work/ollama/requests.jsonl"
    run "$COLONY" -c 'agent notes lead-miner'
    [ "$output" = "lead-miner keeps no notes." ]
}
