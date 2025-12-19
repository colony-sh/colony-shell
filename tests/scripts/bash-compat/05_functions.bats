#!/usr/bin/env bats
# Test Colony shell function support for POSIX compliance
# Functions are a core POSIX shell feature

COLONY="${COLONY_BIN}"

setup() {
    TEST_TEMP="$(mktemp -d)"
    export TEST_TEMP
}

teardown() {
    rm -rf "$TEST_TEMP"
}

# ============================================================================
# POSIX Function Definition - name() { body }
# ============================================================================

@test "function: POSIX syntax - name() { body }" {
    run "$COLONY" -c 'greet() { echo hello; }; greet'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "function: bash syntax - function name { body }" {
    run "$COLONY" -c 'function greet { echo hello; }; greet'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "function: bash syntax with parens - function name() { body }" {
    run "$COLONY" -c 'function greet() { echo hello; }; greet'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "function: call after definition" {
    run "$COLONY" -c 'hello() { echo world; }; hello'
    [ "$status" -eq 0 ]
    [ "$output" = "world" ]
}

@test "function: multiple calls" {
    run "$COLONY" -c 'msg() { echo test; }; msg; msg; msg'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "test" ]
    [ "${lines[1]}" = "test" ]
    [ "${lines[2]}" = "test" ]
}

# ============================================================================
# Function Arguments - Positional Parameters
# ============================================================================

@test "function: positional parameter \$1" {
    run "$COLONY" -c 'greet() { echo hello $1; }; greet world'
    [ "$status" -eq 0 ]
    [ "$output" = "hello world" ]
}

@test "function: multiple positional parameters" {
    run "$COLONY" -c 'add() { echo $1 + $2; }; add 5 10'
    [ "$status" -eq 0 ]
    [ "$output" = "5 + 10" ]
}

@test "function: \$@ expands to all arguments" {
    run "$COLONY" -c 'showargs() { echo $@; }; showargs one two three'
    [ "$status" -eq 0 ]
    [ "$output" = "one two three" ]
}

@test "function: \$* expands to all arguments" {
    run "$COLONY" -c 'showargs() { echo $*; }; showargs a b c'
    [ "$status" -eq 0 ]
    [ "$output" = "a b c" ]
}

@test "function: \$# is argument count" {
    run "$COLONY" -c 'count() { echo $#; }; count one two three'
    [ "$status" -eq 0 ]
    [ "$output" = "3" ]
}

@test "function: \$0 is shell name, not function name" {
    skip "Positional parameter \$0 needs implementation"
    run "$COLONY" -c 'showname() { echo $0; }; showname'
    [ "$status" -eq 0 ]
}

@test "function: no arguments gives empty \$@" {
    run "$COLONY" -c 'noargs() { echo ">$@<"; }; noargs'
    [ "$status" -eq 0 ]
    [ "$output" = "><" ]
}

# ============================================================================
# Return Statement and Exit Codes
# ============================================================================

@test "function: explicit return 0" {
    run "$COLONY" -c 'success() { return 0; }; success; echo $?'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "0" ]
}

@test "function: explicit return non-zero" {
    run "$COLONY" -c 'fail() { return 42; }; fail; echo $?'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "42" ]
}

@test "function: return without value defaults to 0" {
    run "$COLONY" -c 'ret() { return; }; ret; echo $?'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "0" ]
}

@test "function: implicit return uses last command exit" {
    run "$COLONY" -c 'implicit() { false; }; implicit; echo $?'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "1" ]
}

@test "function: return exits function immediately" {
    run "$COLONY" -c 'early() { echo before; return; echo after; }; early'
    [ "$status" -eq 0 ]
    [ "$output" = "before" ]
}

# ============================================================================
# Variable Scope
# ============================================================================

@test "function: can access global variables" {
    run "$COLONY" -c 'GLOBAL=value; func() { echo $GLOBAL; }; func'
    [ "$status" -eq 0 ]
    [ "$output" = "value" ]
}

@test "function: can modify global variables" {
    run "$COLONY" -c 'VAR=before; modify() { VAR=after; }; modify; echo $VAR'
    [ "$status" -eq 0 ]
    [ "$output" = "after" ]
}

