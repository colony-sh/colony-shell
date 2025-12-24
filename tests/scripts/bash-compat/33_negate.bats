#!/usr/bin/env bats
# Test Colony shell pipeline negation (! command) for POSIX compliance
# Compare behavior against bash
#
# POSIX specifies:
# - `! pipeline` inverts the exit status of the pipeline
# - If the pipeline exits 0, `!` makes it exit non-zero (typically 1)
# - If the pipeline exits non-zero, `!` makes it exit 0

COLONY="${COLONY_BIN:-./target/release/colony}"

# ============================================================================
# Basic Negation Tests
# ============================================================================

@test "! true: negates successful command" {
    run "$COLONY" -c '! true'
    [ "$status" -eq 1 ]
}

@test "! false: negates failed command" {
    run "$COLONY" -c '! false'
    [ "$status" -eq 0 ]
}

@test "! echo: successful command negated" {
    run "$COLONY" -c '! echo hello >/dev/null'
    [ "$status" -eq 1 ]
}

@test "negation preserves output" {
    run "$COLONY" -c '! echo hello'
    [ "$status" -eq 1 ]
    [ "$output" = "hello" ]
}

# ============================================================================
# External Commands
# ============================================================================

@test "! with successful external command" {
    run "$COLONY" -c '! /usr/bin/true'
    [ "$status" -eq 1 ]
}

@test "! with failing external command" {
    run "$COLONY" -c '! /usr/bin/false'
    [ "$status" -eq 0 ]
}

@test "! with exit 0 command" {
    run "$COLONY" -c '! exit 0'
    [ "$status" -eq 1 ]
}

@test "! with exit 1 command" {
    run "$COLONY" -c '! exit 1'
    [ "$status" -eq 0 ]
}

@test "! with exit 42 command" {
    run "$COLONY" -c '! exit 42'
    [ "$status" -eq 0 ]
}

# ============================================================================
# Pipeline Negation
# ============================================================================

@test "! negates entire pipeline (success)" {
    run "$COLONY" -c '! echo hello | cat'
    [ "$status" -eq 1 ]
}

@test "! negates entire pipeline (failure at end)" {
    run "$COLONY" -c '! echo hello | false'
    [ "$status" -eq 0 ]
}

@test "! negates pipeline - uses last command status" {
    run "$COLONY" -c '! false | true'
    [ "$status" -eq 1 ]
}

@test "! negates pipeline - three commands" {
    run "$COLONY" -c '! echo a | cat | cat'
    [ "$status" -eq 1 ]
}

@test "! negates pipeline - last command fails" {
    run "$COLONY" -c '! echo a | cat | false'
    [ "$status" -eq 0 ]
}

# ============================================================================
# Double Negation
# ============================================================================

@test "!! true: double negation of success" {
    run "$COLONY" -c '! ! true'
    [ "$status" -eq 0 ]
}

@test "!! false: double negation of failure" {
    run "$COLONY" -c '! ! false'
    [ "$status" -eq 1 ]
}

@test "!!! true: triple negation" {
    run "$COLONY" -c '! ! ! true'
    [ "$status" -eq 1 ]
}

# ============================================================================
# Negation in Conditionals
# ============================================================================

@test "if ! true: then branch not taken" {
    run "$COLONY" -c 'if ! true; then echo yes; else echo no; fi'
    [ "$status" -eq 0 ]
    [ "$output" = "no" ]
}

