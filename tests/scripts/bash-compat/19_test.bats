#!/usr/bin/env bats
# Test Colony shell test/[ builtin for POSIX compliance
# Compare behavior against bash

COLONY="${COLONY_BIN}"

setup() {
    # Create temp directory for test files
    TEST_TEMP="$(mktemp -d)"
    export TEST_TEMP
    
    # Create test files
    echo "content" > "$TEST_TEMP/regular_file"
    mkdir "$TEST_TEMP/directory"
    touch "$TEST_TEMP/empty_file"
    echo "data" > "$TEST_TEMP/readable_file"
    chmod 644 "$TEST_TEMP/readable_file"
    echo "#!/bin/sh" > "$TEST_TEMP/executable_file"
    chmod +x "$TEST_TEMP/executable_file"
    ln -s "$TEST_TEMP/regular_file" "$TEST_TEMP/symlink"
    ln -s "$TEST_TEMP/nonexistent" "$TEST_TEMP/broken_symlink"
}

teardown() {
    # Clean up temp directory
    rm -rf "$TEST_TEMP"
}

# ============================================================================
# File Tests: -e (exists)
# ============================================================================

@test "test -e: regular file exists" {
    run "$COLONY" -c "test -e '$TEST_TEMP/regular_file'"
    [ "$status" -eq 0 ]
}

@test "test -e: directory exists" {
    run "$COLONY" -c "test -e '$TEST_TEMP/directory'"
    [ "$status" -eq 0 ]
}

@test "test -e: nonexistent file" {
    run "$COLONY" -c "test -e '$TEST_TEMP/nonexistent'"
    [ "$status" -ne 0 ]
}

@test "[ -e ]: file exists with bracket syntax" {
    run "$COLONY" -c "[ -e '$TEST_TEMP/regular_file' ]"
    [ "$status" -eq 0 ]
}

@test "[ -e ]: nonexistent file with bracket syntax" {
    run "$COLONY" -c "[ -e '$TEST_TEMP/nonexistent' ]"
    [ "$status" -ne 0 ]
}

# ============================================================================
# File Tests: -f (regular file)
# ============================================================================

@test "test -f: regular file" {
    run "$COLONY" -c "test -f '$TEST_TEMP/regular_file'"
    [ "$status" -eq 0 ]
}

@test "test -f: directory is not regular file" {
    run "$COLONY" -c "test -f '$TEST_TEMP/directory'"
    [ "$status" -ne 0 ]
}

@test "test -f: symlink to regular file" {
    run "$COLONY" -c "test -f '$TEST_TEMP/symlink'"
    [ "$status" -eq 0 ]
}

@test "[ -f ]: regular file with bracket syntax" {
    run "$COLONY" -c "[ -f '$TEST_TEMP/regular_file' ]"
    [ "$status" -eq 0 ]
}

# ============================================================================
# File Tests: -d (directory)
# ============================================================================

@test "test -d: directory" {
    run "$COLONY" -c "test -d '$TEST_TEMP/directory'"
    [ "$status" -eq 0 ]
}

@test "test -d: regular file is not directory" {
    run "$COLONY" -c "test -d '$TEST_TEMP/regular_file'"
    [ "$status" -ne 0 ]
}

@test "[ -d ]: directory with bracket syntax" {
    run "$COLONY" -c "[ -d '$TEST_TEMP/directory' ]"
    [ "$status" -eq 0 ]
}

# ============================================================================
# File Tests: -r (readable)
# ============================================================================

@test "test -r: readable file" {
    run "$COLONY" -c "test -r '$TEST_TEMP/readable_file'"
    [ "$status" -eq 0 ]
}

@test "[ -r ]: readable file with bracket syntax" {
    run "$COLONY" -c "[ -r '$TEST_TEMP/readable_file' ]"
    [ "$status" -eq 0 ]
}

# ============================================================================
# File Tests: -w (writable)
# ============================================================================

@test "test -w: writable file" {
    run "$COLONY" -c "test -w '$TEST_TEMP/regular_file'"
    [ "$status" -eq 0 ]
}

@test "[ -w ]: writable file with bracket syntax" {
    run "$COLONY" -c "[ -w '$TEST_TEMP/regular_file' ]"
    [ "$status" -eq 0 ]
}

# ============================================================================
# File Tests: -x (executable)
# ============================================================================

@test "test -x: executable file" {
    run "$COLONY" -c "test -x '$TEST_TEMP/executable_file'"
    [ "$status" -eq 0 ]
}

@test "test -x: non-executable file" {
    run "$COLONY" -c "test -x '$TEST_TEMP/regular_file'"
    [ "$status" -ne 0 ]
}

