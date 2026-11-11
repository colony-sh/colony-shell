#!/usr/bin/env bats
# Heartbeats run in the colony-events daemon, from state in the event log, with no shell open.

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
    export HOME="$work/home"
    export OLLAMA_HOST="127.0.0.1:$(cat "$work/ollama/port")"
    export COLONY_MODEL_PROVIDER=ollama
    export COLONY_MODEL_NAME=fake
    unset ANTHROPIC_API_KEY OPENAI_API_KEY COLONY_API_KEY
    cd "$work/project"
    printf -- '---\ninterval: 1s\ntimeout: 5s\n---\n\n- [ ] Check\n' > HEARTBEAT.md
}

teardown() {
    stop_daemon
    kill "$ollama_pid" 2>/dev/null || true
    rm -rf "$work"
}

pid_file() { echo "$HOME/.colony/events/router.pid"; }

stop_daemon() {
    local pid
    pid="$(cat "$(pid_file)" 2>/dev/null)" || return 0
    kill "$pid" 2>/dev/null || true
    for _ in $(seq 50); do kill -0 "$pid" 2>/dev/null || return 0; sleep 0.1; done
}

runs() { sqlite3 "$HOME/.colony/reflection/observations.db" "SELECT COUNT(*) FROM heartbeat_runs" 2>/dev/null || echo 0; }
events() { sqlite3 "$HOME/.config/colony/tuple-space.db" "SELECT actor || ' ' || kind FROM events ORDER BY seq"; }

wait_for_runs() {
    for _ in $(seq 100); do [ "$(runs)" -ge "$1" ] && return 0; sleep 0.1; done
    return 1
}

@test "heartbeat start hands the checklist to the daemon, which keeps beating after the shell exits" {
    run "$COLONY" -c 'heartbeat start'
    [ "$status" -eq 0 ]
    [[ "$output" == *"runs in the colony-events daemon (interval=1s"* ]]

    # The shell has exited. The daemon fires on its own.
    wait_for_runs 2
    [ "$(events | head -1)" = "$USER HeartbeatSet" ]
    events | grep -q "^daemon HeartbeatFired$"
    events | grep -q "^daemon HeartbeatAnswered$"

    run "$COLONY" -c 'heartbeat status'
    [[ "$output" == *"Daemon: running"* ]]
    [[ "$output" == *"Heartbeat: "*"/project/HEARTBEAT.md (interval=1s, mode=notify)"* ]]
    [[ "$output" == *"Last run: pass"* ]]

    run "$COLONY" -c 'heartbeat start'
    [[ "$output" == *"already has a heartbeat"* ]]
    [ "$(events | grep -c HeartbeatSet)" -eq 1 ]
}

@test "heartbeat stop stops the beats, and the log records it" {
    "$COLONY" -c 'heartbeat start' 2>/dev/null
    wait_for_runs 1

    run "$COLONY" -c 'heartbeat stop'
    [ "$status" -eq 0 ]
    [[ "$output" == *"stopped "*"/project/HEARTBEAT.md"* ]]
    sleep 1.5
    before="$(runs)"
    sleep 2.5
    [ "$(runs)" -eq "$before" ]
    events | grep -q "HeartbeatPaused$"

    run "$COLONY" -c 'heartbeat stop'
    [ "$status" -eq 1 ]
    [[ "$output" == *"has no heartbeat"* ]]
}

@test "a restarted daemon replays the log and resumes the heartbeat" {
    "$COLONY" -c 'heartbeat start' 2>/dev/null
    wait_for_runs 1
    stop_daemon
    before="$(runs)"

    "$COLONY" -c 'on --daemon start' 2>/dev/null
    wait_for_runs $((before + 2))
}

@test "a second daemon exits instead of firing every beat twice" {
    "$COLONY" -c 'on --daemon start' 2>/dev/null
    first="$(cat "$(pid_file)")"
    run timeout 5 "$(dirname "$COLONY")/colony-events" daemon
    [ "$status" -eq 0 ]
    [ "$(cat "$(pid_file)")" = "$first" ]
    kill -0 "$first"
}

@test "on --daemon install --print shows the login item and changes nothing" {
    run "$COLONY" -c 'on --daemon install --print'
    [ "$status" -eq 0 ]
    if [ "$(uname)" = Darwin ]; then
        [ "${lines[0]}" = "# $HOME/Library/LaunchAgents/sh.colony.events.plist" ]
        printf '%s\n' "${lines[@]:1}" > "$work/item.plist"
        plutil -lint "$work/item.plist"
        grep -q "<string>$(dirname "$COLONY")/colony-events</string>" "$work/item.plist"
        grep -q "<key>OLLAMA_HOST</key><string>$OLLAMA_HOST</string>" "$work/item.plist"
    else
        [ "${lines[0]}" = "# $HOME/.config/systemd/user/sh.colony.events.service" ]
        [[ "$output" == *"ExecStart=\"$(dirname "$COLONY")/colony-events\" daemon"* ]]
    fi
    [[ "$output" != *"API_KEY"* ]]
    [ ! -e "$HOME/Library/LaunchAgents" ]
    [ ! -e "$HOME/.config/systemd" ]
}