@test "function: local variables (if supported)" {
    skip "local keyword not yet implemented"
    run "$COLONY" -c 'func() { local x=inner; }; x=outer; func; echo $x'
    [ "$status" -eq 0 ]
    [ "$output" = "outer" ]
}

# ============================================================================
# Nested Functions and Recursion
# ============================================================================

@test "function: define function inside function" {
    run "$COLONY" -c 'outer() { inner() { echo nested; }; inner; }; outer'
    [ "$status" -eq 0 ]
    [ "$output" = "nested" ]
}

@test "function: recursive function" {
    skip "Recursion needs careful testing for stack limits"
    run "$COLONY" -c 'countdown() { [ $1 -le 0 ] && return; echo $1; countdown $(($1 - 1)); }; countdown 3'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "3" ]
    [ "${lines[1]}" = "2" ]
    [ "${lines[2]}" = "1" ]
}

# ============================================================================
# Function Body Variants
# ============================================================================

@test "function: multiline body" {
    run "$COLONY" -c 'multi() {
        echo line1
        echo line2
        echo line3
    }; multi'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "line1" ]
    [ "${lines[1]}" = "line2" ]
    [ "${lines[2]}" = "line3" ]
}

@test "function: empty body" {
    run "$COLONY" -c 'empty() { :; }; empty'
    [ "$status" -eq 0 ]
}

@test "function: body with pipeline" {
    run "$COLONY" -c 'pipeline() { echo hello | cat; }; pipeline'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "function: body with command list" {
    run "$COLONY" -c 'list() { echo one && echo two || echo three; }; list'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "one" ]
    [ "${lines[1]}" = "two" ]
}

@test "function: body with control flow" {
    run "$COLONY" -c 'conditional() { if [ $1 = yes ]; then echo approved; fi; }; conditional yes'
    [ "$status" -eq 0 ]
    [ "$output" = "approved" ]
}

@test "function: body with loop" {
    run "$COLONY" -c 'loop() { for i in a b c; do echo $i; done; }; loop'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "a" ]
    [ "${lines[1]}" = "b" ]
    [ "${lines[2]}" = "c" ]
}

# ============================================================================
# Function Redefinition
# ============================================================================

@test "function: redefine function" {
    run "$COLONY" -c 'fn() { echo first; }; fn; fn() { echo second; }; fn'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "first" ]
    [ "${lines[1]}" = "second" ]
}

@test "function: unset function" {
    skip "unset -f not yet implemented"
    run "$COLONY" -c 'fn() { echo test; }; fn; unset -f fn; fn'
    [ "$status" -ne 0 ]
}

# ============================================================================
# Functions with Redirections
# ============================================================================

@test "function: redirect function output" {
    run "$COLONY" -c "msg() { echo content; }; msg > $TEST_TEMP/out.txt && cat $TEST_TEMP/out.txt"
    [ "$status" -eq 0 ]
    [ "$output" = "content" ]
}

@test "function: redirect inside function" {
    run "$COLONY" -c "write() { echo data > $TEST_TEMP/func.txt; }; write && cat $TEST_TEMP/func.txt"
    [ "$status" -eq 0 ]
    [ "$output" = "data" ]
}

@test "function: redirect function input" {
    run "$COLONY" -c "echo input > $TEST_TEMP/in.txt && reader() { cat; }; reader < $TEST_TEMP/in.txt"
    [ "$status" -eq 0 ]
    [ "$output" = "input" ]
}

# ============================================================================
# Functions in Pipelines
# ============================================================================

@test "function: function in pipeline" {
    run "$COLONY" -c 'upper() { echo HELLO; }; upper | cat'
    [ "$status" -eq 0 ]
    [ "$output" = "HELLO" ]
}

@test "function: pipeline into function" {
    run "$COLONY" -c 'printer() { cat; }; echo data | printer'
    [ "$status" -eq 0 ]
    [ "$output" = "data" ]
}

