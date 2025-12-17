#!/usr/bin/env bats
# Test Colony shell builtins for POSIX compliance
# Compare behavior against bash

# Path to colony binary (use absolute path set by runner)
# Don't use $(pwd) here - BATS will try to execute it
COLONY="${COLONY_BIN}"

setup() {
    # Create temp directory for test files
    TEST_TEMP="$(mktemp -d)"
    export TEST_TEMP
}

teardown() {
    # Clean up temp directory
    rm -rf "$TEST_TEMP"
}

@test "echo builtin - simple output" {
    run "$COLONY" -c "echo hello"
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "echo builtin - multiple arguments" {
    run "$COLONY" -c "echo hello world"
    [ "$status" -eq 0 ]
    [ "$output" = "hello world" ]
}

@test "pwd builtin - current directory" {
    run "$COLONY" -c "pwd"
    [ "$status" -eq 0 ]
    # Output should be an absolute path
    [[ "$output" == /* ]]
}

@test "export builtin - set environment variable" {
    run "$COLONY" -c "export TEST_VAR=hello; echo \$TEST_VAR"
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "export builtin - multiple variables" {
    run "$COLONY" -c "export VAR1=foo VAR2=bar; echo \$VAR1 \$VAR2"
    [ "$status" -eq 0 ]
    [ "$output" = "foo bar" ]
}

@test "cd builtin - change directory" {
    run "$COLONY" -c "cd /tmp && pwd"
    [ "$status" -eq 0 ]
    [[ "$output" == *"/tmp"* ]]
}

@test "cd builtin - invalid directory" {
    run "$COLONY" -c "cd /nonexistent"
    [ "$status" -ne 0 ]
}

@test "type builtin - identify builtin" {
    run "$COLONY" -c "type cd"
    [ "$status" -eq 0 ]
    [[ "$output" == *"builtin"* ]]
}

@test "type builtin - identify external command" {
    run "$COLONY" -c "type ls"
    [ "$status" -eq 0 ]
    # Should show path or indicate it's a command
}

@test "exit builtin - explicit exit code" {
    run "$COLONY" -c "exit 42"
    [ "$status" -eq 42 ]
}

@test "exit builtin - default exit code 0" {
    run "$COLONY" -c "echo test; exit"
    [ "$status" -eq 0 ]
    [ "$output" = "test" ]
}

# Test against bash for compatibility
@test "compare echo output with bash" {
    local test_string="hello world 123"

    run bash -c "echo $test_string"
    local bash_output="$output"
    local bash_status="$status"

    run "$COLONY" -c "echo $test_string"
    local colony_output="$output"
    local colony_status="$status"

    [ "$colony_status" -eq "$bash_status" ]
    [ "$colony_output" = "$bash_output" ]
}

@test "compare pwd output with bash" {
    cd ~

    run bash -c "pwd"
    local bash_output="$output"

    run "$COLONY" -c "pwd"
    local colony_output="$output"

    [ "$colony_output" = "$bash_output" ]
}

@test "compare variable expansion with bash" {
    export TEST_VAR="test value"

    run bash -c "echo \$TEST_VAR"
    local bash_output="$output"

    run "$COLONY" -c "echo \$TEST_VAR"
    local colony_output="$output"

    [ "$colony_output" = "$bash_output" ]
}

@test "break in for loop" {
    run "$COLONY" -c "for i in a b c; do echo \$i; break; echo bad-\$i; done"
    [ "$status" -eq 0 ]
    [ "$output" = "a" ]
}

@test "continue in for loop" {
    run "$COLONY" -c "for i in a b c; do if [ \$i = b ]; then continue; fi; echo \$i; done"
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "a" ]
    [ "${lines[1]}" = "c" ]
}

@test "nested loops with break" {
    run "$COLONY" -c 'for i in a b; do for j in x y; do echo $i:$j; break; done; done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "a:x" ]
    [ "${lines[1]}" = "b:x" ]
}

@test "command substitution in echo" {
    run "$COLONY" -c "echo hello \$(echo world)"
    [ "$status" -eq 0 ]
    [ "$output" = "hello world" ]
}

@test "pipeline with builtins" {
    run "$COLONY" -c "echo hello | cat"
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "redirection with echo" {
    local testfile="$TEST_TEMP/output.txt"
    run "$COLONY" -c "echo hello > $testfile && cat $testfile"
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}
