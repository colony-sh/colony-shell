#!/usr/bin/env bats
# Test [[ conditional expression ]]
# Based on bash's cond.tests

COLONY="${COLONY_BIN:-../../target/release/colony}"

# =============================================================================
# Basic string tests
# =============================================================================

@test "cond: string equality ==" {
    run "$COLONY" -c '[[ "hello" == "hello" ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

@test "cond: string inequality !=" {
    run "$COLONY" -c '[[ "hello" != "world" ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

@test "cond: string equality = (single)" {
    run "$COLONY" -c '[[ "hello" = "hello" ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

@test "cond: string less than <" {
    run "$COLONY" -c '[[ "apple" < "banana" ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

@test "cond: string greater than >" {
    run "$COLONY" -c '[[ "zebra" > "apple" ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

# =============================================================================
# String length tests
# =============================================================================

@test "cond: -z empty string" {
    run "$COLONY" -c '[[ -z "" ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

@test "cond: -z non-empty string" {
    run "$COLONY" -c '[[ -z "hello" ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "no" ]
}

@test "cond: -n non-empty string" {
    run "$COLONY" -c '[[ -n "hello" ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

@test "cond: -n empty string" {
    run "$COLONY" -c '[[ -n "" ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "no" ]
}

# =============================================================================
# Pattern matching
# =============================================================================

@test "cond: pattern matching == with *" {
    run "$COLONY" -c '[[ "hello" == h* ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

@test "cond: pattern matching == with ?" {
    run "$COLONY" -c '[[ "hello" == hell? ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

@test "cond: pattern matching == with [...]" {
    run "$COLONY" -c '[[ "hello" == [gh]ello ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

@test "cond: pattern matching fails" {
    run "$COLONY" -c '[[ "hello" == world* ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "no" ]
}

@test "cond: quoted pattern is literal" {
    run "$COLONY" -c '[[ "h*" == "h*" ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

@test "cond: quoted pattern does not glob" {
    run "$COLONY" -c '[[ "hello" == "h*" ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "no" ]
}

# =============================================================================
# Regex matching =~
# =============================================================================

@test "cond: regex match =~" {
    run "$COLONY" -c '[[ "hello123" =~ [0-9]+ ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

@test "cond: regex match full" {
    run "$COLONY" -c '[[ "hello" =~ ^h.*o$ ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

@test "cond: regex no match" {
    run "$COLONY" -c '[[ "hello" =~ ^[0-9]+$ ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "no" ]
}

@test "cond: regex with BASH_REMATCH" {
    run "$COLONY" -c '[[ "hello123world" =~ ([0-9]+) ]] && echo "${BASH_REMATCH[1]}"'
    [ "$status" -eq 0 ]
    [ "$output" = "123" ]
}

@test "cond: regex BASH_REMATCH full match" {
    run "$COLONY" -c '[[ "abc123def" =~ ([a-z]+)([0-9]+)([a-z]+) ]] && echo "${BASH_REMATCH[0]}"'
    [ "$status" -eq 0 ]
    [ "$output" = "abc123def" ]
}

# =============================================================================
# Logical operators
# =============================================================================

@test "cond: && both true" {
    run "$COLONY" -c '[[ "a" == "a" && "b" == "b" ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

@test "cond: && one false" {
    run "$COLONY" -c '[[ "a" == "a" && "b" == "c" ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "no" ]
}

@test "cond: || first true" {
    run "$COLONY" -c '[[ "a" == "a" || "b" == "c" ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

@test "cond: || both false" {
    run "$COLONY" -c '[[ "a" == "b" || "c" == "d" ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "no" ]
}

@test "cond: ! negation true becomes false" {
    run "$COLONY" -c '[[ ! "a" == "a" ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "no" ]
}

@test "cond: ! negation false becomes true" {
    run "$COLONY" -c '[[ ! "a" == "b" ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

# =============================================================================
# Grouping with parentheses
# =============================================================================

@test "cond: parentheses grouping" {
    run "$COLONY" -c '[[ ( "a" == "a" ) ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

@test "cond: complex grouping" {
    run "$COLONY" -c '[[ ( "a" == "a" || "b" == "c" ) && "d" == "d" ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

@test "cond: grouping changes precedence" {
    run "$COLONY" -c '[[ "a" == "b" || ( "c" == "c" && "d" == "d" ) ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

# =============================================================================
# File tests
# =============================================================================

@test "cond: -e file exists" {
    run "$COLONY" -c '[[ -e /etc/passwd ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

@test "cond: -e file not exists" {
    run "$COLONY" -c '[[ -e /nonexistent_file_12345 ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "no" ]
}

@test "cond: -f regular file" {
    run "$COLONY" -c '[[ -f /etc/passwd ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

@test "cond: -d directory" {
    run "$COLONY" -c '[[ -d /tmp ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

@test "cond: -r readable" {
    run "$COLONY" -c '[[ -r /etc/passwd ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

@test "cond: -w writable" {
    run "$COLONY" -c '[[ -w /tmp ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

@test "cond: -x executable" {
    run "$COLONY" -c '[[ -x /bin/sh ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

@test "cond: -s non-empty file" {
    run "$COLONY" -c '[[ -s /etc/passwd ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

@test "cond: -L symlink" {
    run "$COLONY" -c 'ln -sf /etc/passwd /tmp/test_symlink; [[ -L /tmp/test_symlink ]] && echo yes || echo no; rm -f /tmp/test_symlink'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

# =============================================================================
# Integer comparisons
# =============================================================================

@test "cond: -eq equal" {
    run "$COLONY" -c '[[ 5 -eq 5 ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

@test "cond: -ne not equal" {
    run "$COLONY" -c '[[ 5 -ne 3 ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

@test "cond: -lt less than" {
    run "$COLONY" -c '[[ 3 -lt 5 ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

@test "cond: -le less or equal" {
    run "$COLONY" -c '[[ 5 -le 5 ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

@test "cond: -gt greater than" {
    run "$COLONY" -c '[[ 5 -gt 3 ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

@test "cond: -ge greater or equal" {
    run "$COLONY" -c '[[ 5 -ge 5 ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

# =============================================================================
# Variable in conditions
# =============================================================================

@test "cond: variable comparison" {
    run "$COLONY" -c 'x=hello; [[ "$x" == "hello" ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

@test "cond: unset variable is empty" {
    run "$COLONY" -c 'unset x; [[ -z "$x" ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

@test "cond: variable with spaces" {
    run "$COLONY" -c 'x="hello world"; [[ "$x" == "hello world" ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

@test "cond: no word splitting in [[" {
    run "$COLONY" -c 'x="hello world"; [[ $x == "hello world" ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

# =============================================================================
# Comparison with bash
# =============================================================================

@test "compare cond with bash: string equality" {
    colony_out=$("$COLONY" -c '[[ "test" == "test" ]] && echo yes' 2>&1)
    bash_out=$(bash -c '[[ "test" == "test" ]] && echo yes' 2>&1)
    [ "$colony_out" = "$bash_out" ]
}

@test "compare cond with bash: pattern match" {
    colony_out=$("$COLONY" -c '[[ "hello" == h* ]] && echo yes' 2>&1)
    bash_out=$(bash -c '[[ "hello" == h* ]] && echo yes' 2>&1)
    [ "$colony_out" = "$bash_out" ]
}

@test "compare cond with bash: -z empty" {
    colony_out=$("$COLONY" -c '[[ -z "" ]] && echo yes' 2>&1)
    bash_out=$(bash -c '[[ -z "" ]] && echo yes' 2>&1)
    [ "$colony_out" = "$bash_out" ]
}

@test "compare cond with bash: file test" {
    colony_out=$("$COLONY" -c '[[ -f /etc/passwd ]] && echo yes' 2>&1)
    bash_out=$(bash -c '[[ -f /etc/passwd ]] && echo yes' 2>&1)
    [ "$colony_out" = "$bash_out" ]
}
