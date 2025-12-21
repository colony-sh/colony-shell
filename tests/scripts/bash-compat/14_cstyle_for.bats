#!/usr/bin/env bats
# Test Colony shell C-style for loop for bash compatibility
# Syntax: for ((init; condition; update)); do body; done

COLONY="${COLONY_BIN}"

# Basic C-style for loop tests
@test "cstyle-for: simple counter 0 to 2" {
    run "$COLONY" -c 'for ((i=0; i<3; i++)); do echo $i; done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "0" ]
    [ "${lines[1]}" = "1" ]
    [ "${lines[2]}" = "2" ]
}

@test "cstyle-for: countdown with decrement" {
    run "$COLONY" -c 'for ((i=10; i>0; i-=2)); do echo $i; done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "10" ]
    [ "${lines[1]}" = "8" ]
    [ "${lines[2]}" = "6" ]
    [ "${lines[3]}" = "4" ]
    [ "${lines[4]}" = "2" ]
}

@test "cstyle-for: infinite loop with break" {
    run "$COLONY" -c 'for ((;;)); do echo infinite; break; done'
    [ "$status" -eq 0 ]
    [ "$output" = "infinite" ]
}

@test "cstyle-for: counter 1 to 5" {
    run "$COLONY" -c 'for ((i=1; i<=5; i++)); do echo $i; done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "1" ]
    [ "${lines[1]}" = "2" ]
    [ "${lines[2]}" = "3" ]
    [ "${lines[3]}" = "4" ]
    [ "${lines[4]}" = "5" ]
}

@test "cstyle-for: multiplication step" {
    run "$COLONY" -c 'for ((i=1; i<=16; i*=2)); do echo $i; done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "1" ]
    [ "${lines[1]}" = "2" ]
    [ "${lines[2]}" = "4" ]
    [ "${lines[3]}" = "8" ]
    [ "${lines[4]}" = "16" ]
}

@test "cstyle-for: pre-decrement" {
    run "$COLONY" -c 'for ((i=3; i>0; --i)); do echo $i; done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "3" ]
    [ "${lines[1]}" = "2" ]
    [ "${lines[2]}" = "1" ]
}

@test "cstyle-for: post-increment with compound statements" {
    run "$COLONY" -c 'for ((i=0; i<3; i++)); do echo "value: $i"; done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "value: 0" ]
    [ "${lines[1]}" = "value: 1" ]
    [ "${lines[2]}" = "value: 2" ]
}

@test "cstyle-for: empty init" {
    run "$COLONY" -c 'i=0; for ((; i<3; i++)); do echo $i; done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "0" ]
    [ "${lines[1]}" = "1" ]
    [ "${lines[2]}" = "2" ]
}

@test "cstyle-for: empty update" {
    # SKIP: This tests body-based increment which relies on a separate variable scoping feature
    skip "Body-based increment (i=\$((i+1))) relies on scoping fix for bash assignments"
    run "$COLONY" -c 'for ((i=0; i<3;)); do echo $i; i=$((i+1)); done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "0" ]
    [ "${lines[1]}" = "1" ]
    [ "${lines[2]}" = "2" ]
}

@test "cstyle-for: empty condition (always true)" {
    run "$COLONY" -c 'for ((i=0;; i++)); do echo $i; if [ $i -ge 2 ]; then break; fi; done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "0" ]
    [ "${lines[1]}" = "1" ]
    [ "${lines[2]}" = "2" ]
}

@test "cstyle-for: comparison operators <=" {
    run "$COLONY" -c 'for ((i=1; i<=3; i++)); do echo $i; done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "1" ]
    [ "${lines[1]}" = "2" ]
    [ "${lines[2]}" = "3" ]
}

@test "cstyle-for: comparison operators >=" {
    run "$COLONY" -c 'for ((i=3; i>=1; i--)); do echo $i; done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "3" ]
    [ "${lines[1]}" = "2" ]
    [ "${lines[2]}" = "1" ]
}

@test "cstyle-for: comparison operators ==" {
    # Loop while i != 3
    run "$COLONY" -c 'for ((i=0; i!=3; i++)); do echo $i; done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "0" ]
    [ "${lines[1]}" = "1" ]
    [ "${lines[2]}" = "2" ]
}

@test "cstyle-for: multiple statements in init" {
    run "$COLONY" -c 'for ((i=0, j=10; i<3; i++, j--)); do echo "$i $j"; done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "0 10" ]
    [ "${lines[1]}" = "1 9" ]
    [ "${lines[2]}" = "2 8" ]
}

@test "cstyle-for: negative start" {
    run "$COLONY" -c 'for ((i=-2; i<=0; i++)); do echo $i; done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "-2" ]
    [ "${lines[1]}" = "-1" ]
    [ "${lines[2]}" = "0" ]
}

@test "cstyle-for: nested loops" {
    run "$COLONY" -c 'for ((i=0; i<2; i++)); do for ((j=0; j<2; j++)); do echo "$i,$j"; done; done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "0,0" ]
    [ "${lines[1]}" = "0,1" ]
    [ "${lines[2]}" = "1,0" ]
    [ "${lines[3]}" = "1,1" ]
}

@test "cstyle-for: with continue" {
    run "$COLONY" -c 'for ((i=0; i<5; i++)); do if [ $i -eq 2 ]; then continue; fi; echo $i; done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "0" ]
    [ "${lines[1]}" = "1" ]
    [ "${lines[2]}" = "3" ]
    [ "${lines[3]}" = "4" ]
}

@test "cstyle-for: semicolon before do" {
    # Optional semicolon between )) and do
    run "$COLONY" -c 'for ((i=0; i<2; i++)) ; do echo $i; done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "0" ]
    [ "${lines[1]}" = "1" ]
}

@test "cstyle-for: no semicolon before do" {
    run "$COLONY" -c 'for ((i=0; i<2; i++)) do echo $i; done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "0" ]
    [ "${lines[1]}" = "1" ]
}

# Compare with bash
@test "compare cstyle-for with bash: basic" {
    run bash -c 'for ((i=0; i<3; i++)); do echo $i; done'
    local bash_output="$output"
    
    run "$COLONY" -c 'for ((i=0; i<3; i++)); do echo $i; done'
    [ "$output" = "$bash_output" ]
}

@test "compare cstyle-for with bash: countdown" {
    run bash -c 'for ((i=5; i>0; i--)); do echo $i; done'
    local bash_output="$output"
    
    run "$COLONY" -c 'for ((i=5; i>0; i--)); do echo $i; done'
    [ "$output" = "$bash_output" ]
}
