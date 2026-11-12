#!/usr/bin/env bats
# Named agents: created by one command, beaten by the daemon, with their life in the event log.

COLONY="${COLONY_BIN}"

setup() {
    command -v python3 >/dev/null || skip "python3 not installed"
    command -v sqlite3 >/dev/null || skip "sqlite3 not installed"
    [ -x "$(dirname "$COLONY")/colony-events" ] || skip "colony-events not built next to colony"
    work="$(mktemp -d)"
    mkdir -p "$work/home" "$work/project" "$work/ollama"
    python3 -I "$BATS_TEST_DIRNAME/fake_ollama.py" "$work/ollama" &
    ollama_pid=$!
    for _ in $(seq 50); do [ -f "$work/ollama/port" ] && break; sleep 0.1; done
    printf 'PASS: nothing new today' > "$work/ollama/reply"
    export HOME="$work/home"
    export OLLAMA_HOST="127.0.0.1:$(cat "$work/ollama/port")"
    export COLONY_MODEL_PROVIDER=ollama
    export COLONY_MODEL_NAME=fake
    unset ANTHROPIC_API_KEY OPENAI_API_KEY COLONY_API_KEY
    cd "$work/project"
}

teardown() {
    stop_daemon
    kill "$ollama_pid" 2>/dev/null || true
    rm -rf "$work"
}

stop_daemon() {
    local pid
    pid="$(cat "$HOME/.colony/events/router.pid" 2>/dev/null)" || return 0
    kill "$pid" 2>/dev/null || true
    for _ in $(seq 50); do kill -0 "$pid" 2>/dev/null || return 0; sleep 0.1; done
}

q() { sqlite3 "$HOME/.config/colony/tuple-space.db" "$1"; }

answered() {
    q "SELECT COUNT(*) FROM events WHERE kind = 'HeartbeatAnswered' AND subject LIKE 'colony://local/agent/$1/run/%'"
}

wait_for_answers() {
    for _ in $(seq 100); do [ "$(answered "$1")" -ge "$2" ] && return 0; sleep 0.1; done
    return 1
}

