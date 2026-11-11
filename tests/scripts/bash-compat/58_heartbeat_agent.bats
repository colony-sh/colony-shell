#!/usr/bin/env bats
# `heartbeat run` reaches colony-agent, here talking to a fake local Ollama.

COLONY="${COLONY_BIN}"

setup() {
    command -v python3 >/dev/null || skip "python3 not installed"
    command -v sqlite3 >/dev/null || skip "sqlite3 not installed"
    work="$(mktemp -d)"
    mkdir -p "$work/home" "$work/project" "$work/ollama"
    python3 -I "$BATS_TEST_DIRNAME/fake_ollama.py" "$work/ollama" &
    ollama_pid=$!
    for _ in $(seq 50); do [ -f "$work/ollama/port" ] && break; sleep 0.1; done
    export HOME="$work/home"
    export OLLAMA_HOST="127.0.0.1:$(cat "$work/ollama/port")"
    export COLONY_MODEL_PROVIDER=ollama
    export COLONY_MODEL_NAME=fake
    unset ANTHROPIC_API_KEY OPENAI_API_KEY COLONY_API_KEY
    cd "$work/project"
    printf -- '---\ninterval: 10m\ntimeout: 5s\n---\n\n- [ ] Check that the build is green\n' > HEARTBEAT.md
}

teardown() {
    kill "$ollama_pid" 2>/dev/null || true
    rm -rf "$work"
}

db() {
    sqlite3 "$HOME/.colony/reflection/observations.db" "$1"
}

@test "heartbeat run completes one run through colony-agent and stores it" {
    printf 'PASS: build is green\nWARN: two TODOs left' > "$work/ollama/reply"

    run "$COLONY" -c 'heartbeat run'
    [ "$status" -eq 0 ]
    [[ "${lines[0]}" == "heartbeat: discovered "*"/project/HEARTBEAT.md" ]]
    [[ "${lines[1]}" == "Run "*": warn ("*"ms)" ]]
    [ "${lines[2]}" = "  PASS: build is green" ]
    [ "${lines[3]}" = "  WARN: two TODOs left" ]

    # The model got the heartbeat system prompt and the checklist body, not the frontmatter.
    request="$(cat "$work/ollama/requests.jsonl")"
    [[ "$request" == *"heartbeat monitor"* ]]
    [[ "$request" == *"Check that the build is green"* ]]
    [[ "$request" != *"interval: 10m"* ]]

    [ "$(db "SELECT status, checklist_path LIKE '%/project/HEARTBEAT.md', interval_secs FROM heartbeat_runs")" = "warn|1|600" ]
}

@test "a run that does not answer within timeout is stored as a timeout" {
    printf -- '---\ntimeout: 1s\n---\n\n- [ ] Check\n' > HEARTBEAT.md
    echo 3 > "$work/ollama/delay"

    run "$COLONY" -c 'heartbeat run'
    [ "$status" -eq 0 ]
    [[ "${lines[1]}" == "Run "*": timeout ("*"ms)" ]]
    [[ "$output" == *"no answer within 1s"* ]]
    [ "$(db 'SELECT status FROM heartbeat_runs')" = "timeout" ]
}

@test "a run whose model cannot be reached is stored as an error, with the reason" {
    kill "$ollama_pid"
    export OLLAMA_HOST="127.0.0.1:1"

    run "$COLONY" -c 'heartbeat run'
    [[ "${lines[1]}" == "Run "*": error ("*"ms)" ]]
    [[ "$output" == *"Ollama request failed"* ]]
    [[ "$output" != *"no agent callback configured"* ]]
    [ "$(db 'SELECT status FROM heartbeat_runs')" = "error" ]
}

@test "the background scheduler runs ticks through colony-agent" {
    printf -- '---\ninterval: 1s\ntimeout: 5s\n---\n\n- [ ] Check\n' > HEARTBEAT.md

    run "$COLONY" -c 'heartbeat start; sleep 3.5; heartbeat status'
    [ "$status" -eq 0 ]
    [[ "$output" == *"Last run: pass"* ]]
    [ "$(db "SELECT COUNT(*) >= 2 FROM heartbeat_runs WHERE status = 'pass'")" = "1" ]
}
