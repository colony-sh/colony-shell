#!/usr/bin/env bats
# `colony log` reads the machine's event log; `colony open` resolves colony:// addresses against it.

COLONY="${COLONY_BIN}"

setup() {
    command -v sqlite3 >/dev/null || skip "sqlite3 not installed"
    export HOME="$(mktemp -d)"
    db="$HOME/.config/colony/tuple-space.db"
    # Creates the empty log.
    run "$COLONY" -c 'colony log'
    [ "$status" -eq 0 ]
    [ -z "$output" ]
    sqlite3 "$db" <<'SQL'
INSERT INTO events (at, actor, kind, subject, data) VALUES
  (1794370000000, 'jesse',  'AgentCreated',   'colony://local/agent/lead-miner',               '{"charter":"find threads"}'),
  (1794370001000, 'daemon', 'RunStarted',     'colony://local/agent/lead-miner/run/1',         '{}'),
  (1794370002000, 'lead-miner', 'OutputRecorded', 'colony://local/agent/lead-miner/run/1/output/1', '{"content":"line one\nline two\n"}'),
  (1794370003000, 'jesse',  'AgentCreated',   'colony://local/agent/lead',                     '{}');
SQL
}

teardown() {
    rm -rf "$HOME"
}

@test "colony log prints events oldest first, and --last and --after narrow it" {
    run "$COLONY" -c 'colony log'
    [ "$status" -eq 0 ]
    [ "${#lines[@]}" -eq 4 ]
    [[ "${lines[0]}" == "#1  "*"  jesse  AgentCreated  colony://local/agent/lead-miner  "*charter* ]]
    [[ "${lines[1]}" == "#2  "*"RunStarted  colony://local/agent/lead-miner/run/1" ]]

    run "$COLONY" -c 'colony log --last 1'
    [ "${#lines[@]}" -eq 1 ]
    [[ "${lines[0]}" == "#4  "* ]]

    run "$COLONY" -c 'colony log --after 2'
    [ "${#lines[@]}" -eq 2 ]
    [[ "${lines[0]}" == "#3  "* ]]
}

@test "colony open shows an output's exact content" {
    run "$COLONY" -c 'colony open colony://local/agent/lead-miner/run/1/output/1 > "$HOME/out"'
    [ "$status" -eq 0 ]
    printf 'line one\nline two\n' > "$HOME/want"
    cmp "$HOME/out" "$HOME/want"
}

@test "colony open on an agent lists its events and its runs', not a sibling's" {
    run "$COLONY" -c 'colony open colony://local/agent/lead-miner'
    [ "$status" -eq 0 ]
    [ "${#lines[@]}" -eq 3 ]
    [[ "$output" != *"agent/lead "* ]]
}

@test "colony open on an event shows it and its data" {
    run "$COLONY" -c 'colony open colony://local/event/1'
    [ "$status" -eq 0 ]
    [[ "${lines[0]}" == "#1  "*AgentCreated* ]]
    [[ "$output" == *'"charter": "find threads"'* ]]
}

@test "colony open says why it cannot resolve" {
    run "$COLONY" -c 'colony open colony://local/agent/nobody'
    [ "$status" -eq 1 ]
    [[ "$output" == *"nothing at colony://local/agent/nobody"* ]]

    run "$COLONY" -c 'colony open colony://buildbox/agent/x'
    [ "$status" -eq 1 ]
    [[ "$output" == *"only local addresses"* ]]

    run "$COLONY" -c 'colony open http://example.com'
    [ "$status" -eq 2 ]
    [[ "$output" == *"not a colony address"* ]]
}

@test "the log refuses to be rewritten" {
    run sqlite3 "$db" "DELETE FROM events"
    [ "$status" -ne 0 ]
    [[ "$output" == *"append-only"* ]]
    run "$COLONY" -c 'colony log'
    [ "${#lines[@]}" -eq 4 ]
}
