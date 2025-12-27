#!/usr/bin/env bats
# Test suite for read builtin
# Based on bash read.tests from github.com/bminor/bash

# Get the directory containing this test file
SCRIPT_DIR="$(cd "$(dirname "$BATS_TEST_FILENAME")" && pwd)"
COLONY="${COLONY_BIN:-$SCRIPT_DIR/../../../target/debug/colony}"

# Helper to run colony and capture output
run_colony() {
    run "$COLONY" -c "$1"
}

# =============================================================================
# Basic read functionality
# =============================================================================

@test "read: basic read into single variable" {
    run_colony 'echo "hello" | { read x; echo "$x"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "read: read with leading/trailing whitespace trimmed" {
    run_colony 'echo " a " | { read x; echo "[$x]"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "[a]" ]
}

@test "read: read into multiple variables" {
    run_colony 'echo "one two three" | { read a b c; echo "a=$a b=$b c=$c"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "a=one b=two c=three" ]
}

@test "read: extra words go into last variable" {
    run_colony 'echo "one two three four" | { read a b; echo "a=$a b=$b"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "a=one b=two three four" ]
}

@test "read: fewer words than variables - extras set to empty" {
    run_colony 'echo "one" | { read a b c; echo "a=[$a] b=[$b] c=[$c]"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "a=[one] b=[] c=[]" ]
}

@test "read: no variable name uses REPLY" {
    run_colony 'echo "hello world" | { read; echo "$REPLY"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "hello world" ]
}

@test "read: REPLY preserves leading whitespace" {
    run_colony 'echo " A B " | { read; echo "[$REPLY]"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "[ A B ]" ]
}

# =============================================================================
# Whitespace handling
# =============================================================================

@test "read: leading whitespace stripped from first variable" {
    run_colony 'echo "  a  b  " | { read x y; echo "x=[$x] y=[$y]"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "x=[a] y=[b]" ]
}

@test "read: internal whitespace preserved in last variable" {
    run_colony 'echo "a  b  c" | { read x; echo "[$x]"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "[a  b  c]" ]
}

# =============================================================================
# Backslash handling (non-raw mode)
# =============================================================================

@test "read: backslash-space preserved in value" {
    run_colony 'echo "a b\\ c" | { read x; echo "[$x]"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "[a b c]" ]
}

@test "read: trailing backslash continues to next line" {
    run_colony 'printf "abc\\\\\ndef" | { read x; echo "[$x]"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "[abcdef]" ]
}

@test "read: backslash-newline continuation" {
    run_colony 'printf "ab\\\\\ncd" | { read; echo "[$REPLY]"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "[abcd]" ]
}

# =============================================================================
# Raw mode (-r)
# =============================================================================

@test "read -r: backslash not treated specially" {
    run_colony 'echo "a\\ b" | { read -r x; echo "[$x]"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "[a\\ b]" ]
}

@test "read -r: trailing backslash preserved" {
    run_colony 'echo "abc\\" | { read -r x; echo "[$x]"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "[abc\\]" ]
}

@test "read -r: multiple variables with backslash" {
    run_colony 'echo " a  b\\ " | { read -r x y; echo "x=[$x] y=[$y]"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "x=[a] y=[b\\]" ]
}

@test "read -r: leading backslash preserved" {
    run_colony 'echo "\\ a  b\\ " | { read -r x y; echo "x=[$x] y=[$y]"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "x=[\\] y=[a  b\\]" ]
}

# =============================================================================
# IFS behavior
# =============================================================================

@test "read: custom IFS colon delimiter" {
    run_colony 'echo "a:b:c" | { IFS=:; read x y z; echo "x=$x y=$y z=$z"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "x=a y=b z=c" ]
}

@test "read: IFS empty reads entire line" {
    run_colony 'echo " foo bar " | { IFS=; read line; echo "[$line]"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "[ foo bar ]" ]
}

@test "read: IFS unset uses default whitespace" {
    run_colony 'echo " foo bar " | { unset IFS; read x y; echo "x=[$x] y=[$y]"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "x=[foo] y=[bar]" ]
}

@test "read: IFS newline only" {
    run_colony 'printf "a b c\nd e f" | { IFS=$'"'"'\n'"'"'; read line; echo "[$line]"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "[a b c]" ]
}

@test "read: IFS with non-whitespace delimiter" {
    run_colony 'echo "a:b:c" | { IFS=:; read x; echo "[$x]"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "[a:b:c]" ]
}