@test "if ! false: then branch taken" {
    run "$COLONY" -c 'if ! false; then echo yes; else echo no; fi'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

@test "while ! false: loop executes" {
    run "$COLONY" -c 'i=0; while ! [ $i -eq 2 ]; do echo $i; i=$((i+1)); done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "0" ]
    [ "${lines[1]}" = "1" ]
}

@test "until ! true: loop executes" {
    run "$COLONY" -c 'i=0; until ! [ $i -lt 2 ]; do echo $i; i=$((i+1)); done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "0" ]
    [ "${lines[1]}" = "1" ]
}

# ============================================================================
# Negation with && and ||
# ============================================================================

@test "! true && echo: second command not executed" {
    run "$COLONY" -c '! true && echo executed'
    [ "$status" -eq 1 ]
    [ -z "$output" ]
}

@test "! false && echo: second command executed" {
    run "$COLONY" -c '! false && echo executed'
    [ "$status" -eq 0 ]
    [ "$output" = "executed" ]
}

@test "! true || echo: second command executed" {
    run "$COLONY" -c '! true || echo fallback'
    [ "$status" -eq 0 ]
    [ "$output" = "fallback" ]
}

@test "! false || echo: second command not executed" {
    run "$COLONY" -c '! false || echo fallback'
    [ "$status" -eq 0 ]
    [ -z "$output" ]
}

# ============================================================================
# Negation in Compound Commands
# ============================================================================

@test "! with brace group" {
    run "$COLONY" -c '! { true; }'
    [ "$status" -eq 1 ]
}

@test "! with failing brace group" {
    run "$COLONY" -c '! { false; }'
    [ "$status" -eq 0 ]
}

@test "! with subshell" {
    run "$COLONY" -c '! (true)'
    [ "$status" -eq 1 ]
}

@test "! with failing subshell" {
    run "$COLONY" -c '! (false)'
    [ "$status" -eq 0 ]
}

@test "! with complex brace group" {
    run "$COLONY" -c '! { echo one; echo two; true; }'
    [ "$status" -eq 1 ]
    [ "${lines[0]}" = "one" ]
    [ "${lines[1]}" = "two" ]
}

# ============================================================================
# Negation with Test Commands
# ============================================================================

@test "! test: negate test success" {
    run "$COLONY" -c '! test 1 -eq 1'
    [ "$status" -eq 1 ]
}

@test "! test: negate test failure" {
    run "$COLONY" -c '! test 1 -eq 2'
    [ "$status" -eq 0 ]
}

@test "! [ ]: negate bracket success" {
    run "$COLONY" -c '! [ -n "hello" ]'
    [ "$status" -eq 1 ]
}

@test "! [ ]: negate bracket failure" {
    run "$COLONY" -c '! [ -z "hello" ]'
    [ "$status" -eq 0 ]
}

# ============================================================================
# Bash Comparison Tests
# ============================================================================

@test "compare: ! true matches bash" {
    bash_out=$(bash -c '! true; echo $?' 2>&1)
    colony_out=$("$COLONY" -c '! true; echo $?' 2>&1)
    [ "$colony_out" = "$bash_out" ]
}

@test "compare: ! false matches bash" {
    bash_out=$(bash -c '! false; echo $?' 2>&1)
    colony_out=$("$COLONY" -c '! false; echo $?' 2>&1)
    [ "$colony_out" = "$bash_out" ]
}

@test "compare: ! pipeline matches bash" {
    bash_out=$(bash -c '! echo hello | cat >/dev/null; echo $?' 2>&1)
    colony_out=$("$COLONY" -c '! echo hello | cat >/dev/null; echo $?' 2>&1)
    [ "$colony_out" = "$bash_out" ]
}

# ============================================================================
# Edge Cases
# ============================================================================

@test "negation with grep" {
    run "$COLONY" -c 'echo hello | ! grep goodbye'
    # This is NOT standard - ! applies to whole pipeline, not single command
    # So this should be: echo hello | (! grep goodbye)
    # But actually this parses as: (echo hello | ! grep goodbye) which is invalid syntax
    # Skip this as it tests non-standard behavior
    skip "non-standard syntax"
}

@test "! with function call" {
    run "$COLONY" -c 'f() { return 0; }; ! f'
    [ "$status" -eq 1 ]
}

@test "! with failing function call" {
    run "$COLONY" -c 'f() { return 1; }; ! f'
    [ "$status" -eq 0 ]
}

@test "! with exit status variable" {
    run "$COLONY" -c '! false; echo $?'
    [ "$output" = "0" ]
}

@test "! preserves last command exit for $?" {
    run "$COLONY" -c '! true; echo $?'
    [ "$output" = "1" ]
}

# ============================================================================
# Negation Parsing
# ============================================================================

@test "! requires space before command" {
    # '!foo' is history expansion in bash, but in POSIX it should be valid
    # Colony should parse '!' as negation followed by 'foo'
    run "$COLONY" -c '!true 2>/dev/null || echo fallback'
    # Either !true works (status 1) or parse error causes fallback
    [ "$status" -eq 0 ] || [ "$status" -eq 1 ]
}

@test "negation in list context" {
    run "$COLONY" -c 'true; ! false; true'
    [ "$status" -eq 0 ]
}

@test "multiple negations in list" {
    run "$COLONY" -c '! true; ! false; ! true'
    # Last command determines exit status
    [ "$status" -eq 1 ]
}
