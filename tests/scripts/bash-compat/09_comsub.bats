#!/usr/bin/env bats
# Command Substitution tests - based on bash tests/comsub.tests
# Tests both old-style backticks `cmd` and new-style $(...) 
# Covers parsing, nesting, quoting, and edge cases

setup() {
    COLONY="${COLONY_BIN:-../../../target/release/colony}"
}

# ============================================================================
# Basic Command Substitution - Dollar Paren $(...)
# ============================================================================

@test "comsub: dollar paren basic echo" {
    run $COLONY -c 'echo $(echo hello)'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "comsub: dollar paren with multiple words" {
    run $COLONY -c 'echo $(echo hello world)'
    [ "$status" -eq 0 ]
    [ "$output" = "hello world" ]
}

@test "comsub: dollar paren inline with text" {
    run $COLONY -c 'echo ab$(echo mn)yz'
    [ "$status" -eq 0 ]
    [ "$output" = "abmnyz" ]
}

@test "comsub: dollar paren with semicolon" {
    run $COLONY -c 'echo ab$(echo mn; echo op)yz'
    [ "$status" -eq 0 ]
    [ "$output" = "abmn opyz" ]
}

@test "comsub: dollar paren in variable assignment" {
    run $COLONY -c 'a=$(echo hello); echo $a'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "comsub: dollar paren with pipeline" {
    run $COLONY -c 'echo $(echo "a b c" | tr " " "\n" | grep b)'
    [ "$status" -eq 0 ]
    [ "$output" = "b" ]
}

# ============================================================================
# Basic Command Substitution - Backticks `...`
# ============================================================================

@test "comsub: backtick basic echo" {
    run $COLONY -c 'echo `echo hello`'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "comsub: backtick with multiple words" {
    run $COLONY -c 'echo `echo hello world`'
    [ "$status" -eq 0 ]
    [ "$output" = "hello world" ]
}

@test "comsub: backtick inline with text" {
    run $COLONY -c 'echo ab`echo mn`yz'
    [ "$status" -eq 0 ]
    [ "$output" = "abmnyz" ]
}

@test "comsub: backtick with newline and comment" {
    run $COLONY -c 'echo `echo ab cd #efg
echo hij`'
    [ "$status" -eq 0 ]
    [ "$output" = "ab cd hij" ]
}

@test "comsub: backtick in variable assignment" {
    run $COLONY -c 'a=`echo hello`; echo $a'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

# ============================================================================
# Quoting in Command Substitution
# ============================================================================

@test "comsub: dollar paren with single quotes" {
    run $COLONY -c "echo \$(echo 'hello world')"
    [ "$status" -eq 0 ]
    [ "$output" = "hello world" ]
}

@test "comsub: dollar paren with double quotes" {
    run $COLONY -c 'echo $(echo "hello world")'
    [ "$status" -eq 0 ]
    [ "$output" = "hello world" ]
}

@test "comsub: backtick with single quotes" {
    run $COLONY -c "echo \`echo 'hello world'\`"
    [ "$status" -eq 0 ]
    [ "$output" = "hello world" ]
}

@test "comsub: backtick with double quotes" {
    run $COLONY -c 'echo `echo "hello world"`'
    [ "$status" -eq 0 ]
    [ "$output" = "hello world" ]
}

@test "comsub: dollar paren preserves escaped dollar" {
    run $COLONY -c 'echo $(echo "\$foo")'
    [ "$status" -eq 0 ]
    [ "$output" = '$foo' ]
}

@test "comsub: backtick with escaped dollar in single quote" {
    run $COLONY -c "echo \`echo '\$foo'\`"
    [ "$status" -eq 0 ]
    [ "$output" = '$foo' ]
}

@test "comsub: backtick with escaped backtick" {
    run $COLONY -c "echo \`echo '\`'\`"
    [ "$status" -eq 0 ]
    [ "$output" = '`' ]
}

@test "comsub: backtick with escaped backslash" {
    run $COLONY -c "echo \`echo '\\\\'\`"
    [ "$status" -eq 0 ]
    [ "$output" = '\' ]
}

# ============================================================================
# Nested Command Substitution
# ============================================================================

@test "comsub: nested dollar paren two levels" {
    run $COLONY -c 'echo $(echo $(echo nested))'
    [ "$status" -eq 0 ]
    [ "$output" = "nested" ]
}

@test "comsub: nested dollar paren three levels" {
    run $COLONY -c 'echo $(echo $(echo $(echo nested)))'
    [ "$status" -eq 0 ]
    [ "$output" = "nested" ]
}

@test "comsub: nested dollar paren four levels" {
    run $COLONY -c 'echo $(echo $(echo $(echo $(echo nested))))'
    [ "$status" -eq 0 ]
    [ "$output" = "nested" ]
}

@test "comsub: nested dollar paren with newlines" {
    run $COLONY -c 'echo $(
echo $(
echo $(echo $(echo nested))
)
)'
    [ "$status" -eq 0 ]
    [ "$output" = "nested" ]
}

@test "comsub: dollar paren nested in backtick" {
    run $COLONY -c 'echo `echo $(echo nested)`'
    [ "$status" -eq 0 ]
    [ "$output" = "nested" ]
}

@test "comsub: backtick nested in dollar paren" {
    run $COLONY -c 'echo $(echo `echo nested`)'
    [ "$status" -eq 0 ]
    [ "$output" = "nested" ]
}

# ============================================================================
# Empty Command Substitution
# ============================================================================

@test "comsub: empty dollar paren" {
    run $COLONY -c 'echo blank --$()--'
    [ "$status" -eq 0 ]
    [ "$output" = "blank ----" ]
}

@test "comsub: empty dollar paren with newline" {
    run $COLONY -c 'echo blank --$(
)--'
    [ "$status" -eq 0 ]
    [ "$output" = "blank ----" ]
}

@test "comsub: empty backtick" {
    run $COLONY -c 'echo blank --``--'
    [ "$status" -eq 0 ]
    [ "$output" = "blank ----" ]
}

# ============================================================================
# Command Substitution in Parameter Expansion
# ============================================================================

@test "comsub: in parameter expansion default value" {
    run $COLONY -c 'echo ${foo:-$(echo default)}'
    [ "$status" -eq 0 ]
    [ "$output" = "default" ]
}

@test "comsub: in parameter expansion with brace expansion" {
    run $COLONY -c 'echo ${foo:-$(echo a{b,c})}' 
    [ "$status" -eq 0 ]
    # Brace expansion happens in the comsub
    [[ "$output" =~ "ab" ]] || [[ "$output" =~ "ac" ]]
}

@test "comsub: in double quoted parameter expansion" {
    run $COLONY -c 'echo "${foo:-$(echo default value)}"'
    [ "$status" -eq 0 ]
    [ "$output" = "default value" ]
}

# ============================================================================
# Command Substitution with Arithmetic
# ============================================================================

@test "comsub: dollar paren in arithmetic expression" {
    run $COLONY -c 'echo $((10 - $(echo 3)))'
    [ "$status" -eq 0 ]
    [ "$output" = "7" ]
}

@test "comsub: arithmetic in for loop with comsub" {
    run $COLONY -c 'for ((i=0; i<$(echo 3); i++)); do echo $i; done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "0" ]
    [ "${lines[1]}" = "1" ]
    [ "${lines[2]}" = "2" ]
}

# ============================================================================
# Command Substitution with Control Structures
# ============================================================================

@test "comsub: in if condition" {
    run $COLONY -c 'if [ "$(echo yes)" = "yes" ]; then echo matched; fi'
    [ "$status" -eq 0 ]
    [ "$output" = "matched" ]
}

@test "comsub: in while condition" {
    run $COLONY -c 'count=$(echo 3); i=0; while [ $i -lt $count ]; do echo $i; i=$((i+1)); done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "0" ]
    [ "${lines[1]}" = "1" ]
    [ "${lines[2]}" = "2" ]
}

@test "comsub: in case pattern" {
    run $COLONY -c 'case $(echo foo) in foo) echo matched;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "matched" ]
}

# ============================================================================
# Command Substitution with Functions
# ============================================================================

@test "comsub: call function in comsub" {
    run $COLONY -c 'foo() { echo hello; }; echo $(foo)'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "comsub: return in comsub aborts comsub" {
    run $COLONY -c 'func() { local v; v=$(echo comsub; return; echo after); echo "v=$v"; }; func'
    [ "$status" -eq 0 ]
    [ "$output" = "v=comsub" ]
}

# ============================================================================
# Whitespace Handling
# ============================================================================

@test "comsub: trailing newline removed" {
    run $COLONY -c 'a=$(echo hello
); echo "[$a]"'
    [ "$status" -eq 0 ]
    [ "$output" = "[hello]" ]
}

@test "comsub: multiple trailing newlines removed" {
    run $COLONY -c 'a=$(echo hello


); echo "[$a]"'
    [ "$status" -eq 0 ]
    [ "$output" = "[hello]" ]
}

@test "comsub: internal newlines become spaces" {
    run $COLONY -c 'echo $(echo hello
echo world)'
    [ "$status" -eq 0 ]
    [ "$output" = "hello world" ]
}

