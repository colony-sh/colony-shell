#!/usr/bin/env bats
# Ask and answer: an ask parks the run, and an answer from the viewport or the shell resumes it.

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
    printf 'HEADLINE: Drafted a reply to thread 4\nASK: Send the reply to thread 4?\nOPTIONS: yes | no' > "$work/ollama/reply"
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
    rm -rf "$work"
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
    for _ in $(seq "${3:-100}"); do [ "$(count "$1")" -ge "$2" ] && return 0; sleep 0.1; done
    return 1
}

view() {
    for _ in $(seq 50); do [ -s "$HOME/.colony/events/viewport.url" ] && break; sleep 0.1; done
    cat "$HOME/.colony/events/viewport.url"
}

ASK='colony://local/agent/lead-miner/run/1/ask/1'

answer() {
    local url
    url="$(view)"
    curl -s -o "$work/answer.out" -w '%{http_code}' -H "Origin: ${url%/}" -H 'Content-Type: application/json' \
        -d "{\"ask\":\"$ASK\",\"answer\":\"$1\"}" "${url}api/answer"
}

park() {
    "$COLONY" -c 'agent new lead-miner --every 1s --charter "Find threads and draft replies."'
    wait_for ReportPosted 1
    for _ in $(seq 50); do "$COLONY" -c 'agent list' | grep -q 'parked' && return 0; sleep 0.1; done
    return 1
}

@test "an ask parks the run: no more beats, and the viewport shows it first" {
    park
    run q "SELECT json_extract(data, '$.outcome') FROM events WHERE kind = 'HeartbeatAnswered'"
    [ "$output" = "needs_you" ]
    run q "SELECT json_extract(data, '$.status') || '|' || json_extract(data, '$.asks[0].address') || '|' || json_extract(data, '$.asks[0].question') FROM events WHERE kind = 'ReportPosted'"
    [ "$output" = "needs_you|$ASK|Send the reply to thread 4?" ]

    sleep 2.5
    [ "$(count HeartbeatFired)" -eq 1 ]
    run "$COLONY" -c 'agent list'
    [[ "${lines[1]}" =~ ^lead-miner\ +parked\ +1s\ +-\  ]]

    run curl -fsS "$(view)api/feed"
    [[ "$output" == *'"needs_you":[{'*'"question":"Send the reply to thread 4?","options":["yes","no"],"answer":null'* ]]
}

@test "answering from the viewport resumes the run within ten seconds, and the log says who and when" {
    park
    printf 'HEADLINE: Sent the reply to thread 4\nWARN: sent one reply' > "$work/ollama/reply"
    run answer yes
    [ "$output" = "200" ]
    wait_for RunResumed 1 100

    run q "SELECT actor || '|' || json_extract(data, '$.answer') FROM events WHERE kind = 'AskAnswered' AND subject = '$ASK'"
    [ "$output" = "viewport@127.0.0.1|yes" ]
    # The resumed turn was told what it asked and what you answered.
    tail -1 "$work/ollama/requests.jsonl" | grep -q 'ANSWER: yes'

    # The same run goes on: its new output is output 2, behind a new report, then it sleeps.
    run q "SELECT kind || ' ' || subject FROM events WHERE seq > (SELECT seq FROM events WHERE kind = 'AskAnswered') AND kind != 'HeartbeatFired' ORDER BY seq LIMIT 5"
    [ "${lines[0]}" = "AgentStateChanged colony://local/agent/lead-miner" ]
    [ "${lines[1]}" = "OutputRecorded colony://local/agent/lead-miner/run/1/output/2" ]
    [ "${lines[2]}" = "RunResumed colony://local/agent/lead-miner/run/1" ]
    [ "${lines[3]}" = "ReportPosted colony://local/agent/lead-miner/run/1" ]
    [ "${lines[4]}" = "AgentStateChanged colony://local/agent/lead-miner" ]
    run q "SELECT json_extract(data, '$.to') FROM events WHERE kind = 'AgentStateChanged' ORDER BY seq DESC LIMIT 1"
    [ "$output" = "sleeping" ]

    run curl -fsS "$(view)api/feed"
    [[ "$output" == '{"needs_you":[],'* ]]
    [[ "$output" == *'"answer":"yes","by":"viewport@127.0.0.1"'* ]]

    # Answered once: a second answer is refused.
    run answer no
    [ "$output" = "409" ]
}

@test "an answer must be one of the ask's options, and another web page cannot answer" {
    park
    run answer maybe
    [ "$output" = "409" ]
    grep -q 'answer one of: yes, no' "$work/answer.out"

    url="$(view)"
    run curl -s -o /dev/null -w '%{http_code}' -H 'Origin: http://evil.example' -H 'Content-Type: application/json' \
        -d "{\"ask\":\"$ASK\",\"answer\":\"yes\"}" "${url}api/answer"
    [ "$output" = "403" ]
    [ "$(count AskAnswered)" -eq 0 ]
}

@test "agent answer answers from the shell" {
    park
    printf 'PASS: done' > "$work/ollama/reply"
    run "$COLONY" -c 'agent answer lead-miner no'
    [ "$status" -eq 0 ]
    [ "$output" = "$ASK: answered" ]
    wait_for RunResumed 1
    run "$COLONY" -c 'agent answer lead-miner yes'
    [ "$status" -eq 1 ]
    [[ "$output" == *"no open ask"* ]]
}

@test "a parked ask survives a daemon restart, and an answer given while it was down resumes the run" {
    park
    stop_daemon
    printf 'PASS: done' > "$work/ollama/reply"
    # Answer with no daemon running: the answer goes in the log, and the daemon starts.
    run "$COLONY" -c 'agent answer lead-miner yes'
    [ "$status" -eq 0 ]
    wait_for RunResumed 1
    [ "$(count HeartbeatFired)" -ge 1 ]
}
