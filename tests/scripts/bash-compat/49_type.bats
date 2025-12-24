#!/usr/bin/env bats
# Test type builtin
# Based on bash's type.tests

COLONY="${COLONY_BIN:-../../target/release/colony}"

# =============================================================================
# Basic type command
# =============================================================================

@test "type: builtin command" {
    run "$COLONY" -c 'type echo'
    [ "$status" -eq 0 ]
    [[ "$output" == *"builtin"* ]]
}

@test "type: external command" {
    run "$COLONY" -c 'type ls'
    [ "$status" -eq 0 ]
    [[ "$output" == *"/"* ]] || [[ "$output" == *"alias"* ]]
}

@test "type: function" {
    run "$COLONY" -c 'myfunc() { echo hello; }; type myfunc'
    [ "$status" -eq 0 ]
    [[ "$output" == *"function"* ]]
}

@test "type: alias" {
    run "$COLONY" -c 'alias myalias=echo; type myalias'
    [ "$status" -eq 0 ]
    [[ "$output" == *"alias"* ]]
}

@test "type: keyword" {
    run "$COLONY" -c 'type if'
    [ "$status" -eq 0 ]
    [[ "$output" == *"keyword"* ]] || [[ "$output" == *"reserved"* ]]
}

@test "type: not found" {
    run "$COLONY" -c 'type nonexistent_command_xyz'
    [ "$status" -ne 0 ]
    [[ "$output" == *"not found"* ]]
}

# =============================================================================
# type -t (type only)
# =============================================================================

@test "type: -t builtin" {
    run "$COLONY" -c 'type -t echo'
    [ "$status" -eq 0 ]
    [ "$output" = "builtin" ]
}

@test "type: -t external" {
    run "$COLONY" -c 'type -t ls'
    [ "$status" -eq 0 ]
    [ "$output" = "file" ] || [ "$output" = "alias" ]
}

@test "type: -t function" {
    run "$COLONY" -c 'myfunc() { :; }; type -t myfunc'
    [ "$status" -eq 0 ]
    [ "$output" = "function" ]
}

@test "type: -t alias" {
    run "$COLONY" -c 'alias myalias=ls; type -t myalias'
    [ "$status" -eq 0 ]
    [ "$output" = "alias" ]
}

@test "type: -t keyword" {
    run "$COLONY" -c 'type -t for'
    [ "$status" -eq 0 ]
    [ "$output" = "keyword" ]
}

@test "type: -t not found returns nothing" {
    run "$COLONY" -c 'type -t nonexistent_xyz'
    [ "$status" -ne 0 ]
    [ -z "$output" ]
}

# =============================================================================
# type -p (path only)
# =============================================================================

@test "type: -p external command" {
    run "$COLONY" -c 'type -p ls'
    [ "$status" -eq 0 ]
    [[ "$output" == "/"* ]]
}

@test "type: -p builtin returns nothing" {
    run "$COLONY" -c 'type -p echo'
    [ "$status" -eq 0 ]
    [ -z "$output" ]
}

@test "type: -p function returns nothing" {
    run "$COLONY" -c 'myfunc() { :; }; type -p myfunc'
    [ "$status" -eq 0 ]
    [ -z "$output" ]
}

@test "type: -p alias returns nothing" {
    run "$COLONY" -c 'alias myalias=ls; type -p myalias'
    [ "$status" -eq 0 ]
    [ -z "$output" ]
}

@test "type: -p not found" {
    run "$COLONY" -c 'type -p nonexistent_xyz'
    [ "$status" -ne 0 ]
}

# =============================================================================
# type -P (force PATH search)
# =============================================================================

@test "type: -P searches PATH" {
    run "$COLONY" -c 'type -P ls'
    [ "$status" -eq 0 ]
    [[ "$output" == "/"* ]]
}

@test "type: -P ignores builtins" {
    run "$COLONY" -c 'type -P echo'
    [ "$status" -eq 0 ]
    [[ "$output" == "/"* ]]
}

@test "type: -P ignores functions" {
    run "$COLONY" -c 'ls() { :; }; type -P ls'
    [ "$status" -eq 0 ]
    [[ "$output" == "/"* ]]
}

# =============================================================================
# type -a (all locations)
# =============================================================================

@test "type: -a shows all" {
    run "$COLONY" -c 'type -a echo'
    [ "$status" -eq 0 ]
    [[ "$output" == *"builtin"* ]]
}

@test "type: -a shows function and builtin" {
    run "$COLONY" -c 'echo() { builtin echo modified; }; type -a echo'
    [ "$status" -eq 0 ]
    [[ "$output" == *"function"* ]]
    [[ "$output" == *"builtin"* ]]
}

@test "type: -a shows alias and external" {
    run "$COLONY" -c 'alias ls="ls -la"; type -a ls'
    [ "$status" -eq 0 ]
    [[ "$output" == *"alias"* ]]
}

