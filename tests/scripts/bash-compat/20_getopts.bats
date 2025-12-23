#!/usr/bin/env bats
# Test Colony shell getopts builtin for POSIX compliance
# Based on bash getopts.tests from https://github.com/bminor/bash

# Path to colony binary (use absolute path set by runner)
COLONY="${COLONY_BIN}"

setup() {
    # Create temp directory for test files
    TEST_TEMP="$(mktemp -d)"
    export TEST_TEMP
    # Reset OPTIND at start of each test
    unset OPTIND
}

teardown() {
    # Clean up temp directory
    rm -rf "$TEST_TEMP"
}

# =============================================================================
# Basic error handling
# =============================================================================

@test "getopts - no arguments should fail" {
    run "$COLONY" -c "getopts"
    [ "$status" -ne 0 ]
}

@test "getopts - missing variable name should fail" {
    run "$COLONY" -c "getopts opts"
    [ "$status" -ne 0 ]
}

# =============================================================================
# Basic option parsing
# =============================================================================

@test "getopts - single option no argument" {
    run "$COLONY" -c '
set -- -a
while getopts "a" opt; do
    echo "opt=$opt"
done
'
    [ "$status" -eq 0 ]
    [ "$output" = "opt=a" ]
}

@test "getopts - multiple separate options" {
    run "$COLONY" -c '
set -- -a -b -c
while getopts "abc" opt; do
    echo "opt=$opt"
done
'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "opt=a" ]
    [ "${lines[1]}" = "opt=b" ]
    [ "${lines[2]}" = "opt=c" ]
}

@test "getopts - combined options (-abc)" {
    run "$COLONY" -c '
set -- -abc
while getopts "abc" opt; do
    echo "opt=$opt"
done
'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "opt=a" ]
    [ "${lines[1]}" = "opt=b" ]
    [ "${lines[2]}" = "opt=c" ]
}

# =============================================================================
# Options with arguments
# =============================================================================

@test "getopts - option with separate argument" {
    run "$COLONY" -c '
set -- -b bval
while getopts "ab:" opt; do
    echo "opt=$opt OPTARG=$OPTARG"
done
'
    [ "$status" -eq 0 ]
    [ "$output" = "opt=b OPTARG=bval" ]
}

@test "getopts - option with attached argument (-bvalue)" {
    run "$COLONY" -c '
set -- -bbval
while getopts "ab:" opt; do
    echo "opt=$opt OPTARG=$OPTARG"
done
'
    [ "$status" -eq 0 ]
    [ "$output" = "opt=b OPTARG=bval" ]
}

@test "getopts - mixed options with and without arguments" {
    run "$COLONY" -c '
set -- -a -b bval -c
while getopts "ab:c" opt; do
    echo "opt=$opt OPTARG=$OPTARG"
done
'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "opt=a OPTARG=" ]
    [ "${lines[1]}" = "opt=b OPTARG=bval" ]
    [ "${lines[2]}" = "opt=c OPTARG=" ]
}

@test "getopts - option argument with spaces" {
    run "$COLONY" -c '
set -- -b "value with spaces"
while getopts "ab:" opt; do
    echo "opt=$opt OPTARG=[$OPTARG]"
done
'
    [ "$status" -eq 0 ]
    [ "$output" = "opt=b OPTARG=[value with spaces]" ]
}

# =============================================================================
# OPTIND behavior
# =============================================================================

@test "getopts - OPTIND starts at 1" {
    run "$COLONY" -c '
set -- -a -b
echo "before: OPTIND=$OPTIND"
getopts "ab" opt
echo "after: OPTIND=$OPTIND"
'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "before: OPTIND=1" ]
    # After processing -a, OPTIND should be 2
    [ "${lines[1]}" = "after: OPTIND=2" ]
}