@test "read: multiple consecutive IFS delimiters create empty fields" {
    run_colony 'echo "a::c" | { IFS=:; read x y z; echo "x=[$x] y=[$y] z=[$z]"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "x=[a] y=[] z=[c]" ]
}

@test "read: leading IFS delimiter creates empty first field" {
    run_colony 'echo ":a:b" | { IFS=:; read x y z; echo "x=[$x] y=[$y] z=[$z]"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "x=[] y=[a] z=[b]" ]
}

# =============================================================================
# File input (redirection)
# =============================================================================

@test "read: from file via redirection" {
    run_colony 'echo "A B C" > /tmp/colony_read_test && read x y z < /tmp/colony_read_test && echo "x=$x y=$y z=$z" && rm /tmp/colony_read_test'
    [ "$status" -eq 0 ]
    [ "$output" = "x=A y=B z=C" ]
}

@test "read: multiple reads from same file" {
    run_colony 'printf "line1\nline2\n" > /tmp/colony_read_test && { read a; read b; } < /tmp/colony_read_test && echo "a=$a b=$b" && rm /tmp/colony_read_test'
    [ "$status" -eq 0 ]
    [ "$output" = "a=line1 b=line2" ]
}

# =============================================================================
# EOF handling
# =============================================================================

@test "read: returns 1 on EOF" {
    run_colony 'echo "" | { read x; echo "status=$?"; }'
    [ "$status" -eq 0 ]
    [[ "$output" == *"status=0"* ]] || [[ "$output" == *"status=1"* ]]
}

@test "read: empty input sets variable to empty" {
    run_colony 'echo "" | { read x; echo "x=[$x]"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "x=[]" ]
}

@test "read: no input returns failure" {
    run_colony 'printf "" | { read x; echo "status=$?"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "status=1" ]
}

# =============================================================================
# -p prompt option
# =============================================================================

@test "read -p: prompt option (output goes to stderr)" {
    skip "read -p not yet implemented"
    run_colony 'echo "answer" | { read -p "Question: " x; echo "got: $x"; }'
    [ "$status" -eq 0 ]
    [[ "$output" == *"got: answer"* ]]
}

# =============================================================================
# -s silent option
# =============================================================================

@test "read -s: silent mode (no echo)" {
    skip "read -s not yet implemented"
    run_colony 'echo "secret" | { read -s x; echo "got: $x"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "got: secret" ]
}

# =============================================================================
# -n nchars option
# =============================================================================

@test "read -n: read specific number of characters" {
    skip "read -n not yet implemented"
    run_colony 'echo "abcdefg" | { read -n 3 x; echo "[$x]"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "[abc]" ]
}

@test "read -n: fewer chars available than requested" {
    skip "read -n not yet implemented"
    run_colony 'echo "ab" | { read -n 5 x; echo "[$x]"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "[ab]" ]
}

@test "read -n 1: read single character" {
    skip "read -n not yet implemented"
    run_colony 'echo "xyz" | { read -n 1 x; echo "[$x]"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "[x]" ]
}

# =============================================================================
# -N nchars option (exact count, ignores delimiter)
# =============================================================================

@test "read -N: read exactly N characters" {
    skip "read -N not yet implemented"
    run_colony 'printf "abc\ndef" | { read -N 5 x; echo "[$x]"; }'
    [ "$status" -eq 0 ]
    [[ "$output" == *"abc"* ]]
}

# =============================================================================
# -d delimiter option
# =============================================================================

@test "read -d: custom delimiter" {
    skip "read -d not yet implemented"
    run_colony 'echo "abc|def" | { read -d "|" x; echo "[$x]"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "[abc]" ]
}

@test "read -d: space as delimiter" {
    skip "read -d not yet implemented"
    run_colony 'echo "hello world" | { read -d " " x; echo "[$x]"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "[hello]" ]
}

@test "read -d: null delimiter for NUL-separated input" {
    skip "read -d not yet implemented"
    run_colony 'printf "a\0b\0c" | { read -d "" x; echo "[$x]"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "[a]" ]
}

# =============================================================================
# -t timeout option
# =============================================================================

@test "read -t: timeout with available input succeeds" {
    skip "read -t not yet implemented"
    run_colony 'echo "quick" | { read -t 1 x; echo "[$x]"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "[quick]" ]
}

@test "read -t 0: poll for available input" {
    skip "read -t not yet implemented"
    run_colony 'echo "data" | { read -t 0 && echo "data available"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "data available" ]
}

# =============================================================================
# -u fd option
# =============================================================================