# =============================================================================
# type -f (skip functions)
# =============================================================================

@test "type: -f ignores function" {
    run "$COLONY" -c 'echo() { builtin echo modified; }; type -f echo'
    [ "$status" -eq 0 ]
    [[ "$output" == *"builtin"* ]]
    [[ "$output" != *"function"* ]]
}

@test "type: -f with no function" {
    run "$COLONY" -c 'type -f ls'
    [ "$status" -eq 0 ]
}

# =============================================================================
# Multiple commands
# =============================================================================

@test "type: multiple commands" {
    run "$COLONY" -c 'type echo ls'
    [ "$status" -eq 0 ]
    [[ "$output" == *"echo"* ]]
    [[ "$output" == *"ls"* ]]
}

@test "type: multiple with one not found" {
    run "$COLONY" -c 'type echo nonexistent_xyz ls'
    [ "$status" -ne 0 ]
    [[ "$output" == *"echo"* ]]
    [[ "$output" == *"not found"* ]]
}

# =============================================================================
# Combined flags
# =============================================================================

@test "type: -at shows all types" {
    run "$COLONY" -c 'echo() { :; }; type -at echo'
    [ "$status" -eq 0 ]
    [[ "$output" == *"function"* ]]
    [[ "$output" == *"builtin"* ]]
}

@test "type: -ap shows all paths" {
    run "$COLONY" -c 'type -ap ls'
    [ "$status" -eq 0 ]
}

# =============================================================================
# Keywords
# =============================================================================

@test "type: if keyword" {
    run "$COLONY" -c 'type if'
    [ "$status" -eq 0 ]
    [[ "$output" == *"keyword"* ]] || [[ "$output" == *"reserved"* ]]
}

@test "type: then keyword" {
    run "$COLONY" -c 'type then'
    [ "$status" -eq 0 ]
    [[ "$output" == *"keyword"* ]] || [[ "$output" == *"reserved"* ]]
}

@test "type: while keyword" {
    run "$COLONY" -c 'type while'
    [ "$status" -eq 0 ]
    [[ "$output" == *"keyword"* ]] || [[ "$output" == *"reserved"* ]]
}

@test "type: for keyword" {
    run "$COLONY" -c 'type for'
    [ "$status" -eq 0 ]
    [[ "$output" == *"keyword"* ]] || [[ "$output" == *"reserved"* ]]
}

@test "type: case keyword" {
    run "$COLONY" -c 'type case'
    [ "$status" -eq 0 ]
    [[ "$output" == *"keyword"* ]] || [[ "$output" == *"reserved"* ]]
}

@test "type: function keyword" {
    run "$COLONY" -c 'type function'
    [ "$status" -eq 0 ]
    [[ "$output" == *"keyword"* ]] || [[ "$output" == *"reserved"* ]]
}

# =============================================================================
# Builtins
# =============================================================================

@test "type: cd builtin" {
    run "$COLONY" -c 'type cd'
    [ "$status" -eq 0 ]
    [[ "$output" == *"builtin"* ]]
}

@test "type: pwd builtin" {
    run "$COLONY" -c 'type pwd'
    [ "$status" -eq 0 ]
    [[ "$output" == *"builtin"* ]]
}

@test "type: export builtin" {
    run "$COLONY" -c 'type export'
    [ "$status" -eq 0 ]
    [[ "$output" == *"builtin"* ]]
}

@test "type: read builtin" {
    run "$COLONY" -c 'type read'
    [ "$status" -eq 0 ]
    [[ "$output" == *"builtin"* ]]
}

@test "type: type builtin" {
    run "$COLONY" -c 'type type'
    [ "$status" -eq 0 ]
    [[ "$output" == *"builtin"* ]]
}

# =============================================================================
# Compare with bash
# =============================================================================

@test "compare type with bash: builtin" {
    colony_out=$("$COLONY" -c 'type echo' 2>&1)
    bash_out=$(bash -c 'type echo' 2>&1)
    [[ "$colony_out" == *"builtin"* ]] && [[ "$bash_out" == *"builtin"* ]]
}

@test "compare type with bash: -t builtin" {
    colony_out=$("$COLONY" -c 'type -t echo' 2>&1)
    bash_out=$(bash -c 'type -t echo' 2>&1)
    [ "$colony_out" = "$bash_out" ]
}

@test "compare type with bash: -t keyword" {
    colony_out=$("$COLONY" -c 'type -t if' 2>&1)
    bash_out=$(bash -c 'type -t if' 2>&1)
    [ "$colony_out" = "$bash_out" ]
}

@test "compare type with bash: not found" {
    run "$COLONY" -c 'type nonexistent_xyz_123'
    [ "$status" -ne 0 ]
    run bash -c 'type nonexistent_xyz_123'
    [ "$status" -ne 0 ]
}
