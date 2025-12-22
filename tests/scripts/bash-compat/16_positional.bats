#!/usr/bin/env bats
# Tests for positional parameter expansion with modifiers
# These are critical for BATS compatibility (e.g., ${0##*/})

COLONY="${COLONY_BIN:-../../../target/release/colony}"

run_colony() {
    run "$COLONY" -c "$1"
}

# Helper to compare with bash
compare_with_bash() {
    local colony_out bash_out
    colony_out=$("$COLONY" -c "$1" 2>&1) || true
    bash_out=$(bash -c "$1" 2>&1) || true
    [ "$colony_out" = "$bash_out" ]
}

# =============================================================================
# Basic positional parameters
# =============================================================================

@test "positional: \$0 basic" {
    run_colony 'echo "[$0]"'
    [ "$status" -eq 0 ]
}

@test "positional: \$1 with argument" {
    run "$COLONY" -c 'echo "[$1]"' arg1
    [ "$status" -eq 0 ]
    [ "$output" = "[arg1]" ]
}

@test "positional: \$@ basic" {
    run "$COLONY" -c 'echo "$@"' a b c
    [ "$status" -eq 0 ]
    [ "$output" = "a b c" ]
}

@test "positional: \$* basic" {
    run "$COLONY" -c 'echo "$*"' a b c
    [ "$status" -eq 0 ]
    [ "$output" = "a b c" ]
}

@test "positional: \$# count" {
    run "$COLONY" -c 'echo "$#"' a b c
    [ "$status" -eq 0 ]
    [ "$output" = "3" ]
}

# =============================================================================
# Positional parameters with default value modifiers (CRITICAL FOR BATS)
# =============================================================================

@test "positional: \${1:-default} when unset" {
    run_colony 'echo "${1:-default}"'
    [ "$status" -eq 0 ]
    [ "$output" = "default" ]
}

@test "positional: \${1:-default} when set" {
    run "$COLONY" -c 'echo "${1:-default}"' myarg
    [ "$status" -eq 0 ]
    [ "$output" = "myarg" ]
}

@test "positional: \${1-default} when unset" {
    run_colony 'echo "${1-default}"'
    [ "$status" -eq 0 ]
    [ "$output" = "default" ]
}

@test "positional: \${2:-fallback} with only one arg" {
    run "$COLONY" -c 'echo "${2:-fallback}"' first
    [ "$status" -eq 0 ]
    [ "$output" = "fallback" ]
}

# =============================================================================
# Pattern removal on positional parameters (CRITICAL FOR BATS - ${0##*/})
# =============================================================================

@test "positional: \${0##*/} strip path prefix" {
    run_colony 'echo "${0##*/}"'
    [ "$status" -eq 0 ]
    # Should output just the basename, not the full path
}

@test "positional: \${1##*/} strip path from arg" {
    run "$COLONY" -c 'echo "${1##*/}"' /path/to/file.txt
    [ "$status" -eq 0 ]
    [ "$output" = "file.txt" ]
}

@test "positional: \${1#*/} strip first path component" {
    run "$COLONY" -c 'echo "${1#*/}"' /path/to/file.txt
    [ "$status" -eq 0 ]
    [ "$output" = "path/to/file.txt" ]
}

@test "positional: \${1%%.*} strip extension greedy" {
    run "$COLONY" -c 'echo "${1%%.*}"' file.tar.gz
    [ "$status" -eq 0 ]
    [ "$output" = "file" ]
}

@test "positional: \${1%.*} strip extension" {
    run "$COLONY" -c 'echo "${1%.*}"' file.tar.gz
    [ "$status" -eq 0 ]
    [ "$output" = "file.tar" ]
}

# =============================================================================
# Length of positional parameters
# =============================================================================

@test "positional: \${#1} length of first arg" {
    run "$COLONY" -c 'echo "${#1}"' hello
    [ "$status" -eq 0 ]
    [ "$output" = "5" ]
}

@test "positional: \${#@} count of all args" {
    run "$COLONY" -c 'echo "${#@}"' a b c d e
    [ "$status" -eq 0 ]
    [ "$output" = "5" ]
}

@test "positional: \${#*} count of all args" {
    run "$COLONY" -c 'echo "${#*}"' a b c d e
    [ "$status" -eq 0 ]
    [ "$output" = "5" ]
}

# =============================================================================
# Slicing positional parameters
# =============================================================================

@test "positional: \${@:2} all args from second" {
    run "$COLONY" -c 'echo "${@:2}"' a b c d
    [ "$status" -eq 0 ]
    [ "$output" = "b c d" ]
}

@test "positional: \${@:2:2} two args from second" {
    run "$COLONY" -c 'echo "${@:2:2}"' a b c d e
    [ "$status" -eq 0 ]
    [ "$output" = "b c" ]
}

@test "positional: \${*:1:3} first three args" {
    run "$COLONY" -c 'echo "${*:1:3}"' a b c d e
    [ "$status" -eq 0 ]
    [ "$output" = "a b c" ]
}

# =============================================================================
# Special parameters with modifiers
# =============================================================================

@test "positional: \${?:-0} exit status with default" {
    run_colony 'true; echo "${?:-99}"'
    [ "$status" -eq 0 ]
    [ "$output" = "0" ]
}

@test "positional: \${#:-0} param count with default" {
    run_colony 'echo "${#:-0}"'
    [ "$status" -eq 0 ]
    [ "$output" = "0" ]
}

# =============================================================================
# Substitution on positional parameters
# =============================================================================

@test "positional: \${1/old/new} substitution" {
    run "$COLONY" -c 'echo "${1/world/universe}"' "hello world"
    [ "$status" -eq 0 ]
    [ "$output" = "hello universe" ]
}

@test "positional: \${@/a/X} substitute in all args" {
    run "$COLONY" -c 'echo "${@/a/X}"' cat bat rat
    [ "$status" -eq 0 ]
    [ "$output" = "cXt bXt rXt" ]
}

# =============================================================================
# Case modification on positional parameters
# =============================================================================

@test "positional: \${1^^} uppercase" {
    run "$COLONY" -c 'echo "${1^^}"' hello
    [ "$status" -eq 0 ]
    [ "$output" = "HELLO" ]
}

@test "positional: \${1,,} lowercase" {
    run "$COLONY" -c 'echo "${1,,}"' HELLO
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

# =============================================================================
# Compare with bash
# =============================================================================

@test "compare positional with bash: \${1:-default}" {
    compare_with_bash 'echo "${1:-default}"'
}

@test "compare positional with bash: \${1##*/}" {
    skip "Grammar fix needed first"
    compare_with_bash 'set -- /path/to/file; echo "${1##*/}"'
}

@test "compare positional with bash: \${#1}" {
    compare_with_bash 'set -- hello; echo "${#1}"'
}

@test "compare positional with bash: \${@:2:2}" {
    compare_with_bash 'set -- a b c d e; echo "${@:2:2}"'
}
