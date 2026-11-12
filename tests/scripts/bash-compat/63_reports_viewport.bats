#!/usr/bin/env bats
# Reports with evidence, and the viewport the daemon serves.

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
    rm -rf "$work"
}

q() { sqlite3 "$HOME/.config/colony/tuple-space.db" "$1"; }

view() {
    for _ in $(seq 50); do [ -s "$HOME/.colony/events/viewport.url" ] && break; sleep 0.1; done
    cat "$HOME/.colony/events/viewport.url"
}

wait_for() {
    for _ in $(seq 100); do [ "$(q "SELECT COUNT(*) FROM events WHERE kind = '$1'")" -ge "$2" ] && return 0; sleep 0.1; done
    return 1
}

@test "a run that acted posts a report with a headline, a status and evidence; the viewport shows it" {
    printf 'HEADLINE: Found 2 threads about agent handoffs\nWARN: two threads need replies' > "$work/ollama/reply"
    "$COLONY" -c 'agent new lead-miner --every 1s --charter "Look for threads."'
    wait_for ReportPosted 1

    run q "SELECT json_extract(data, '$.headline') || '|' || json_extract(data, '$.status') || '|' || json_extract(data, '$.evidence[0]') FROM events WHERE kind = 'ReportPosted' LIMIT 1"
    [ "$output" = "Found 2 threads about agent handoffs|attention|colony://local/agent/lead-miner/run/1/output/1" ]

    url="$(view)"
    [[ "$url" == http://127.0.0.1:*/ ]]
    run curl -fsS "${url}api/feed"
    [ "$status" -eq 0 ]
    [[ "$output" == *'"headline":"Found 2 threads about agent handoffs"'* ]]
    [[ "$output" == *'"needs_you":[]'* ]]

    # Opening the evidence shows the exact output it names.
    curl -fsS "${url}open?a=colony%3A%2F%2Flocal%2Fagent%2Flead-miner%2Frun%2F1%2Foutput%2F1" > "$work/got"
    printf 'HEADLINE: Found 2 threads about agent handoffs\nWARN: two threads need replies' > "$work/want"
    cmp "$work/got" "$work/want"

    run "$COLONY" -c 'colony open colony://local/view/reports'
    [ "$status" -eq 0 ]
    [[ "${lines[0]}" == *"ReportPosted  colony://local/agent/lead-miner/run/1"* ]]
}

@test "a run that finds nothing posts no report" {
    printf 'PASS: nothing new' > "$work/ollama/reply"
    "$COLONY" -c 'agent new lead-miner --every 1s --charter "Look."'
    wait_for HeartbeatAnswered 2
    [ "$(q "SELECT COUNT(*) FROM events WHERE kind = 'ReportPosted'")" -eq 0 ]
    run curl -fsS "$(view)api/feed"
    [[ "$output" == '{"needs_you":[],"feed":[]}' ]]
}

@test "a blocked run needs you, with the answer event as its evidence" {
    "$COLONY" -c 'agent new lead-miner --every 1s --daily-tokens 1 --charter "Look."'
    printf 'WARN: something' > "$work/ollama/reply"
    wait_for ReportPosted 2
    run curl -fsS "$(view)api/feed"
    [[ "$output" == *'"needs_you":[{'*'"status":"blocked"'*'daily token budget spent'*'colony://local/event/'* ]]
}

@test "the page is served with a CSP that forbids inline script, and output is never rendered as HTML" {
    printf 'HEADLINE: <script>alert(1)</script>\nWARN: x' > "$work/ollama/reply"
    "$COLONY" -c 'agent new lead-miner --every 1s --charter "Look."'
    wait_for ReportPosted 1
    url="$(view)"

    run curl -fsSi "$url"
    [[ "$output" == *"content-security-policy: default-src 'none'; script-src 'self';"* ]]
    [[ "$output" == *'<script src="/app.js"></script>'* ]]
    run curl -fsSi "${url}open?a=colony%3A%2F%2Flocal%2Fagent%2Flead-miner%2Frun%2F1%2Foutput%2F1"
    [[ "$output" == *"content-type: text/plain; charset=utf-8"* ]]
    [[ "$output" == *"x-content-type-options: nosniff"* ]]

    run curl -s -o /dev/null -w '%{http_code}' "${url}open?a=http://example.com"
    [ "$output" = "400" ]
    run curl -s -o /dev/null -w '%{http_code}' "${url}open?a=colony%3A%2F%2Flocal%2Fagent%2Fnobody%2Frun%2F9%2Foutput%2F1"
    [ "$output" = "404" ]
}

@test "on --daemon status shows the viewport's address" {
    "$COLONY" -c 'on --daemon start' 2>/dev/null
    url="$(view)"
    run "$COLONY" -c 'on --daemon status'
    [[ "$output" == *"Viewport: $url"* ]]
}

@test "a request naming another host is refused (DNS rebinding)" {
    "$COLONY" -c 'on --daemon start' 2>/dev/null
    url="$(view)"
    run curl -s -o /dev/null -w '%{http_code}' -H 'Host: evil.example' "${url}api/feed"
    [ "$output" = "421" ]
    run curl -s -o /dev/null -w '%{http_code}' "${url}api/feed"
    [ "$output" = "200" ]
}
