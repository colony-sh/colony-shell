#!/usr/bin/env bats
# Test Colony shell case statements for POSIX compliance
# Compare behavior against bash

COLONY="${COLONY_BIN}"

# ============================================================================
# Basic Case Statements
# ============================================================================

@test "case: simple exact match" {
    run $COLONY -c 'case foo in foo) echo matched;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "matched" ]
}

@test "case: no match uses default" {
    run $COLONY -c 'case foo in bar) echo wrong;; *) echo default;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "default" ]
}

@test "case: first match wins" {
    run $COLONY -c 'case foo in foo) echo first;; foo) echo second;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "first" ]
}

@test "case: empty case no output" {
    run $COLONY -c 'case foo in bar) echo wrong;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "" ]
}

# ============================================================================
# Pattern Matching
# ============================================================================

@test "case: wildcard * matches anything" {
    run $COLONY -c 'case hello in h*) echo matched;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "matched" ]
}

@test "case: wildcard ? matches single char" {
    run $COLONY -c 'case hi in h?) echo matched;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "matched" ]
}

@test "case: pattern prefix match" {
    run $COLONY -c 'case foobar in foo*) echo prefix;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "prefix" ]
}

@test "case: pattern suffix match" {
    run $COLONY -c 'case foobar in *bar) echo suffix;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "suffix" ]
}

@test "case: pattern contains match" {
    run $COLONY -c 'case foobar in *oba*) echo contains;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "contains" ]
}

@test "case: bracket pattern" {
    run $COLONY -c 'case a in [abc]) echo matched;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "matched" ]
}

@test "case: bracket range pattern" {
    run $COLONY -c 'case m in [a-z]) echo matched;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "matched" ]
}

@test "case: bracket negation pattern" {
    run $COLONY -c 'case 5 in [!a-z]) echo matched;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "matched" ]
}

# ============================================================================
# Multiple Patterns (OR)
# ============================================================================

@test "case: multiple patterns with |" {
    run $COLONY -c 'case bar in foo|bar|baz) echo matched;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "matched" ]
}

@test "case: multiple patterns second match" {
    run $COLONY -c 'case world in hello|world) echo matched;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "matched" ]
}

@test "case: multiple patterns no match" {
    run $COLONY -c 'case xyz in foo|bar|baz) echo wrong;; *) echo default;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "default" ]
}

# ============================================================================
# Case Terminators: ;; (break)
# ============================================================================

@test "case: ;; terminates normally" {
    run $COLONY -c 'case foo in foo) echo first;; bar) echo second;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "first" ]
}

@test "case: ;; stops at first match" {
    run $COLONY -c 'case test in t*) echo match1;; test) echo match2;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "match1" ]
}

# ============================================================================
# Case Terminators: ;& (fallthrough)
# ============================================================================

@test "case: ;& falls through to next arm" {
    run $COLONY -c 'case foo in foo) echo first;& bar) echo second;; esac'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "first" ]
    [ "${lines[1]}" = "second" ]
}

@test "case: ;& fallthrough chain" {
    run $COLONY -c 'case foo in foo) echo one;& bar) echo two;& baz) echo three;; esac'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "one" ]
    [ "${lines[1]}" = "two" ]
    [ "${lines[2]}" = "three" ]
}

@test "case: ;& stops at ;;" {
    run $COLONY -c 'case foo in foo) echo one;& bar) echo two;; baz) echo three;; esac'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "one" ]
    [ "${lines[1]}" = "two" ]
    [ "${#lines[@]}" -eq 2 ]
}

# ============================================================================
# Case Terminators: ;;& (continue matching)
# ============================================================================

@test "case: ;;& continues pattern testing" {
    run $COLONY -c 'case foobar in foo*) echo prefix;;& *bar) echo suffix;; esac'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "prefix" ]
    [ "${lines[1]}" = "suffix" ]
}

@test "case: ;;& skips non-matching arms" {
    run $COLONY -c 'case foobar in foo*) echo match1;;& xxx) echo skip;;& *bar) echo match2;; esac'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "match1" ]
    [ "${lines[1]}" = "match2" ]
    [ "${#lines[@]}" -eq 2 ]
}