@test "read -u: read from file descriptor" {
    skip "read -u not yet implemented"
    run_colony 'echo "from fd 3" | { exec 3<&0; read -u 3 x; echo "[$x]"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "[from fd 3]" ]
}

# =============================================================================
# -a array option
# =============================================================================

@test "read -a: read into array" {
    skip "read -a not yet implemented"
    run_colony 'echo "one two three" | { read -a arr; echo "${arr[0]} ${arr[1]} ${arr[2]}"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "one two three" ]
}

@test "read -a: array with custom IFS" {
    skip "read -a not yet implemented"
    run_colony 'echo "a:b:c" | { IFS=:; read -a arr; echo "${#arr[@]} elements"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "3 elements" ]
}

# =============================================================================
# -e readline option
# =============================================================================

@test "read -e: readline mode (basic test)" {
    skip "read -e not yet implemented"
    run_colony 'echo "input" | { read -e x; echo "[$x]"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "[input]" ]
}

# =============================================================================
# While read loop patterns
# =============================================================================

@test "read: while loop pattern" {
    run_colony 'printf "a\nb\nc\n" | { while read x; do echo "got: $x"; done; }'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "got: a" ]
    [ "${lines[1]}" = "got: b" ]
    [ "${lines[2]}" = "got: c" ]
}

@test "read: while loop with multiple variables" {
    run_colony 'printf "1 a\n2 b\n3 c\n" | { while read n v; do echo "$n=$v"; done; }'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "1=a" ]
    [ "${lines[1]}" = "2=b" ]
    [ "${lines[2]}" = "3=c" ]
}

@test "read: process /etc/passwd style colon-separated" {
    run_colony 'echo "root:x:0:0:root:/root:/bin/bash" | { IFS=:; read user x uid gid desc home shell; echo "user=$user uid=$uid home=$home"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "user=root uid=0 home=/root" ]
}

# =============================================================================
# Edge cases
# =============================================================================

@test "read: empty variable name list with REPLY" {
    run_colony 'echo "test value" | { read; echo "REPLY=[$REPLY]"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "REPLY=[test value]" ]
}

@test "read: variable already has value - gets overwritten" {
    run_colony 'x=old; echo "new" | { read x; echo "[$x]"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "[new]" ]
}

@test "read: handle carriage return in input" {
    run_colony 'printf "hello\r\n" | { read x; echo "[$x]"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "[hello]" ]
}

@test "read: very long line" {
    run_colony 'python3 -c "print(\"a\" * 10000)" | { read x; echo "len=${#x}"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "len=10000" ]
}

@test "read: line with only whitespace" {
    run_colony 'echo "   " | { read x; echo "x=[$x]"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "x=[]" ]
}

@test "read: tabs as whitespace" {
    run_colony 'printf "a\tb\tc" | { read x y z; echo "x=$x y=$y z=$z"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "x=a y=b z=c" ]
}

# =============================================================================
# Here-string input
# =============================================================================

@test "read: from here-string" {
    skip "here-strings (<<<) not yet fully implemented"
    run_colony 'read x <<< "hello"; echo "[$x]"'
    [ "$status" -eq 0 ]
    [ "$output" = "[hello]" ]
}

@test "read: here-string with variable expansion" {
    skip "here-strings (<<<) not yet fully implemented"
    run_colony 'msg="world"; read x <<< "hello $msg"; echo "[$x]"'
    [ "$status" -eq 0 ]
    [ "$output" = "[hello world]" ]
}

# =============================================================================
# Combined options
# =============================================================================

@test "read -r with custom IFS" {
    run_colony 'echo "a\\:b:c" | { IFS=:; read -r x y z; echo "x=[$x] y=[$y] z=[$z]"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "x=[a\\] y=[b] z=[c]" ]
}

# =============================================================================
# Error handling
# =============================================================================

@test "read: invalid option returns error" {
    skip "read option validation not yet implemented"
    run_colony 'read --invalid-option x'
    [ "$status" -ne 0 ]
}

# =============================================================================
# Subshell behavior
# =============================================================================

@test "read: in subshell doesn't affect parent" {
    run_colony 'x=original; echo "new" | { read x; }; echo "x=$x"'
    [ "$status" -eq 0 ]
    [ "$output" = "x=original" ]
}

@test "read: using process substitution preserves variable" {
    skip "process substitution not yet implemented"
    run_colony 'read x < <(echo "hello"); echo "x=$x"'
    [ "$status" -eq 0 ]
    [ "$output" = "x=hello" ]
}
