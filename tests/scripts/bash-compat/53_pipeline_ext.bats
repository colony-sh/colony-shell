#!/usr/bin/env bats
# Test Colony shell pipeline extensions (F058)
# Colony-specific feature - extended pipeline operators |>, ?>, !>

COLONY="${COLONY_BIN}"

# ============================================================================
# |> (Tap Pipe) - passes data through like | but also copies to stddiag
# ============================================================================

@test "tap pipe: basic passthrough" {
    run $COLONY -c 'echo hello |> cat'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "tap pipe: multi-stage" {
    run $COLONY -c 'echo -e "b\na\nc" |> sort'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "a" ]
    [ "${lines[1]}" = "b" ]
    [ "${lines[2]}" = "c" ]
}

@test "tap pipe: mixed with regular pipe" {
    run $COLONY -c 'echo hello |> cat | tr a-z A-Z'
    [ "$status" -eq 0 ]
    [ "$output" = "HELLO" ]
}

@test "tap pipe: data flows through to next command" {
    run $COLONY -c 'echo test |> cat |> cat'
    [ "$status" -eq 0 ]
    [ "$output" = "test" ]
}

@test "tap pipe: preserves exit status of last command" {
    run $COLONY -c 'echo ok |> true'
    [ "$status" -eq 0 ]
}

# ============================================================================
# ?> (Guard Pipe) - stops pipeline if previous command fails
# ============================================================================

@test "guard pipe: stops on failure" {
    run $COLONY -c 'false ?> echo should-not-run'
    [ "$status" -ne 0 ]
    [ "$output" = "" ]
}

@test "guard pipe: passes through on success" {
    run $COLONY -c '
echo hello ?> cat
'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "guard pipe: chain stops at first failure" {
    run $COLONY -c '
true ?> false ?> echo should-not-run
'
    [ "$status" -ne 0 ]
    [ "$output" = "" ]
}

@test "guard pipe: chain passes all on success" {
    run $COLONY -c '
echo data ?> cat ?> cat
'
    [ "$status" -eq 0 ]
    [ "$output" = "data" ]
}

@test "guard pipe: exit code from failing command preserved" {
    run $COLONY -c 'false ?> echo no'
    [ "$status" -eq 1 ]
}

@test "guard pipe: success followed by guard runs next segment" {
    run $COLONY -c '
echo test ?> echo ok
'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "test" ]
    [ "${lines[1]}" = "ok" ]
}

# ============================================================================
# !> (Error Catch Pipe) - routes to handler on pipeline failure
# ============================================================================

@test "catch pipe: handler runs on failure" {
    run $COLONY -c 'false !> echo caught'
    [ "$status" -eq 0 ]
    [ "$output" = "caught" ]
}

@test "catch pipe: handler does not run on success" {
    run $COLONY -c 'true !> echo caught'
    [ "$status" -eq 0 ]
    [ "$output" = "" ]
}

@test "catch pipe: handler exit code used" {
    run $COLONY -c 'false !> false'
    [ "$status" -eq 1 ]
}

@test "catch pipe: handler with successful command" {
    run $COLONY -c 'false !> echo recovery'
    [ "$status" -eq 0 ]
    [ "$output" = "recovery" ]
}

@test "catch pipe: pipeline failure triggers handler" {
    run $COLONY -c '
echo data | false !> echo caught-pipeline
'
    [ "$status" -eq 0 ]
    [ "$output" = "caught-pipeline" ]
}

@test "catch pipe: successful pipeline skips handler" {
    run $COLONY -c '
echo data | cat !> echo should-not-run
'
    [ "$status" -eq 0 ]
    [ "$output" = "data" ]
}

# ============================================================================
# Combined Operators
# ============================================================================

@test "combined: guard then catch" {
    run $COLONY -c '
false ?> echo no !> echo caught
'
    [ "$status" -eq 0 ]
    [ "$output" = "caught" ]
}

@test "combined: tap with regular pipe" {
    run $COLONY -c 'echo hello | cat |> tr a-z A-Z'
    [ "$status" -eq 0 ]
    [ "$output" = "HELLO" ]
}

@test "combined: guard then regular pipe within segment" {
    run $COLONY -c '
echo hello ?> echo world | tr a-z A-Z
'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "hello" ]
    [ "${lines[1]}" = "WORLD" ]
}

# ============================================================================
# Edge Cases
# ============================================================================

@test "edge: single command with catch on failure" {
    run $COLONY -c 'false !> echo ok'
    [ "$status" -eq 0 ]
    [ "$output" = "ok" ]
}

@test "edge: single command with catch on success" {
    run $COLONY -c 'echo hello !> echo caught'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "edge: guard runs second segment on success" {
    run $COLONY -c '
printf "hello\nworld\n" ?> echo done
'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "hello" ]
    [ "${lines[1]}" = "world" ]
    [ "${lines[2]}" = "done" ]
}

@test "edge: nested pipelines in command list" {
    run $COLONY -c 'echo a |> cat && echo b |> cat'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "a" ]
    [ "${lines[1]}" = "b" ]
}