@test "[ -x ]: executable file with bracket syntax" {
    run "$COLONY" -c "[ -x '$TEST_TEMP/executable_file' ]"
    [ "$status" -eq 0 ]
}

# ============================================================================
# File Tests: -s (size > 0)
# ============================================================================

@test "test -s: non-empty file" {
    run "$COLONY" -c "test -s '$TEST_TEMP/regular_file'"
    [ "$status" -eq 0 ]
}

@test "test -s: empty file" {
    run "$COLONY" -c "test -s '$TEST_TEMP/empty_file'"
    [ "$status" -ne 0 ]
}

@test "[ -s ]: non-empty file with bracket syntax" {
    run "$COLONY" -c "[ -s '$TEST_TEMP/regular_file' ]"
    [ "$status" -eq 0 ]
}

# ============================================================================
# File Tests: -L / -h (symlink)
# ============================================================================

@test "test -L: symlink" {
    run "$COLONY" -c "test -L '$TEST_TEMP/symlink'"
    [ "$status" -eq 0 ]
}

@test "test -L: regular file is not symlink" {
    run "$COLONY" -c "test -L '$TEST_TEMP/regular_file'"
    [ "$status" -ne 0 ]
}

@test "test -h: symlink (alternate form)" {
    run "$COLONY" -c "test -h '$TEST_TEMP/symlink'"
    [ "$status" -eq 0 ]
}

@test "test -L: broken symlink still detected" {
    run "$COLONY" -c "test -L '$TEST_TEMP/broken_symlink'"
    [ "$status" -eq 0 ]
}

@test "[ -L ]: symlink with bracket syntax" {
    run "$COLONY" -c "[ -L '$TEST_TEMP/symlink' ]"
    [ "$status" -eq 0 ]
}

# ============================================================================
# String Tests: -z (zero length)
# ============================================================================

@test "test -z: empty string" {
    run "$COLONY" -c 'test -z ""'
    [ "$status" -eq 0 ]
}

@test "test -z: non-empty string" {
    run "$COLONY" -c 'test -z "hello"'
    [ "$status" -ne 0 ]
}

@test "test -z: unset variable" {
    run "$COLONY" -c 'test -z "$UNSET_VAR"'
    [ "$status" -eq 0 ]
}

@test "[ -z ]: empty string with bracket syntax" {
    run "$COLONY" -c '[ -z "" ]'
    [ "$status" -eq 0 ]
}

# ============================================================================
# String Tests: -n (non-zero length)
# ============================================================================

@test "test -n: non-empty string" {
    run "$COLONY" -c 'test -n "hello"'
    [ "$status" -eq 0 ]
}

@test "test -n: empty string" {
    run "$COLONY" -c 'test -n ""'
    [ "$status" -ne 0 ]
}

@test "[ -n ]: non-empty string with bracket syntax" {
    run "$COLONY" -c '[ -n "hello" ]'
    [ "$status" -eq 0 ]
}

# ============================================================================
# String Tests: STRING (non-empty implicit)
# ============================================================================

@test "test: non-empty string is true" {
    run "$COLONY" -c 'test "hello"'
    [ "$status" -eq 0 ]
}

@test "test: empty string is false" {
    run "$COLONY" -c 'test ""'
    [ "$status" -ne 0 ]
}

@test "[ STRING ]: non-empty with bracket syntax" {
    run "$COLONY" -c '[ "hello" ]'
    [ "$status" -eq 0 ]
}

# ============================================================================
# String Tests: = (equal)
# ============================================================================

@test "test =: equal strings" {
    run "$COLONY" -c 'test "hello" = "hello"'
    [ "$status" -eq 0 ]
}

@test "test =: unequal strings" {
    run "$COLONY" -c 'test "hello" = "world"'
    [ "$status" -ne 0 ]
}

@test "test =: empty strings are equal" {
    run "$COLONY" -c 'test "" = ""'
    [ "$status" -eq 0 ]
}

@test "[ = ]: equal strings with bracket syntax" {
    run "$COLONY" -c '[ "hello" = "hello" ]'
    [ "$status" -eq 0 ]
}

# ============================================================================
# String Tests: != (not equal)
# ============================================================================

@test "test !=: unequal strings" {
    run "$COLONY" -c 'test "hello" != "world"'
    [ "$status" -eq 0 ]
}

@test "test !=: equal strings" {
    run "$COLONY" -c 'test "hello" != "hello"'
    [ "$status" -ne 0 ]
}

@test "[ != ]: unequal strings with bracket syntax" {
    run "$COLONY" -c '[ "hello" != "world" ]'
    [ "$status" -eq 0 ]
}

