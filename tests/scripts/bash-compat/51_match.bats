#!/usr/bin/env bats
# Test Colony shell match statement (F056)
# Colony-specific feature - structured pattern matching

COLONY="${COLONY_BIN}"

# ============================================================================
# Basic Match Statements
# ============================================================================

@test "match: integer literal match" {
    run $COLONY -c 'match 42 { 42 => { echo matched; } _ => { echo default; } }'
    [ "$status" -eq 0 ]
    [ "$output" = "matched" ]
}

@test "match: string literal match" {
    run $COLONY -c 'match "hello" { "hello" => { echo matched; } _ => { echo default; } }'
    [ "$status" -eq 0 ]
    [ "$output" = "matched" ]
}

@test "match: no match falls through to wildcard" {
    run $COLONY -c 'match 99 { 42 => { echo wrong; } _ => { echo default; } }'
    [ "$status" -eq 0 ]
    [ "$output" = "default" ]
}

@test "match: first matching arm wins" {
    run $COLONY -c 'match 1 { 1 => { echo first; } 1 => { echo second; } _ => { echo default; } }'
    [ "$status" -eq 0 ]
    [ "$output" = "first" ]
}

@test "match: no matching arm gives exit 0" {
    run $COLONY -c 'match 99 { 42 => { echo wrong; } }'
    [ "$status" -eq 0 ]
    [ "$output" = "" ]
}

@test "match: multiple arms select correct one" {
    run $COLONY -c 'match "world" { "hello" => { echo wrong; } "world" => { echo right; } _ => { echo default; } }'
    [ "$status" -eq 0 ]
    [ "$output" = "right" ]
}

# ============================================================================
# Wildcard/Default Arm
# ============================================================================

@test "match: wildcard-only arm always matches" {
    run $COLONY -c 'match 42 { _ => { echo always; } }'
    [ "$status" -eq 0 ]
    [ "$output" = "always" ]
}

# ============================================================================
# Multiple Patterns (Or-Patterns)
# ============================================================================

@test "match: or-pattern matches first alternative" {
    run $COLONY -c 'match 1 { 1 | 2 | 3 => { echo small; } _ => { echo big; } }'
    [ "$status" -eq 0 ]
    [ "$output" = "small" ]
}

@test "match: or-pattern matches second alternative" {
    run $COLONY -c 'match 2 { 1 | 2 | 3 => { echo small; } _ => { echo big; } }'
    [ "$status" -eq 0 ]
    [ "$output" = "small" ]
}

@test "match: or-pattern matches third alternative" {
    run $COLONY -c 'match 3 { 1 | 2 | 3 => { echo small; } _ => { echo big; } }'
    [ "$status" -eq 0 ]
    [ "$output" = "small" ]
}

@test "match: or-pattern no match falls through" {
    run $COLONY -c 'match 5 { 1 | 2 | 3 => { echo small; } _ => { echo big; } }'
    [ "$status" -eq 0 ]
    [ "$output" = "big" ]
}

@test "match: or-pattern with strings" {
    run $COLONY -c 'match "hello" { "hi" | "hello" | "hey" => { echo greeting; } _ => { echo unknown; } }'
    [ "$status" -eq 0 ]
    [ "$output" = "greeting" ]
}

# ============================================================================
# Binding Patterns
# ============================================================================

@test "match: binding captures value" {
    run $COLONY -c 'match 42 { n => { echo "n=$n"; } }'
    [ "$status" -eq 0 ]
    [ "$output" = "n=42" ]
}

@test "match: binding captures string" {
    run $COLONY -c 'match "hello" { s => { echo "s=$s"; } }'
    [ "$status" -eq 0 ]
    [ "$output" = "s=hello" ]
}

# ============================================================================
# Guard Clauses
# ============================================================================

@test "match: guard with truthy value matches" {
    run $COLONY -c 'match 1 { n if n => { echo truthy; } _ => { echo falsy; } }'
    [ "$status" -eq 0 ]
    [ "$output" = "truthy" ]
}

@test "match: guard with empty string skips arm" {
    run $COLONY -c 'match "" { n if n => { echo truthy; } _ => { echo falsy; } }'
    [ "$status" -eq 0 ]
    [ "$output" = "falsy" ]
}

# ============================================================================
# List Pattern Destructuring
# ============================================================================

@test "match: list pattern matches exact length" {
    run $COLONY -c 'match [1, 2, 3] { [a, b, c] => { echo "a=$a b=$b c=$c"; } _ => { echo no; } }'
    [ "$status" -eq 0 ]
    [ "$output" = "a=1 b=2 c=3" ]
}

@test "match: list pattern rejects wrong length" {
    run $COLONY -c 'match [1, 2] { [a, b, c] => { echo wrong; } _ => { echo no; } }'
    [ "$status" -eq 0 ]
    [ "$output" = "no" ]
}

@test "match: list pattern with rest" {
    run $COLONY -c 'match [1, 2, 3, 4, 5] { [a, ..rest] => { echo "first=$a"; } _ => { echo no; } }'
    [ "$status" -eq 0 ]
    [ "$output" = "first=1" ]
}

# ============================================================================
# Dict Pattern Destructuring
# ============================================================================

@test "match: dict pattern matches fields" {
    run $COLONY -c 'match {name: "alice", age: 30} { {name, age} => { echo "name=$name age=$age"; } _ => { echo no; } }'
    [ "$status" -eq 0 ]
    [ "$output" = "name=alice age=30" ]
}

@test "match: dict pattern rejects missing field" {
    run $COLONY -c 'match {name: "alice"} { {name, age} => { echo wrong; } _ => { echo no; } }'
    [ "$status" -eq 0 ]
    [ "$output" = "no" ]
}

# ============================================================================
# Multi-Statement Bodies
# ============================================================================

@test "match: block body runs multiple statements" {
    run $COLONY -c 'match 1 { 1 => { echo first; echo second; } _ => { echo default; } }'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "first" ]
    [ "${lines[1]}" = "second" ]
}

# ============================================================================
# Expression Bodies
# ============================================================================

@test "match: expression body with truthy value" {
    run $COLONY -c 'match 1 { 1 => 1 _ => 0 }'
    [ "$status" -eq 0 ]
}

@test "match: expression body with falsy value" {
    run $COLONY -c 'match 1 { 99 => 1 _ => 0 }'
    [ "$status" -eq 1 ]
}