@test "agent new leaves the agent sleeping, with its next beat in agent list" {
    run "$COLONY" -c 'agent new lead-miner --every 1h --charter "Find people describing the problem."'
    [ "$status" -eq 0 ]
    [[ "${lines[0]}" =~ ^colony://local/agent/lead-miner:\ sleeping,\ next\ beat\ [0-9]{4}-[0-9]{2}-[0-9]{2}\ [0-9]{2}:[0-9]{2}$ ]]

    run "$COLONY" -c 'agent list'
    [ "$status" -eq 0 ]
    [[ "${lines[0]}" == NAME*STATE*EVERY*"NEXT BEAT"*RUNS ]]
    [[ "${lines[1]}" =~ ^lead-miner\ +sleeping\ +1h\ +[0-9-]{10}\ [0-9:]{5}\ +0$ ]]

    grep -q "Find people describing the problem." "$HOME/.colony/agents/lead-miner/HEARTBEAT.md"
    grep -q "^workdir: .*/project$" "$HOME/.colony/agents/lead-miner/HEARTBEAT.md"
    [ "$(q "SELECT kind FROM events WHERE subject = 'colony://local/agent/lead-miner'")" = "AgentCreated" ]
}

@test "the daemon beats the agent with no shell open, and each run is addressable" {
    "$COLONY" -c 'agent new lead-miner --every 1s --charter "Look for threads."'
    wait_for_answers lead-miner 2

    # One run, in order: running, fired, output, answered, sleeping.
    run q "SELECT kind || ' ' || COALESCE(subject, '') FROM events WHERE seq > 1 ORDER BY seq LIMIT 5"
    [ "${lines[0]}" = "AgentStateChanged colony://local/agent/lead-miner" ]
    [ "${lines[1]}" = "HeartbeatFired colony://local/agent/lead-miner/run/1" ]
    [ "${lines[2]}" = "OutputRecorded colony://local/agent/lead-miner/run/1/output/1" ]
    [ "${lines[3]}" = "HeartbeatAnswered colony://local/agent/lead-miner/run/1" ]
    [ "${lines[4]}" = "AgentStateChanged colony://local/agent/lead-miner" ]
    [ "$(q "SELECT json_extract(data, '$.outcome') FROM events WHERE kind = 'HeartbeatAnswered' LIMIT 1")" = "idle" ]

    run "$COLONY" -c 'colony open colony://local/agent/lead-miner/run/1/output/1 | cat'
    [ "$status" -eq 0 ]
    [[ "$output" == *"PASS: nothing new today"* ]]

    run "$COLONY" -c 'agent list'
    [[ "${lines[1]}" =~ ^lead-miner\ +(sleeping|running)\ +1s\  ]]
}

@test "pause stops the beats, resume starts them, retire ends the agent" {
    "$COLONY" -c 'agent new lead-miner --every 1s --charter "Look."'
    wait_for_answers lead-miner 1

    run "$COLONY" -c 'agent pause lead-miner'
    [ "$status" -eq 0 ]
    [ "$output" = "colony://local/agent/lead-miner: paused" ]
    sleep 2
    before="$(answered lead-miner)"
    sleep 2.5
    [ "$(answered lead-miner)" -eq "$before" ]
    run "$COLONY" -c 'agent list'
    [[ "${lines[1]}" =~ ^lead-miner\ +paused\ +1s\ +-\  ]]

    run "$COLONY" -c 'agent resume lead-miner'
    [ "$status" -eq 0 ]
    wait_for_answers lead-miner $((before + 1))

    run "$COLONY" -c 'agent retire lead-miner'
    [ "$status" -eq 0 ]
    run "$COLONY" -c 'agent resume lead-miner'
    [ "$status" -eq 1 ]
    [[ "$output" == *"lead-miner is retired"* ]]
    run "$COLONY" -c 'agent new lead-miner --every 1s --charter "Again."'
    [ "$status" -eq 1 ]
    [[ "$output" == *"already exists"* ]]
}

@test "a restarted daemon keeps the agent's next beat, from the log" {
    "$COLONY" -c 'agent new lead-miner --every 4s --charter "Look."'
    "$COLONY" -c 'agent list' > "$work/before"
    stop_daemon
    sleep 1
    "$COLONY" -c 'on --daemon start' 2>/dev/null
    "$COLONY" -c 'agent list' > "$work/after"
    cmp "$work/before" "$work/after"
    # Created 4s ago at most; restarted after 1s. The beat comes on time, not 4s after the restart.
    wait_for_answers lead-miner 1
}

@test "a beat past the daily token budget is answered missed" {
    "$COLONY" -c 'agent new lead-miner --every 1s --daily-tokens 1 --charter "Look."'
    wait_for_answers lead-miner 2
    run q "SELECT json_extract(data, '$.outcome') || ' ' || COALESCE(json_extract(data, '$.reason'), '') FROM events WHERE kind = 'HeartbeatAnswered' ORDER BY seq LIMIT 2"
    [ "${lines[0]}" = "idle " ]
    [[ "${lines[1]}" == "missed daily token budget spent ("*" of 1)" ]]
}

@test "agent new says what is wrong" {
    run "$COLONY" -c 'agent new Lead --every 1h --charter x'
    [ "$status" -eq 1 ]
    [[ "$output" == *"bad name 'Lead'"* ]]
    run "$COLONY" -c 'agent new a --charter x'
    [[ "$output" == *"missing --every"* ]]
    run "$COLONY" -c 'agent new a --every 1h'
    [[ "$output" == *"missing --charter"* ]]
    run "$COLONY" -c 'agent new a --every soon --charter x'
    [[ "$output" == *"bad duration 'soon'"* ]]
    run "$COLONY" -c 'agent new a --every 1h --charter x --in nowhere'
    [[ "$output" == *"nowhere"* ]]
    run "$COLONY" -c 'agent list'
    [ "$output" = "No agents." ]
}