# ============================================================================
# String Tests: < (less than, lexicographic)
# ============================================================================

@test "test <: lexicographically less" {
    run "$COLONY" -c 'test "apple" \< "banana"'
    [ "$status" -eq 0 ]
}

@test "test <: lexicographically greater" {
    run "$COLONY" -c 'test "zebra" \< "apple"'
    [ "$status" -ne 0 ]
}

@test "test <: equal strings not less" {
    run "$COLONY" -c 'test "apple" \< "apple"'
    [ "$status" -ne 0 ]
}

@test "[ < ]: lexicographic comparison with bracket syntax" {
    run "$COLONY" -c '[ "apple" \< "banana" ]'
    [ "$status" -eq 0 ]
}

# ============================================================================
# String Tests: > (greater than, lexicographic)
# ============================================================================

@test "test >: lexicographically greater" {
    run "$COLONY" -c 'test "zebra" \> "apple"'
    [ "$status" -eq 0 ]
}

@test "test >: lexicographically less" {
    run "$COLONY" -c 'test "apple" \> "zebra"'
    [ "$status" -ne 0 ]
}

@test "[ > ]: lexicographic comparison with bracket syntax" {
    run "$COLONY" -c '[ "zebra" \> "apple" ]'
    [ "$status" -eq 0 ]
}

# ============================================================================
# Integer Tests: -eq (equal)
# ============================================================================

@test "test -eq: equal integers" {
    run "$COLONY" -c 'test 42 -eq 42'
    [ "$status" -eq 0 ]
}

@test "test -eq: unequal integers" {
    run "$COLONY" -c 'test 42 -eq 43'
    [ "$status" -ne 0 ]
}

@test "test -eq: negative integers" {
    run "$COLONY" -c 'test -5 -eq -5'
    [ "$status" -eq 0 ]
}

@test "[ -eq ]: equal integers with bracket syntax" {
    run "$COLONY" -c '[ 42 -eq 42 ]'
    [ "$status" -eq 0 ]
}

# ============================================================================
# Integer Tests: -ne (not equal)
# ============================================================================

@test "test -ne: unequal integers" {
    run "$COLONY" -c 'test 42 -ne 43'
    [ "$status" -eq 0 ]
}

@test "test -ne: equal integers" {
    run "$COLONY" -c 'test 42 -ne 42'
    [ "$status" -ne 0 ]
}

@test "[ -ne ]: unequal integers with bracket syntax" {
    run "$COLONY" -c '[ 42 -ne 43 ]'
    [ "$status" -eq 0 ]
}

# ============================================================================
# Integer Tests: -lt (less than)
# ============================================================================

@test "test -lt: less than" {
    run "$COLONY" -c 'test 5 -lt 10'
    [ "$status" -eq 0 ]
}

@test "test -lt: not less than" {
    run "$COLONY" -c 'test 10 -lt 5'
    [ "$status" -ne 0 ]
}

@test "test -lt: equal not less" {
    run "$COLONY" -c 'test 5 -lt 5'
    [ "$status" -ne 0 ]
}

@test "test -lt: negative numbers" {
    run "$COLONY" -c 'test -10 -lt -5'
    [ "$status" -eq 0 ]
}

@test "[ -lt ]: less than with bracket syntax" {
    run "$COLONY" -c '[ 5 -lt 10 ]'
    [ "$status" -eq 0 ]
}

# ============================================================================
# Integer Tests: -le (less or equal)
# ============================================================================

@test "test -le: less than" {
    run "$COLONY" -c 'test 5 -le 10'
    [ "$status" -eq 0 ]
}

@test "test -le: equal" {
    run "$COLONY" -c 'test 5 -le 5'
    [ "$status" -eq 0 ]
}

@test "test -le: greater than" {
    run "$COLONY" -c 'test 10 -le 5'
    [ "$status" -ne 0 ]
}

@test "[ -le ]: less or equal with bracket syntax" {
    run "$COLONY" -c '[ 5 -le 5 ]'
    [ "$status" -eq 0 ]
}

# ============================================================================
# Integer Tests: -gt (greater than)
# ============================================================================

@test "test -gt: greater than" {
    run "$COLONY" -c 'test 10 -gt 5'
    [ "$status" -eq 0 ]
}

@test "test -gt: not greater than" {
    run "$COLONY" -c 'test 5 -gt 10'
    [ "$status" -ne 0 ]
}

@test "test -gt: equal not greater" {
    run "$COLONY" -c 'test 5 -gt 5'
    [ "$status" -ne 0 ]
}

