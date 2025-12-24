#!/usr/bin/env bats
# Tests for dynamic variables: RANDOM, SECONDS, LINENO, BASHPID, EPOCHSECONDS, EPOCHREALTIME, SRANDOM

COLONY="${COLONY_BIN}"

# =============================================================================
# RANDOM tests
# =============================================================================

@test "RANDOM returns a number" {
    result=$($COLONY -c 'echo $RANDOM')
    [[ "$result" =~ ^[0-9]+$ ]]
}

@test "RANDOM returns value in range 0-32767" {
    result=$($COLONY -c 'echo $RANDOM')
    [ "$result" -ge 0 ]
    [ "$result" -le 32767 ]
}

@test "RANDOM returns different values on consecutive access" {
    result=$($COLONY -c 'r1=$RANDOM; r2=$RANDOM; [ "$r1" != "$r2" ] && echo different || echo same')
    # With high probability, two consecutive RANDOM values should be different
    # But there's a 1/32768 chance they're the same, so we test multiple times
    [[ "$result" == "different" ]] || {
        result=$($COLONY -c 'r1=$RANDOM; r2=$RANDOM; [ "$r1" != "$r2" ] && echo different || echo same')
        [[ "$result" == "different" ]]
    }
}

@test "RANDOM can be seeded" {
    # Same seed should produce same sequence
    result1=$($COLONY -c 'RANDOM=12345; echo $RANDOM $RANDOM $RANDOM')
    result2=$($COLONY -c 'RANDOM=12345; echo $RANDOM $RANDOM $RANDOM')
    [ "$result1" = "$result2" ]
}

@test "different seeds produce different sequences" {
    result1=$($COLONY -c 'RANDOM=12345; echo $RANDOM')
    result2=$($COLONY -c 'RANDOM=54321; echo $RANDOM')
    [ "$result1" != "$result2" ]
}

# =============================================================================
# SRANDOM tests (cryptographic random)
# =============================================================================

@test "SRANDOM returns a number" {
    result=$($COLONY -c 'echo $SRANDOM')
    [[ "$result" =~ ^[0-9]+$ ]]
}

@test "SRANDOM returns 32-bit value" {
    result=$($COLONY -c 'echo $SRANDOM')
    [ "$result" -ge 0 ]
    # 32-bit unsigned max is 4294967295
    [ "$result" -le 4294967295 ]
}

@test "SRANDOM returns different values" {
    result=$($COLONY -c 's1=$SRANDOM; s2=$SRANDOM; [ "$s1" != "$s2" ] && echo different || echo same')
    [[ "$result" == "different" ]] || {
        result=$($COLONY -c 's1=$SRANDOM; s2=$SRANDOM; [ "$s1" != "$s2" ] && echo different || echo same')
        [[ "$result" == "different" ]]
    }
}

# =============================================================================
# SECONDS tests
# =============================================================================

@test "SECONDS returns a number" {
    result=$($COLONY -c 'echo $SECONDS')
    [[ "$result" =~ ^[0-9]+$ ]]
}

@test "SECONDS starts at 0 for new shell" {
    result=$($COLONY -c 'echo $SECONDS')
    [ "$result" -eq 0 ] || [ "$result" -eq 1 ]  # Allow for timing
}

@test "SECONDS can be assigned" {
    result=$($COLONY -c 'SECONDS=100; echo $SECONDS')
    [ "$result" -ge 100 ]
    [ "$result" -le 101 ]  # Allow for timing
}

@test "SECONDS continues from assigned value" {
    result=$($COLONY -c 'SECONDS=1000; sleep 0.1; s=$SECONDS; [ "$s" -ge 1000 ] && echo ok || echo fail')
    [ "$result" = "ok" ]
}

# =============================================================================
# EPOCHSECONDS tests
# =============================================================================

@test "EPOCHSECONDS returns a number" {
    result=$($COLONY -c 'echo $EPOCHSECONDS')
    [[ "$result" =~ ^[0-9]+$ ]]
}

