#!/usr/bin/env bats
# Test Colony shell proc definitions (F057)
# Colony-specific feature - typed procedures with named params and defaults

COLONY="${COLONY_BIN}"

# ============================================================================
# Basic Proc Definitions
# ============================================================================

@test "proc: define and call simple proc" {
    run $COLONY -c 'proc greet(name) { echo "Hello $name"; }; greet World'
    [ "$status" -eq 0 ]
    [ "$output" = "Hello World" ]
}

@test "proc: define and call proc with no params" {
    run $COLONY -c 'proc hello() { echo hi; }; hello'
    [ "$status" -eq 0 ]
    [ "$output" = "hi" ]
}

@test "proc: multiple params" {
    run $COLONY -c 'proc add(a, b) { echo "$a + $b"; }; add 3 5'
    [ "$status" -eq 0 ]
    [ "$output" = "3 + 5" ]
}

@test "proc: three params" {
    run $COLONY -c 'proc triple(x, y, z) { echo "$x $y $z"; }; triple a b c'
    [ "$status" -eq 0 ]
    [ "$output" = "a b c" ]
}

# ============================================================================
# Default Parameter Values
# ============================================================================

@test "proc: default param not overridden" {
    run $COLONY -c 'proc greet(name, greeting = "Hello") { echo "$greeting $name"; }; greet World'
    [ "$status" -eq 0 ]
    [ "$output" = "Hello World" ]
}

@test "proc: default param overridden" {
    run $COLONY -c 'proc greet(name, greeting = "Hello") { echo "$greeting $name"; }; greet World Hi'
    [ "$status" -eq 0 ]
    [ "$output" = "Hi World" ]
}

@test "proc: multiple defaults" {
    run $COLONY -c 'proc info(name = "anon", age = 0) { echo "$name is $age"; }; info'
    [ "$status" -eq 0 ]
    [ "$output" = "anon is 0" ]
}

@test "proc: default integer value" {
    run $COLONY -c 'proc count(n = 10) { echo "count=$n"; }; count'
    [ "$status" -eq 0 ]
    [ "$output" = "count=10" ]
}

@test "proc: override default with explicit value" {
    run $COLONY -c 'proc count(n = 10) { echo "count=$n"; }; count 42'
    [ "$status" -eq 0 ]
    [ "$output" = "count=42" ]
}

# ============================================================================
# Missing Parameters
# ============================================================================

@test "proc: missing param becomes empty string" {
    run $COLONY -c 'proc show(x) { echo "x=[$x]"; }; show'
    [ "$status" -eq 0 ]
    [ "$output" = "x=[]" ]
}

@test "proc: extra args ignored by named params" {
    run $COLONY -c 'proc one(x) { echo "x=$x"; }; one a b c'
    [ "$status" -eq 0 ]
    [ "$output" = "x=a" ]
}

# ============================================================================
# Multi-Statement Bodies
# ============================================================================

@test "proc: multi-statement body" {
    run $COLONY -c 'proc multi(a, b) { echo "first=$a"; echo "second=$b"; }; multi x y'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "first=x" ]
    [ "${lines[1]}" = "second=y" ]
}

@test "proc: body with conditionals" {
    run $COLONY -c 'proc check(val) { if [ "$val" = "yes" ]; then echo ok; else echo no; fi; }; check yes'
    [ "$status" -eq 0 ]
    [ "$output" = "ok" ]
}

@test "proc: body with loop" {
    run $COLONY -c 'proc repeat(n) { for i in 1 2 3; do echo "$n:$i"; done; }; repeat x'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "x:1" ]
    [ "${lines[1]}" = "x:2" ]
    [ "${lines[2]}" = "x:3" ]
}

# ============================================================================
# Return Statements
# ============================================================================

@test "proc: return with exit code" {
    run $COLONY -c 'proc fail() { return 1; }; fail'
    [ "$status" -eq 1 ]
}

@test "proc: return 0 for success" {
    run $COLONY -c 'proc succeed() { return 0; }; succeed'
    [ "$status" -eq 0 ]
}

@test "proc: return stops execution" {
    run $COLONY -c 'proc early() { echo before; return 0; echo after; }; early'
    [ "$status" -eq 0 ]
    [ "$output" = "before" ]
}

# ============================================================================
# Proc Scope
# ============================================================================

@test "proc: params are local to proc" {
    run $COLONY -c 'x=outer; proc inner(x) { echo "inside=$x"; }; inner inside; echo "outside=$x"'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "inside=inside" ]
    [ "${lines[1]}" = "outside=outer" ]
}

@test "proc: positional params preserved" {
    run $COLONY -c 'set -- original; proc test_pos(a) { echo "$a"; }; test_pos hello; echo "$1"'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "hello" ]
    [ "${lines[1]}" = "original" ]
}

# ============================================================================
# Proc Redefine
# ============================================================================

@test "proc: redefine overwrites" {
    run $COLONY -c 'proc say() { echo first; }; proc say() { echo second; }; say'
    [ "$status" -eq 0 ]
    [ "$output" = "second" ]
}

# ============================================================================
# Proc in Pipeline
# ============================================================================

@test "proc: output in pipeline" {
    run $COLONY -c 'proc nums() { echo "1"; echo "2"; echo "3"; }; nums | wc -l'
    [ "$status" -eq 0 ]
    # wc -l on macOS may have leading whitespace
    result=$(echo "$output" | tr -d ' ')
    [ "$result" = "3" ]
}

# ============================================================================
# Nested Proc Calls
# ============================================================================

@test "proc: proc calls proc" {
    run $COLONY -c 'proc greet(name) { echo "Hi $name"; }; proc formal(name) { greet "Mr. $name"; }; formal Smith'
    [ "$status" -eq 0 ]
    [ "$output" = "Hi Mr. Smith" ]
}