@test "test -gt: negative numbers" {
    run "$COLONY" -c 'test -5 -gt -10'
    [ "$status" -eq 0 ]
}

@test "[ -gt ]: greater than with bracket syntax" {
    run "$COLONY" -c '[ 10 -gt 5 ]'
    [ "$status" -eq 0 ]
}

# ============================================================================
# Integer Tests: -ge (greater or equal)
# ============================================================================

@test "test -ge: greater than" {
    run "$COLONY" -c 'test 10 -ge 5'
    [ "$status" -eq 0 ]
}

@test "test -ge: equal" {
    run "$COLONY" -c 'test 5 -ge 5'
    [ "$status" -eq 0 ]
}

@test "test -ge: less than" {
    run "$COLONY" -c 'test 5 -ge 10'
    [ "$status" -ne 0 ]
}

@test "[ -ge ]: greater or equal with bracket syntax" {
    run "$COLONY" -c '[ 5 -ge 5 ]'
    [ "$status" -eq 0 ]
}

# ============================================================================
# Logical Operators: ! (not)
# ============================================================================

@test "test !: negate true" {
    run "$COLONY" -c 'test ! -e /nonexistent'
    [ "$status" -eq 0 ]
}

@test "test !: negate false" {
    run "$COLONY" -c "test ! -e '$TEST_TEMP/regular_file'"
    [ "$status" -ne 0 ]
}

@test "test !: negate string equality" {
    run "$COLONY" -c 'test ! "a" = "b"'
    [ "$status" -eq 0 ]
}

@test "[ ! ]: negation with bracket syntax" {
    run "$COLONY" -c '[ ! -z "hello" ]'
    [ "$status" -eq 0 ]
}

# ============================================================================
# Logical Operators: -a (and)
# ============================================================================

@test "test -a: both true" {
    run "$COLONY" -c 'test -n "a" -a -n "b"'
    [ "$status" -eq 0 ]
}

@test "test -a: first false" {
    run "$COLONY" -c 'test -z "a" -a -n "b"'
    [ "$status" -ne 0 ]
}

@test "test -a: second false" {
    run "$COLONY" -c 'test -n "a" -a -z "b"'
    [ "$status" -ne 0 ]
}

@test "test -a: both false" {
    run "$COLONY" -c 'test -z "a" -a -z "b"'
    [ "$status" -ne 0 ]
}

@test "[ -a ]: and with bracket syntax" {
    run "$COLONY" -c '[ -n "a" -a -n "b" ]'
    [ "$status" -eq 0 ]
}

# ============================================================================
# Logical Operators: -o (or)
# ============================================================================

@test "test -o: both true" {
    run "$COLONY" -c 'test -n "a" -o -n "b"'
    [ "$status" -eq 0 ]
}

@test "test -o: first true" {
    run "$COLONY" -c 'test -n "a" -o -z "b"'
    [ "$status" -eq 0 ]
}

@test "test -o: second true" {
    run "$COLONY" -c 'test -z "a" -o -n "b"'
    [ "$status" -eq 0 ]
}

@test "test -o: both false" {
    run "$COLONY" -c 'test -z "a" -o -z "b"'
    [ "$status" -ne 0 ]
}

@test "[ -o ]: or with bracket syntax" {
    run "$COLONY" -c '[ -n "a" -o -n "b" ]'
    [ "$status" -eq 0 ]
}

# ============================================================================
# Logical Operators: ( ) (grouping)
# ============================================================================

@test "test: grouping changes evaluation" {
    # Without grouping: true -o false -a false = true (because -a binds tighter)
    # With grouping: (true -o false) -a false = false
    run "$COLONY" -c 'test \( -n "a" -o -z "b" \) -a -z "c"'
    [ "$status" -ne 0 ]
}

@test "test: nested grouping" {
    run "$COLONY" -c 'test \( -n "a" -a \( -n "b" -o -z "c" \) \)'
    [ "$status" -eq 0 ]
}

@test "[ ( ) ]: grouping with bracket syntax" {
    run "$COLONY" -c '[ \( -n "a" \) ]'
    [ "$status" -eq 0 ]
}

# ============================================================================
# Edge Cases
# ============================================================================

@test "test: no arguments returns false" {
    run "$COLONY" -c 'test'
    [ "$status" -ne 0 ]
}

@test "[]: empty brackets error" {
    run "$COLONY" -c '[ ]'
    [ "$status" -ne 0 ]
}

@test "test: string with spaces" {
    run "$COLONY" -c 'test "hello world" = "hello world"'
    [ "$status" -eq 0 ]
}

@test "test: string containing equals sign" {
    run "$COLONY" -c 'test "a=b" = "a=b"'
    [ "$status" -eq 0 ]
}

