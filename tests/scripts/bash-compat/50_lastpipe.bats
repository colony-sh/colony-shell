#!/usr/bin/env bats
# Test lastpipe shopt option
# Based on bash's lastpipe.tests

COLONY="${COLONY_BIN:-../../target/release/colony}"

# =============================================================================
# Basic lastpipe behavior
# =============================================================================

@test "lastpipe: disabled by default" {
    run "$COLONY" -c 'x=initial; echo hello | read x; echo $x'
    [ "$status" -eq 0 ]
    [ "$output" = "initial" ]
}

@test "lastpipe: enabled allows variable to persist" {
    run "$COLONY" -c 'shopt -s lastpipe; x=initial; echo hello | read x; echo $x'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "lastpipe: works with multiple pipeline stages" {
    run "$COLONY" -c 'shopt -s lastpipe; echo "a b c" | cat | read x; echo $x'
    [ "$status" -eq 0 ]
    [ "$output" = "a b c" ]
}

@test "lastpipe: can be disabled" {
    run "$COLONY" -c 'shopt -s lastpipe; shopt -u lastpipe; x=initial; echo hello | read x; echo $x'
    [ "$status" -eq 0 ]
    [ "$output" = "initial" ]
}

# =============================================================================
# Variable modifications in last command
# =============================================================================

@test "lastpipe: array assignment" {
    run "$COLONY" -c 'shopt -s lastpipe; arr=(); echo -e "a\nb\nc" | while read line; do arr+=("$line"); done; echo "${arr[@]}"'
    [ "$status" -eq 0 ]
    [ "$output" = "a b c" ]
}

@test "lastpipe: counter increment" {
    run "$COLONY" -c 'shopt -s lastpipe; count=0; echo -e "1\n2\n3" | while read line; do ((count++)); done; echo $count'
    [ "$status" -eq 0 ]
    [ "$output" = "3" ]
}

@test "lastpipe: mapfile in pipeline" {
    run "$COLONY" -c 'shopt -s lastpipe; echo -e "a\nb\nc" | mapfile -t arr; echo "${arr[1]}"'
    [ "$status" -eq 0 ]
    [ "$output" = "b" ]
}

# =============================================================================
# Exit status
# =============================================================================

@test "lastpipe: exit status from last command" {
    run "$COLONY" -c 'shopt -s lastpipe; echo hello | false; echo $?'
    [ "$status" -eq 0 ]
    [ "$output" = "1" ]
}

@test "lastpipe: exit status success" {
    run "$COLONY" -c 'shopt -s lastpipe; false | true; echo $?'
    [ "$status" -eq 0 ]
    [ "$output" = "0" ]
}

@test "lastpipe: pipefail interaction" {
    run "$COLONY" -c 'shopt -s lastpipe; set -o pipefail; false | true; echo $?'
    [ "$status" -eq 0 ]
    [ "$output" = "1" ]
}

# =============================================================================
# Job control interaction
# =============================================================================

@test "lastpipe: requires job control off in interactive shell" {
    skip "Interactive shell test"
    run "$COLONY" -c 'set +m; shopt -s lastpipe; echo hello | read x; echo $x'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

# =============================================================================
# Complex pipelines
# =============================================================================

@test "lastpipe: three stage pipeline" {
    run "$COLONY" -c 'shopt -s lastpipe; echo "hello world" | tr " " "\n" | read first; echo $first'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "lastpipe: pipeline with grep" {
    run "$COLONY" -c 'shopt -s lastpipe; echo -e "apple\nbanana\ncherry" | grep "an" | read fruit; echo $fruit'
    [ "$status" -eq 0 ]
    [ "$output" = "banana" ]
}

@test "lastpipe: pipeline with awk" {
    run "$COLONY" -c 'shopt -s lastpipe; echo "1 2 3" | awk "{print \$2}" | read second; echo $second'
    [ "$status" -eq 0 ]
    [ "$output" = "2" ]
}

# =============================================================================
# While loop in pipeline
# =============================================================================

@test "lastpipe: while loop reads all lines" {
    run "$COLONY" -c 'shopt -s lastpipe; total=0; echo -e "1\n2\n3" | while read n; do total=$((total + n)); done; echo $total'
    [ "$status" -eq 0 ]
    [ "$output" = "6" ]
}

@test "lastpipe: while loop with break" {
    run "$COLONY" -c 'shopt -s lastpipe; found=no; echo -e "a\nb\nc" | while read line; do if [[ "$line" == "b" ]]; then found=yes; break; fi; done; echo $found'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

# =============================================================================
# Function calls in pipeline
# =============================================================================

@test "lastpipe: function modifies variable" {
    run "$COLONY" -c 'shopt -s lastpipe; func() { read x; }; y=initial; echo hello | func; echo ${x:-unset}'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "lastpipe: function return value" {
    run "$COLONY" -c 'shopt -s lastpipe; func() { read x; return 42; }; echo hello | func; echo $?'
    [ "$status" -eq 0 ]
    [ "$output" = "42" ]
}

# =============================================================================
# Brace group in pipeline
# =============================================================================

@test "lastpipe: brace group" {
    run "$COLONY" -c 'shopt -s lastpipe; echo hello | { read x; echo "got: $x"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "got: hello" ]
}

@test "lastpipe: brace group variable persists" {
    run "$COLONY" -c 'shopt -s lastpipe; x=before; echo hello | { read x; }; echo $x'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

# =============================================================================
# Subshell behavior (not affected)
# =============================================================================

@test "lastpipe: subshell still isolated" {
    run "$COLONY" -c 'shopt -s lastpipe; x=before; echo hello | (read x; echo "in sub: $x"); echo "after: $x"'
    [ "$status" -eq 0 ]
    [[ "${lines[0]}" == "in sub: hello" ]]
    [[ "${lines[1]}" == "after: before" ]]
}

# =============================================================================
# shopt -q query
# =============================================================================

@test "lastpipe: query when off" {
    run "$COLONY" -c 'shopt -q lastpipe; echo $?'
    [ "$status" -eq 0 ]
    [ "$output" = "1" ]
}

@test "lastpipe: query when on" {
    run "$COLONY" -c 'shopt -s lastpipe; shopt -q lastpipe; echo $?'
    [ "$status" -eq 0 ]
    [ "$output" = "0" ]
}

# =============================================================================
# Compare with bash
# =============================================================================

@test "compare lastpipe with bash: basic" {
    colony_out=$("$COLONY" -c 'shopt -s lastpipe; x=init; echo hello | read x; echo $x' 2>&1)
    bash_out=$(bash -c 'shopt -s lastpipe; set +m; x=init; echo hello | read x; echo $x' 2>&1)
    [ "$colony_out" = "$bash_out" ]
}

@test "compare lastpipe with bash: counter" {
    colony_out=$("$COLONY" -c 'shopt -s lastpipe; n=0; echo -e "1\n2\n3" | while read x; do ((n++)); done; echo $n' 2>&1)
    bash_out=$(bash -c 'shopt -s lastpipe; set +m; n=0; echo -e "1\n2\n3" | while read x; do ((n++)); done; echo $n' 2>&1)
    [ "$colony_out" = "$bash_out" ]
}

@test "compare lastpipe with bash: disabled" {
    colony_out=$("$COLONY" -c 'x=init; echo hello | read x; echo $x' 2>&1)
    bash_out=$(bash -c 'x=init; echo hello | read x; echo $x' 2>&1)
    [ "$colony_out" = "$bash_out" ]
}