@test "getopts - OPTIND after all options" {
    run "$COLONY" -c '
set -- -a -b arg1 arg2
while getopts "ab" opt; do
    :
done
echo "OPTIND=$OPTIND"
shift $((OPTIND - 1))
echo "remaining: $@"
'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "OPTIND=3" ]
    [ "${lines[1]}" = "remaining: arg1 arg2" ]
}

@test "getopts - OPTIND reset for reparsing" {
    run "$COLONY" -c '
set -- -a -b
while getopts "ab" opt; do
    echo "first: $opt"
done
OPTIND=1
while getopts "ab" opt; do
    echo "second: $opt"
done
'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "first: a" ]
    [ "${lines[1]}" = "first: b" ]
    [ "${lines[2]}" = "second: a" ]
    [ "${lines[3]}" = "second: b" ]
}

# =============================================================================
# Error handling - verbose mode (default)
# =============================================================================

@test "getopts - invalid option in verbose mode" {
    run "$COLONY" -c '
set -- -z
while getopts "ab" opt; do
    echo "opt=$opt"
done
' 2>&1
    [ "$status" -eq 0 ]
    # opt should be ? for invalid option
    [[ "$output" == *"opt=?"* ]]
}

@test "getopts - missing argument in verbose mode" {
    run "$COLONY" -c '
set -- -b
while getopts "ab:" opt; do
    echo "opt=$opt"
done
' 2>&1
    [ "$status" -eq 0 ]
    # opt should be ? for missing argument
    [[ "$output" == *"opt=?"* ]]
}

# =============================================================================
# Error handling - silent mode (optstring starts with :)
# =============================================================================

@test "getopts - invalid option in silent mode" {
    run "$COLONY" -c '
set -- -z
while getopts ":ab" opt; do
    echo "opt=$opt OPTARG=$OPTARG"
done
'
    [ "$status" -eq 0 ]
    # In silent mode, opt=? and OPTARG=invalid option char
    [ "$output" = "opt=? OPTARG=z" ]
}

@test "getopts - missing argument in silent mode" {
    run "$COLONY" -c '
set -- -b
while getopts ":ab:" opt; do
    echo "opt=$opt OPTARG=$OPTARG"
done
'
    [ "$status" -eq 0 ]
    # In silent mode, opt=: and OPTARG=option requiring arg
    [ "$output" = "opt=: OPTARG=b" ]
}

# =============================================================================
# Double dash (--) handling
# =============================================================================

@test "getopts - double dash ends option processing" {
    run "$COLONY" -c '
set -- -a -- -b -c
while getopts "abc" opt; do
    echo "opt=$opt"
done
shift $((OPTIND - 1))
echo "remaining: $@"
'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "opt=a" ]
    [ "${lines[1]}" = "remaining: -b -c" ]
}

# =============================================================================
# Return value
# =============================================================================

@test "getopts - returns 0 when option found" {
    run "$COLONY" -c '
set -- -a
getopts "a" opt
echo "status=$?"
'
    [ "$status" -eq 0 ]
    [ "$output" = "status=0" ]
}

@test "getopts - returns 1 when no more options" {
    run "$COLONY" -c '
set -- -a
getopts "a" opt
getopts "a" opt
echo "status=$?"
'
    [ "$status" -eq 0 ]
    [ "$output" = "status=1" ]
}

@test "getopts - returns 1 for non-option argument" {
    run "$COLONY" -c '
set -- arg
getopts "a" opt
echo "status=$?"
'
    [ "$status" -eq 0 ]
    [ "$output" = "status=1" ]
}

# =============================================================================
# Custom argument list
# =============================================================================

@test "getopts - custom args instead of positional params" {
    run "$COLONY" -c '
OPTIND=1
getopts "ab:" opt -a
echo "opt=$opt"
OPTIND=1
getopts "ab:" opt -b bval
echo "opt=$opt OPTARG=$OPTARG"
'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "opt=a" ]
    [ "${lines[1]}" = "opt=b OPTARG=bval" ]
}

# =============================================================================
# Compare with bash behavior
# =============================================================================