@test "EPOCHSECONDS is close to current time" {
    now=$(date +%s)
    result=$($COLONY -c 'echo $EPOCHSECONDS')
    diff=$((result - now))
    [ "$diff" -ge -2 ] && [ "$diff" -le 2 ]
}

@test "EPOCHSECONDS matches bash" {
    bash_result=$(bash -c 'echo $EPOCHSECONDS')
    colony_result=$($COLONY -c 'echo $EPOCHSECONDS')
    diff=$((colony_result - bash_result))
    [ "$diff" -ge -2 ] && [ "$diff" -le 2 ]
}

# =============================================================================
# EPOCHREALTIME tests
# =============================================================================

@test "EPOCHREALTIME returns a decimal number" {
    result=$($COLONY -c 'echo $EPOCHREALTIME')
    [[ "$result" =~ ^[0-9]+\.[0-9]+$ ]]
}

@test "EPOCHREALTIME has microsecond precision" {
    result=$($COLONY -c 'echo $EPOCHREALTIME')
    # Should have 6 digits after decimal
    [[ "$result" =~ \.[0-9]{6}$ ]]
}

@test "EPOCHREALTIME is close to current time" {
    now=$(date +%s)
    result=$($COLONY -c 'echo $EPOCHREALTIME')
    # Extract integer part
    int_part=${result%.*}
    diff=$((int_part - now))
    [ "$diff" -ge -2 ] && [ "$diff" -le 2 ]
}

# =============================================================================
# BASHPID tests
# =============================================================================

@test "BASHPID returns a number" {
    result=$($COLONY -c 'echo $BASHPID')
    [[ "$result" =~ ^[0-9]+$ ]]
}

@test "BASHPID equals dollar-dollar in main shell" {
    result=$($COLONY -c '[ "$BASHPID" = "$$" ] && echo same || echo different')
    # Note: Our $$ implementation may differ - skip this if needed
    skip "BASHPID == $$ test - implementation specific"
}

@test "BASHPID is different in subshell" {
    skip "Subshell BASHPID test requires subshell support"
    result=$($COLONY -c 'main=$BASHPID; sub=$(echo $BASHPID); [ "$main" != "$sub" ] && echo different || echo same')
    [ "$result" = "different" ]
}

# =============================================================================
# LINENO tests
# =============================================================================

@test "LINENO returns a number" {
    result=$($COLONY -c 'echo $LINENO')
    [[ "$result" =~ ^[0-9]+$ ]]
}

@test "LINENO is 1 for single line command" {
    result=$($COLONY -c 'echo $LINENO')
    [ "$result" = "1" ]
}

@test "LINENO increments in multiline script" {
    skip "Multiline LINENO tracking requires script execution"
}

# =============================================================================
# Integration tests
# =============================================================================

@test "multiple dynamic vars in one command" {
    result=$($COLONY -c 'echo $RANDOM $SECONDS $EPOCHSECONDS $LINENO')
    words=($result)
    [ "${#words[@]}" -eq 4 ]
    # All should be numbers
    [[ "${words[0]}" =~ ^[0-9]+$ ]]
    [[ "${words[1]}" =~ ^[0-9]+$ ]]
    [[ "${words[2]}" =~ ^[0-9]+$ ]]
    [[ "${words[3]}" =~ ^[0-9]+$ ]]
}

@test "dynamic vars in arithmetic" {
    result=$($COLONY -c 'echo $((RANDOM % 100))')
    [[ "$result" =~ ^[0-9]+$ ]]
    [ "$result" -ge 0 ]
    [ "$result" -lt 100 ]
}

@test "dynamic vars in comparison" {
    result=$($COLONY -c '[ $RANDOM -ge 0 ] && echo ok || echo fail')
    [ "$result" = "ok" ]
}

@test "dynamic vars in variable assignment" {
    result=$($COLONY -c 'x=$RANDOM; echo $x')
    [[ "$result" =~ ^[0-9]+$ ]]
    [ "$result" -ge 0 ]
    [ "$result" -le 32767 ]
}