@test "function: function between pipeline stages" {
    run "$COLONY" -c 'middle() { cat; }; echo start | middle | cat'
    [ "$status" -eq 0 ]
    [ "$output" = "start" ]
}

# ============================================================================
# Edge Cases
# ============================================================================

@test "function: function name with underscore" {
    run "$COLONY" -c 'my_function() { echo ok; }; my_function'
    [ "$status" -eq 0 ]
    [ "$output" = "ok" ]
}

@test "function: function name with numbers" {
    run "$COLONY" -c 'func2() { echo ok; }; func2'
    [ "$status" -eq 0 ]
    [ "$output" = "ok" ]
}

@test "function: cannot start with number" {
    run "$COLONY" -c '2func() { echo bad; }'
    [ "$status" -ne 0 ]
}

@test "function: function shadows builtin" {
    run "$COLONY" -c 'echo() { printf "custom\n"; }; echo test'
    [ "$status" -eq 0 ]
    [ "$output" = "custom" ]
}

@test "function: function shadows external command" {
    run "$COLONY" -c 'ls() { echo fake-ls; }; ls'
    [ "$status" -eq 0 ]
    [ "$output" = "fake-ls" ]
}

# ============================================================================
# Bash Compatibility Comparison
# ============================================================================

@test "compare function definition with bash" {
    run bash -c 'greet() { echo hello; }; greet'
    local bash_output="$output"

    run "$COLONY" -c 'greet() { echo hello; }; greet'
    local colony_output="$output"

    [ "$colony_output" = "$bash_output" ]
}

@test "compare function with args with bash" {
    run bash -c 'add() { echo $(($1 + $2)); }; add 5 3'
    local bash_output="$output"

    run "$COLONY" -c 'add() { echo $(($1 + $2)); }; add 5 3'
    local colony_output="$output"

    [ "$colony_output" = "$bash_output" ]
}

@test "compare function return with bash" {
    run bash -c 'fail() { return 7; }; fail; echo $?'
    local bash_output="$output"

    run "$COLONY" -c 'fail() { return 7; }; fail; echo $?'
    local colony_output="$output"

    [ "$colony_output" = "$bash_output" ]
}

@test "compare \$@ in function with bash" {
    run bash -c 'show() { echo $@; }; show a b c'
    local bash_output="$output"

    run "$COLONY" -c 'show() { echo $@; }; show a b c'
    local colony_output="$output"

    [ "$colony_output" = "$bash_output" ]
}

# ============================================================================
# Special Cases
# ============================================================================

@test "function: function name same as variable" {
    run "$COLONY" -c 'NAME=var; NAME() { echo func; }; NAME; echo $NAME'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "func" ]
    [ "${lines[1]}" = "var" ]
}

@test "function: export function (not standard)" {
    skip "Function export not standard POSIX"
    run "$COLONY" -c 'fn() { echo test; }; export -f fn'
    # Behavior varies between shells
}

@test "function: function with same name as alias" {
    run "$COLONY" -c 'alias greet="echo alias"; greet() { echo function; }; greet'
    [ "$status" -eq 0 ]
    # Functions take precedence over aliases
    [ "$output" = "function" ]
}

@test "function: dollar sign in function body" {
    run "$COLONY" -c 'price() { echo "$1"; }; price 100'
    [ "$status" -eq 0 ]
    [ "$output" = "100" ]
}

@test "function: quotes in function body" {
    run "$COLONY" -c 'quoted() { echo "hello world"; }; quoted'
    [ "$status" -eq 0 ]
    [ "$output" = "hello world" ]
}

# ============================================================================
# Error Cases
# ============================================================================

@test "function: calling undefined function fails" {
    run "$COLONY" -c 'undefined_function'
    [ "$status" -ne 0 ]
}

@test "function: syntax error in function body" {
    run "$COLONY" -c 'bad() { if true; }; bad'
    [ "$status" -ne 0 ]
}

@test "function: missing closing brace" {
    run "$COLONY" -c 'broken() { echo test'
    [ "$status" -ne 0 ]
}

@test "function: empty function name" {
    run "$COLONY" -c '() { echo bad; }'
    [ "$status" -ne 0 ]
}