@test "compare getopts basic with bash" {
    local script='
set -- -a -b bval -c
while getopts "ab:c" opt; do
    echo "opt=$opt OPTARG=$OPTARG"
done
'
    run bash -c "$script"
    local bash_output="$output"
    
    run "$COLONY" -c "$script"
    local colony_output="$output"
    
    [ "$colony_output" = "$bash_output" ]
}

@test "compare getopts combined options with bash" {
    local script='
set -- -abc
while getopts "abc" opt; do
    echo "opt=$opt"
done
'
    run bash -c "$script"
    local bash_output="$output"
    
    run "$COLONY" -c "$script"
    local colony_output="$output"
    
    [ "$colony_output" = "$bash_output" ]
}

@test "compare getopts attached argument with bash" {
    local script='
set -- -bbval
while getopts "ab:" opt; do
    echo "opt=$opt OPTARG=$OPTARG"
done
'
    run bash -c "$script"
    local bash_output="$output"
    
    run "$COLONY" -c "$script"
    local colony_output="$output"
    
    [ "$colony_output" = "$bash_output" ]
}

@test "compare getopts silent mode invalid with bash" {
    local script='
set -- -z
while getopts ":ab" opt; do
    echo "opt=$opt OPTARG=$OPTARG"
done
'
    run bash -c "$script"
    local bash_output="$output"
    
    run "$COLONY" -c "$script"
    local colony_output="$output"
    
    [ "$colony_output" = "$bash_output" ]
}

@test "compare getopts silent mode missing arg with bash" {
    local script='
set -- -b
while getopts ":ab:" opt; do
    echo "opt=$opt OPTARG=$OPTARG"
done
'
    run bash -c "$script"
    local bash_output="$output"
    
    run "$COLONY" -c "$script"
    local colony_output="$output"
    
    [ "$colony_output" = "$bash_output" ]
}

# =============================================================================
# Edge cases
# =============================================================================

@test "getopts - empty positional params returns 1" {
    run "$COLONY" -c '
set --
getopts "a" opt
echo "status=$?"
'
    [ "$status" -eq 0 ]
    [ "$output" = "status=1" ]
}

@test "getopts - option after non-option stops processing" {
    run "$COLONY" -c '
set -- -a arg -b
count=0
while getopts "ab" opt; do
    count=$((count + 1))
done
echo "count=$count"
'
    [ "$status" -eq 0 ]
    # POSIX: only -a should be processed, then arg stops option processing
    [ "$output" = "count=1" ]
}

@test "getopts - multiple calls share state via OPTIND" {
    run "$COLONY" -c '
set -- -a -b -c
getopts "abc" opt
echo "1: opt=$opt OPTIND=$OPTIND"
getopts "abc" opt
echo "2: opt=$opt OPTIND=$OPTIND"
getopts "abc" opt
echo "3: opt=$opt OPTIND=$OPTIND"
'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "1: opt=a OPTIND=2" ]
    [ "${lines[1]}" = "2: opt=b OPTIND=3" ]
    [ "${lines[2]}" = "3: opt=c OPTIND=4" ]
}

@test "getopts - more than 9 positional params" {
    run "$COLONY" -c '
set -- -a -b bval one two three four five six seven eight nine ten eleven twelve
while getopts "ab:" opt; do
    echo "opt=$opt OPTARG=$OPTARG"
done
shift $((OPTIND - 1))
echo "remaining: $#"
'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "opt=a OPTARG=" ]
    [ "${lines[1]}" = "opt=b OPTARG=bval" ]
    [ "${lines[2]}" = "remaining: 12" ]
}

@test "getopts - OPTERR=0 suppresses error messages" {
    run "$COLONY" -c '
OPTERR=0
set -- -z
getopts "ab" opt
echo "opt=$opt"
' 2>&1
    [ "$status" -eq 0 ]
    # Should only have opt=? without error message
    [ "$output" = "opt=?" ]
}