@test "comsub: preserve internal newlines when quoted" {
    run $COLONY -c 'echo "$(echo hello
echo world)"'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "hello" ]
    [ "${lines[1]}" = "world" ]
}

# ============================================================================
# Special Cases and Edge Cases
# ============================================================================

@test "comsub: with exit status" {
    run $COLONY -c 'a=$(echo success); echo $?'
    [ "$status" -eq 0 ]
    [ "$output" = "0" ]
}

@test "comsub: failed command exit status" {
    run $COLONY -c 'a=$(false); echo $?'
    [ "$status" -eq 0 ]
    [ "$output" = "1" ]
}

@test "comsub: with redirections" {
    run $COLONY -c 'echo $(echo hello >&2 2>&1)'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "comsub: with background job" {
    skip "Background jobs in comsub - complex test"
}

@test "comsub: preserve spaces in quoted context" {
    run $COLONY -c 'echo "$(echo "a  b  c")"'
    [ "$status" -eq 0 ]
    [ "$output" = "a  b  c" ]
}

# ============================================================================
# Backslash and Line Continuation
# ============================================================================

@test "comsub: dollar paren with backslash-newline in quotes" {
    run $COLONY -c 'echo $(echo "foo\
bar")'
    [ "$status" -eq 0 ]
    [ "$output" = "foobar" ]
}

@test "comsub: backtick with backslash-newline in single quotes" {
    run $COLONY -c "echo \`echo 'foo\\
bar'\`"
    [ "$status" -eq 0 ]
    [ "$output" = 'foo\ bar' ]
}

# ============================================================================
# Comparison with Bash
# ============================================================================

@test "compare: basic dollar paren with bash" {
    run $COLONY -c 'echo $(echo hello world)'
    colony_output="$output"
    run bash -c 'echo $(echo hello world)'
    [ "$colony_output" = "$output" ]
}

@test "compare: basic backtick with bash" {
    run $COLONY -c 'echo `echo hello world`'
    colony_output="$output"
    run bash -c 'echo `echo hello world`'
    [ "$colony_output" = "$output" ]
}

@test "compare: nested comsub with bash" {
    run $COLONY -c 'echo $(echo $(echo nested))'
    colony_output="$output"
    run bash -c 'echo $(echo $(echo nested))'
    [ "$colony_output" = "$output" ]
}

@test "compare: comsub with quotes with bash" {
    run $COLONY -c 'echo "$(echo "hello world")"'
    colony_output="$output"
    run bash -c 'echo "$(echo "hello world")"'
    [ "$colony_output" = "$output" ]
}

@test "compare: empty comsub with bash" {
    run $COLONY -c 'echo blank --$()--'
    colony_output="$output"
    run bash -c 'echo blank --$()--'
    [ "$colony_output" = "$output" ]
}
