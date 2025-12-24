#!/usr/bin/env bats
# Process Substitution tests - <(cmd) and >(cmd)
# Tests bash extension for process substitution
# This feature creates named pipes or /dev/fd/N and connects processes

setup() {
    COLONY="${COLONY_BIN:-../../../target/release/colony}"
    TEST_TEMP="$(mktemp -d)"
    export TEST_TEMP
}

teardown() {
    rm -rf "$TEST_TEMP"
}

# ============================================================================
# Basic Input Process Substitution <(cmd)
# ============================================================================

@test "procsub: basic <(echo) with cat" {
    run $COLONY -c 'cat <(echo hello)'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "procsub: <(cmd) returns /dev/fd path" {
    run $COLONY -c 'echo <(echo hello)'
    [ "$status" -eq 0 ]
    # Should print something like /dev/fd/63 or a FIFO path
    [[ "$output" =~ ^/dev/fd/[0-9]+$ ]] || [[ "$output" =~ ^/tmp/ ]] || [[ "$output" =~ ^/var/ ]]
}

@test "procsub: <(cmd) with multi-word output" {
    run $COLONY -c 'cat <(echo hello world)'
    [ "$status" -eq 0 ]
    [ "$output" = "hello world" ]
}

@test "procsub: <(cmd) with multi-line output" {
    run $COLONY -c 'cat <(echo -e "line1\nline2\nline3")'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "line1" ]
    [ "${lines[1]}" = "line2" ]
    [ "${lines[2]}" = "line3" ]
}

@test "procsub: <(cmd) with printf" {
    run $COLONY -c 'cat <(printf "a\nb\nc\n")'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "a" ]
    [ "${lines[1]}" = "b" ]
    [ "${lines[2]}" = "c" ]
}

# ============================================================================
# Basic Output Process Substitution >(cmd)
# ============================================================================

@test "procsub: basic >(cat) with echo" {
    run $COLONY -c 'echo hello > >(cat)'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "procsub: >(cmd) with grep filter" {
    run $COLONY -c 'echo -e "error: something\ninfo: other" > >(grep error)'
    [ "$status" -eq 0 ]
    [ "$output" = "error: something" ]
}

@test "procsub: >(cmd) writes to file" {
    run $COLONY -c "echo hello > >(cat > $TEST_TEMP/output.txt); sleep 0.1; cat $TEST_TEMP/output.txt"
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

# ============================================================================
# Multiple Process Substitutions
# ============================================================================

@test "procsub: two <(cmd) with diff" {
    run $COLONY -c 'diff <(echo "line1") <(echo "line1")'
    [ "$status" -eq 0 ]
    [ "$output" = "" ]
}

@test "procsub: diff with different <(cmd)" {
    run $COLONY -c 'diff <(echo "a") <(echo "b")'
    [ "$status" -eq 1 ]
    [[ "$output" =~ "1c1" ]]
}

@test "procsub: three <(cmd) arguments" {
    run $COLONY -c 'cat <(echo 1) <(echo 2) <(echo 3)'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "1" ]
    [ "${lines[1]}" = "2" ]
    [ "${lines[2]}" = "3" ]
}

@test "procsub: mixed <(cmd) and regular args" {
    run $COLONY -c "echo 'regular' > $TEST_TEMP/file.txt; cat $TEST_TEMP/file.txt <(echo 'procsub')"
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "regular" ]
    [ "${lines[1]}" = "procsub" ]
}

# ============================================================================
# Process Substitution with Pipelines
# ============================================================================

@test "procsub: <(pipeline) with multiple commands" {
    run $COLONY -c 'cat <(echo "hello world" | tr "o" "0")'
    [ "$status" -eq 0 ]
    [ "$output" = "hell0 w0rld" ]
}

@test "procsub: <(cmd) piped to another command" {
    run $COLONY -c 'cat <(echo -e "b\na\nc") | sort'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "a" ]
    [ "${lines[1]}" = "b" ]
    [ "${lines[2]}" = "c" ]
}