@test "case: ;;& stops at ;" {
    run $COLONY -c 'case foo in f*) echo one;;& foo) echo two;; bar) echo three;; esac'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "one" ]
    [ "${lines[1]}" = "two" ]
    [ "${#lines[@]}" -eq 2 ]
}

# ============================================================================
# Variable Expansion in Patterns
# ============================================================================

@test "case: variable in value" {
    run $COLONY -c 'x=foo; case "$x" in foo) echo matched;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "matched" ]
}

@test "case: variable in pattern" {
    run $COLONY -c 'pat=foo; case foo in "$pat") echo matched;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "matched" ]
}

@test "case: empty variable matches empty string" {
    run $COLONY -c 'unset x; case "$x" in "") echo empty;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "empty" ]
}

# ============================================================================
# Multiple Commands in Arm
# ============================================================================

@test "case: multiple commands in arm" {
    run $COLONY -c 'case foo in foo) echo first; echo second;; esac'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "first" ]
    [ "${lines[1]}" = "second" ]
}

@test "case: command list in arm" {
    run $COLONY -c 'case foo in foo) echo a && echo b || echo c;; esac'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "a" ]
    [ "${lines[1]}" = "b" ]
}

@test "case: pipeline in arm" {
    run $COLONY -c 'case foo in foo) echo hello | cat;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

# ============================================================================
# Edge Cases
# ============================================================================

@test "case: reserved word as pattern" {
    run $COLONY -c 'case esac in esac) echo matched;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "matched" ]
}

@test "case: special chars in value" {
    run $COLONY -c 'case "]" in "]") echo matched;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "matched" ]
}

@test "case: space in pattern" {
    run $COLONY -c 'case " " in " ") echo matched;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "matched" ]
}

@test "case: empty arm body" {
    run $COLONY -c 'case foo in foo) ;; esac; echo after'
    [ "$status" -eq 0 ]
    [ "$output" = "after" ]
}

@test "case: numeric value" {
    run $COLONY -c 'case 42 in 42) echo matched;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "matched" ]
}

@test "case: case inside case" {
    run $COLONY -c 'case a in a) case b in b) echo nested;; esac;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "nested" ]
}

# ============================================================================
# Exit Status
# ============================================================================

@test "case: exit status 0 on match" {
    run $COLONY -c 'case foo in foo) true;; esac'
    [ "$status" -eq 0 ]
}

@test "case: exit status from last command" {
    run $COLONY -c 'case foo in foo) false;; esac'
    [ "$status" -eq 1 ]
}

@test "case: exit status 0 on no match" {
    run $COLONY -c 'case foo in bar) false;; esac'
    [ "$status" -eq 0 ]
}

# ============================================================================
# Compare with Bash
# ============================================================================

@test "compare: basic case with bash" {
    colony_out=$($COLONY -c 'case test in test) echo match;; esac')
    bash_out=$(bash -c 'case test in test) echo match;; esac')
    [ "$colony_out" = "$bash_out" ]
}

@test "compare: fallthrough with bash" {
    colony_out=$($COLONY -c 'case x in x) echo a;& y) echo b;; esac')
    bash_out=$(bash -c 'case x in x) echo a;& y) echo b;; esac')
    [ "$colony_out" = "$bash_out" ]
}

@test "compare: continue with bash" {
    colony_out=$($COLONY -c 'case ab in a*) echo 1;;& *b) echo 2;; esac')
    bash_out=$(bash -c 'case ab in a*) echo 1;;& *b) echo 2;; esac')
    [ "$colony_out" = "$bash_out" ]
}

@test "compare: multiple patterns with bash" {
    colony_out=$($COLONY -c 'case b in a|b|c) echo match;; esac')
    bash_out=$(bash -c 'case b in a|b|c) echo match;; esac')
    [ "$colony_out" = "$bash_out" ]
}

@test "compare: wildcard patterns with bash" {
    colony_out=$($COLONY -c 'case hello in h*o) echo match;; esac')
    bash_out=$(bash -c 'case hello in h*o) echo match;; esac')
    [ "$colony_out" = "$bash_out" ]
}