@test "test: string starting with dash" {
    run "$COLONY" -c 'test "-n" = "-n"'
    [ "$status" -eq 0 ]
}

@test "test: zero integer" {
    run "$COLONY" -c 'test 0 -eq 0'
    [ "$status" -eq 0 ]
}

@test "test: large integers" {
    run "$COLONY" -c 'test 999999 -lt 1000000'
    [ "$status" -eq 0 ]
}

@test "test: integer with leading zeros" {
    run "$COLONY" -c 'test 007 -eq 7'
    [ "$status" -eq 0 ]
}

@test "[]: missing closing bracket" {
    run "$COLONY" -c '[ -n "hello"'
    [ "$status" -ne 0 ]
}

@test "test: special characters in string" {
    run "$COLONY" -c 'test "!@#\$%" = "!@#\$%"'
    [ "$status" -eq 0 ]
}

@test "test: newline in string comparison" {
    run "$COLONY" -c $'test "a\nb" = "a\nb"'
    [ "$status" -eq 0 ]
}

# ============================================================================
# Variable Expansion in Tests
# ============================================================================

@test "test: variable in string comparison" {
    run "$COLONY" -c 'x=hello; test "$x" = "hello"'
    [ "$status" -eq 0 ]
}

@test "test: variable in integer comparison" {
    run "$COLONY" -c 'x=42; test "$x" -eq 42'
    [ "$status" -eq 0 ]
}

@test "test: unset variable in comparison" {
    run "$COLONY" -c 'test "$UNSET" = ""'
    [ "$status" -eq 0 ]
}

@test "test: arithmetic expansion in comparison" {
    run "$COLONY" -c 'test $((2+3)) -eq 5'
    [ "$status" -eq 0 ]
}

# ============================================================================
# Compare with Bash
# ============================================================================

@test "compare: file test with bash" {
    colony_status=0
    "$COLONY" -c "test -f '$TEST_TEMP/regular_file'" || colony_status=$?
    bash_status=0
    bash -c "test -f '$TEST_TEMP/regular_file'" || bash_status=$?
    [ "$colony_status" -eq "$bash_status" ]
}

@test "compare: string equality with bash" {
    colony_status=0
    "$COLONY" -c 'test "hello" = "hello"' || colony_status=$?
    bash_status=0
    bash -c 'test "hello" = "hello"' || bash_status=$?
    [ "$colony_status" -eq "$bash_status" ]
}

@test "compare: integer comparison with bash" {
    colony_status=0
    "$COLONY" -c 'test 10 -gt 5' || colony_status=$?
    bash_status=0
    bash -c 'test 10 -gt 5' || bash_status=$?
    [ "$colony_status" -eq "$bash_status" ]
}

@test "compare: negation with bash" {
    colony_status=0
    "$COLONY" -c 'test ! -z "hello"' || colony_status=$?
    bash_status=0
    bash -c 'test ! -z "hello"' || bash_status=$?
    [ "$colony_status" -eq "$bash_status" ]
}

@test "compare: complex expression with bash" {
    colony_status=0
    "$COLONY" -c 'test -n "a" -a 5 -lt 10' || colony_status=$?
    bash_status=0
    bash -c 'test -n "a" -a 5 -lt 10' || bash_status=$?
    [ "$colony_status" -eq "$bash_status" ]
}

@test "compare: bracket syntax with bash" {
    colony_status=0
    "$COLONY" -c '[ 42 -eq 42 ]' || colony_status=$?
    bash_status=0
    bash -c '[ 42 -eq 42 ]' || bash_status=$?
    [ "$colony_status" -eq "$bash_status" ]
}

# ============================================================================
# Conditional Execution Integration
# ============================================================================

@test "test in if statement" {
    run "$COLONY" -c 'if test -n "hello"; then echo yes; else echo no; fi'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

@test "[ ] in if statement" {
    run "$COLONY" -c 'if [ 5 -lt 10 ]; then echo yes; else echo no; fi'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

@test "test with && operator" {
    run "$COLONY" -c 'test -n "hello" && echo matched'
    [ "$status" -eq 0 ]
    [ "$output" = "matched" ]
}

@test "test with || operator" {
    run "$COLONY" -c 'test -z "hello" || echo fallback'
    [ "$status" -eq 0 ]
    [ "$output" = "fallback" ]
}

@test "[ ] in while loop condition" {
    run "$COLONY" -c 'i=0; while [ "$i" -lt 3 ]; do echo $i; i=$((i+1)); done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "0" ]
    [ "${lines[1]}" = "1" ]
    [ "${lines[2]}" = "2" ]
}