@test "procsub: complex pipeline inside <()" {
    run $COLONY -c 'cat <(echo "a b c" | tr " " "\n" | sort -r)'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "c" ]
    [ "${lines[1]}" = "b" ]
    [ "${lines[2]}" = "a" ]
}

# ============================================================================
# Process Substitution with Variables
# ============================================================================

@test "procsub: variable expansion inside <()" {
    run $COLONY -c 'VAR=hello; cat <(echo $VAR)'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "procsub: quoted variable inside <()" {
    run $COLONY -c 'VAR="hello world"; cat <(echo "$VAR")'
    [ "$status" -eq 0 ]
    [ "$output" = "hello world" ]
}

@test "procsub: assign procsub path to variable" {
    run $COLONY -c 'FD=<(echo hello); cat "$FD"'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

# ============================================================================
# Nested Process Substitution
# ============================================================================

@test "procsub: nested <(cat <(echo))" {
    run $COLONY -c 'cat <(cat <(echo nested))'
    [ "$status" -eq 0 ]
    [ "$output" = "nested" ]
}

@test "procsub: double nested" {
    run $COLONY -c 'cat <(cat <(cat <(echo deep)))'
    [ "$status" -eq 0 ]
    [ "$output" = "deep" ]
}

# ============================================================================
# Process Substitution with Command Substitution
# ============================================================================

@test "procsub: <() inside command substitution" {
    run $COLONY -c 'echo $(cat <(echo hello))'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "procsub: command substitution inside <()" {
    run $COLONY -c 'cat <(echo $(echo hello))'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

# ============================================================================
# Process Substitution with Control Flow
# ============================================================================

@test "procsub: <() in while loop" {
    run $COLONY -c 'while read line; do echo "read: $line"; done < <(echo -e "a\nb")'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "read: a" ]
    [ "${lines[1]}" = "read: b" ]
}

@test "procsub: <() in for loop" {
    run $COLONY -c 'for f in <(echo a) <(echo b); do cat "$f"; done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "a" ]
    [ "${lines[1]}" = "b" ]
}

@test "procsub: <() in if condition (file test)" {
    run $COLONY -c 'if [ -r <(echo hello) ]; then echo readable; else echo not; fi'
    [ "$status" -eq 0 ]
    [ "$output" = "readable" ]
}

# ============================================================================
# Process Substitution with Redirections
# ============================================================================

@test "procsub: redirect stdin from <()" {
    run $COLONY -c 'cat < <(echo hello)'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "procsub: tee with >()" {
    run $COLONY -c "echo hello | tee >(cat > $TEST_TEMP/a.txt) >(cat > $TEST_TEMP/b.txt) > /dev/null; sleep 0.1; cat $TEST_TEMP/a.txt $TEST_TEMP/b.txt"
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "hello" ]
    [ "${lines[1]}" = "hello" ]
}

# ============================================================================
# Process Substitution with Quoting
# ============================================================================

@test "procsub: <() not expanded inside single quotes" {
    run $COLONY -c "echo '<(echo hello)'"
    [ "$status" -eq 0 ]
    [ "$output" = "<(echo hello)" ]
}

@test "procsub: <() expanded inside double quotes" {
    # NOTE: Process substitution is NOT expanded inside double quotes in bash either
    # Both bash and Colony preserve the literal <(...) text inside double quotes
    # This test verifies the correct (literal) behavior
    run $COLONY -c 'echo "<(echo hello)"'
    [ "$status" -eq 0 ]
    [ "$output" = "<(echo hello)" ]
}

@test "procsub: space in <() command" {
    run $COLONY -c 'cat <(echo "hello   world")'
    [ "$status" -eq 0 ]
    [ "$output" = "hello   world" ]
}

# ============================================================================
# Error Handling
# ============================================================================

@test "procsub: command failure inside <() still works" {
    run $COLONY -c 'cat <(false; echo after)'
    [ "$status" -eq 0 ]
    [ "$output" = "after" ]
}

@test "procsub: non-existent command inside <()" {
    run $COLONY -c 'cat <(nonexistent_command_12345 2>/dev/null; echo fallback)'
    [ "$status" -eq 0 ]
    [ "$output" = "fallback" ]
}

# ============================================================================
# Comparison with Bash
# ============================================================================

@test "compare: basic <(echo) with bash" {
    run $COLONY -c 'cat <(echo hello)'
    colony_output="$output"
    run bash -c 'cat <(echo hello)'
    [ "$colony_output" = "$output" ]
}

@test "compare: diff <() <() with bash" {
    run $COLONY -c 'diff <(echo a) <(echo a)'
    colony_status="$status"
    colony_output="$output"
    run bash -c 'diff <(echo a) <(echo a)'
    [ "$colony_status" -eq "$status" ]
    [ "$colony_output" = "$output" ]
}

@test "compare: <() path format with bash" {
    # Test that process substitution returns a valid /dev/fd/N path
    run $COLONY -c 'f=<(echo x); echo "$f"'
    [ "$status" -eq 0 ]
    [[ "$output" =~ ^/dev/fd/[0-9]+$ ]]
}

@test "compare: nested <() with bash" {
    run $COLONY -c 'cat <(cat <(echo nested))'
    colony_output="$output"
    run bash -c 'cat <(cat <(echo nested))'
    [ "$colony_output" = "$output" ]
}

@test "compare: while read < <() with bash" {
    run $COLONY -c 'while read line; do echo "[$line]"; done < <(echo -e "a\nb")'
    colony_output="$output"
    run bash -c 'while read line; do echo "[$line]"; done < <(echo -e "a\nb")'
    [ "$colony_output" = "$output" ]
}

# ============================================================================
# Edge Cases
# ============================================================================

@test "procsub: empty command in <()" {
    run $COLONY -c 'cat <(:)'
    [ "$status" -eq 0 ]
    [ "$output" = "" ]
}

@test "procsub: large output in <()" {
    run $COLONY -c 'cat <(seq 1 100) | wc -l'
    [ "$status" -eq 0 ]
    [[ "$output" =~ "100" ]]
}

@test "procsub: binary-safe data through <()" {
    run $COLONY -c 'cat <(printf "a\x00b") | od -c | head -1'
    [ "$status" -eq 0 ]
    [[ "$output" =~ "a" ]]
}

@test "procsub: special characters in <() output" {
    run $COLONY -c 'cat <(echo '\''$PATH'\'')'
    [ "$status" -eq 0 ]
    [ "$output" = '$PATH' ]
}

@test "procsub: newlines preserved in <()" {
    # Test that consecutive newlines are preserved through process substitution
    run $COLONY -c 'cat <(printf "a\n\n\nb")'
    [ "$status" -eq 0 ]
    # Compare full output rather than lines array (BATS doesn't handle empty lines well)
    expected=$'a\n\n\nb'
    [ "$output" = "$expected" ]
}

# ============================================================================
# Process Substitution as Command Arguments
# ============================================================================

@test "procsub: grep with <() as pattern file" {
    run $COLONY -c 'grep -f <(echo hello) <(echo -e "hello\nworld")'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "procsub: paste with two <()" {
    run $COLONY -c 'paste <(echo -e "1\n2\n3") <(echo -e "a\nb\nc")'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "1	a" ]
    [ "${lines[1]}" = "2	b" ]
    [ "${lines[2]}" = "3	c" ]
}

@test "procsub: sort with <() containing unsorted data" {
    run $COLONY -c 'sort <(echo -e "c\na\nb")'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "a" ]
    [ "${lines[1]}" = "b" ]
    [ "${lines[2]}" = "c" ]
}

@test "procsub: comm with two sorted <()" {
    run $COLONY -c 'comm <(echo -e "a\nb\nc") <(echo -e "b\nc\nd")'
    [ "$status" -eq 0 ]
    # comm shows differences between sorted files
}

@test "procsub: wc with <()" {
    run $COLONY -c 'wc -l <(echo -e "1\n2\n3")'
    [ "$status" -eq 0 ]
    [[ "$output" =~ "3" ]]
}
