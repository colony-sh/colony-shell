#!/usr/bin/env bats
# Test Colony shell arithmetic expansion for POSIX compliance
# Compare behavior against bash

COLONY="${COLONY_BIN}"

@test "arithmetic: simple addition" {
    run "$COLONY" -c 'echo $((3 + 5))'
    [ "$status" -eq 0 ]
    [ "$output" = "8" ]
}

@test "arithmetic: simple subtraction" {
    run "$COLONY" -c 'echo $((10 - 3))'
    [ "$status" -eq 0 ]
    [ "$output" = "7" ]
}

@test "arithmetic: simple multiplication" {
    run "$COLONY" -c 'echo $((4 * 5))'
    [ "$status" -eq 0 ]
    [ "$output" = "20" ]
}

@test "arithmetic: simple division" {
    run "$COLONY" -c 'echo $((20 / 4))'
    [ "$status" -eq 0 ]
    [ "$output" = "5" ]
}

@test "arithmetic: modulo" {
    run "$COLONY" -c 'echo $((17 % 5))'
    [ "$status" -eq 0 ]
    [ "$output" = "2" ]
}

@test "arithmetic: precedence - multiplication before addition" {
    run "$COLONY" -c 'echo $((2 + 3 * 4))'
    [ "$status" -eq 0 ]
    [ "$output" = "14" ]
}

@test "arithmetic: precedence - parentheses override" {
    run "$COLONY" -c 'echo $(((2 + 3) * 4))'
    [ "$status" -eq 0 ]
    [ "$output" = "20" ]
}

@test "arithmetic: precedence - division before subtraction" {
    run "$COLONY" -c 'echo $((20 - 12 / 3))'
    [ "$status" -eq 0 ]
    [ "$output" = "16" ]
}

@test "arithmetic: unary minus" {
    run "$COLONY" -c 'echo $((-5))'
    [ "$status" -eq 0 ]
    [ "$output" = "-5" ]
}

@test "arithmetic: unary minus on expression" {
    run "$COLONY" -c 'echo $((-(2 + 3)))'
    [ "$status" -eq 0 ]
    [ "$output" = "-5" ]
}

@test "arithmetic: variable expansion" {
    run "$COLONY" -c 'export x=10 && echo $((x + 5))'
    [ "$status" -eq 0 ]
    [ "$output" = "15" ]
}

@test "arithmetic: multiple variables" {
    run "$COLONY" -c 'export x=10 && export y=5 && echo $((x * y))'
    [ "$status" -eq 0 ]
    [ "$output" = "50" ]
}

@test "arithmetic: undefined variable is zero" {
    run "$COLONY" -c 'echo $((undefined_var + 5))'
    [ "$status" -eq 0 ]
    [ "$output" = "5" ]
}

@test "arithmetic: complex expression" {
    run "$COLONY" -c 'echo $((10 + 5 * 2 - 8 / 4))'
    [ "$status" -eq 0 ]
    [ "$output" = "18" ]
}

@test "arithmetic: nested parentheses" {
    run "$COLONY" -c 'echo $(((10 + (5 * 2)) - (8 / 4)))'
    [ "$status" -eq 0 ]
    [ "$output" = "18" ]
}

@test "arithmetic: spaces don't matter" {
    run "$COLONY" -c 'echo $((3+5))'
    [ "$status" -eq 0 ]
    [ "$output" = "8" ]
}

@test "arithmetic: multiple in one line" {
    run "$COLONY" -c 'echo $((1+1)) $((2*2)) $((3+3))'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "2 4 6" ]
}

@test "arithmetic: in command substitution" {
    run "$COLONY" -c 'export result=$((5 * 5)) && echo $result'
    [ "$status" -eq 0 ]
    [ "$output" = "25" ]
}

# Compare with bash
@test "compare arithmetic with bash" {
    local expr="3 + 5 * 32"

    run bash -c "echo \$((${expr}))"
    local bash_output="$output"

    run "$COLONY" -c "echo \$((${expr}))"
    local colony_output="$output"

    [ "$colony_output" = "$bash_output" ]
}

@test "compare complex arithmetic with bash" {
    local expr="(10 + 20) * 3 - 15 / 5"

    run bash -c "echo \$((${expr}))"
    local bash_output="$output"

    run "$COLONY" -c "echo \$((${expr}))"
    local colony_output="$output"

    [ "$colony_output" = "$bash_output" ]
}
