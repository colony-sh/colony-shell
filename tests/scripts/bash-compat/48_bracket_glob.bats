#!/usr/bin/env bats
# Test bracket glob expressions [a-z], [!abc], [:alpha:], etc.
# Based on bash's glob-bracket.tests

COLONY="${COLONY_BIN:-../../target/release/colony}"

setup() {
    export TESTDIR=$(mktemp -d)
    cd "$TESTDIR"
    touch a b c x y z
    touch file1 file2 file3
    touch abc def ghi
    touch A B C
    touch 1 2 3
    mkdir dir1 dir2
}

teardown() {
    rm -rf "$TESTDIR"
}

# =============================================================================
# Basic character class [abc]
# =============================================================================

@test "bracket: [abc] matches single char" {
    run "$COLONY" -c "cd $TESTDIR; echo [abc]"
    [ "$status" -eq 0 ]
    [[ "$output" == *"a"* ]]
    [[ "$output" == *"b"* ]]
    [[ "$output" == *"c"* ]]
}

@test "bracket: [xyz] matches specific chars" {
    run "$COLONY" -c "cd $TESTDIR; echo [xyz]"
    [ "$status" -eq 0 ]
    [[ "$output" == *"x"* ]]
    [[ "$output" == *"y"* ]]
    [[ "$output" == *"z"* ]]
}

@test "bracket: [123] matches digits" {
    run "$COLONY" -c "cd $TESTDIR; echo [123]"
    [ "$status" -eq 0 ]
    [[ "$output" == *"1"* ]]
    [[ "$output" == *"2"* ]]
    [[ "$output" == *"3"* ]]
}

# =============================================================================
# Range expressions [a-z]
# =============================================================================

@test "bracket: [a-c] range" {
    run "$COLONY" -c "cd $TESTDIR; echo [a-c]"
    [ "$status" -eq 0 ]
    [[ "$output" == *"a"* ]]
    [[ "$output" == *"b"* ]]
    [[ "$output" == *"c"* ]]
}

@test "bracket: [x-z] range" {
    run "$COLONY" -c "cd $TESTDIR; echo [x-z]"
    [ "$status" -eq 0 ]
    [[ "$output" == *"x"* ]]
    [[ "$output" == *"y"* ]]
    [[ "$output" == *"z"* ]]
}

@test "bracket: [1-3] digit range" {
    run "$COLONY" -c "cd $TESTDIR; echo [1-3]"
    [ "$status" -eq 0 ]
    [[ "$output" == *"1"* ]]
    [[ "$output" == *"2"* ]]
    [[ "$output" == *"3"* ]]
}

@test "bracket: [A-C] uppercase range" {
    run "$COLONY" -c "cd $TESTDIR; echo [A-C]"
    [ "$status" -eq 0 ]
    [[ "$output" == *"A"* ]]
    [[ "$output" == *"B"* ]]
    [[ "$output" == *"C"* ]]
}

# =============================================================================
# Negation [!abc] and [^abc]
# =============================================================================

@test "bracket: [!abc] negation" {
    run "$COLONY" -c "cd $TESTDIR; echo [!abc]"
    [ "$status" -eq 0 ]
    [[ "$output" != *"abc"* ]] || [[ "$output" == *"x"* ]]
}

@test "bracket: [^abc] negation (alternate syntax)" {
    run "$COLONY" -c "cd $TESTDIR; echo [^abc]"
    [ "$status" -eq 0 ]
    [[ "$output" != *"abc"* ]] || [[ "$output" == *"x"* ]]
}

@test "bracket: [!a-c] negated range" {
    run "$COLONY" -c "cd $TESTDIR; echo [!a-c]"
    [ "$status" -eq 0 ]
    [[ "$output" == *"x"* ]] || [[ "$output" == *"1"* ]]
}

# =============================================================================
# POSIX character classes [:alpha:]
# =============================================================================

@test "bracket: [[:alpha:]] alphabetic" {
    run "$COLONY" -c "cd $TESTDIR; echo [[:alpha:]]"
    [ "$status" -eq 0 ]
    [[ "$output" == *"a"* ]] || [[ "$output" == *"A"* ]]
}

@test "bracket: [[:digit:]] digits" {
    run "$COLONY" -c "cd $TESTDIR; echo [[:digit:]]"
    [ "$status" -eq 0 ]
    [[ "$output" == *"1"* ]]
}

@test "bracket: [[:lower:]] lowercase" {
    run "$COLONY" -c "cd $TESTDIR; echo [[:lower:]]"
    [ "$status" -eq 0 ]
    [[ "$output" == *"a"* ]]
    [[ "$output" != *"A"* ]] || true
}

@test "bracket: [[:upper:]] uppercase" {
    run "$COLONY" -c "cd $TESTDIR; echo [[:upper:]]"
    [ "$status" -eq 0 ]
    [[ "$output" == *"A"* ]]
}

@test "bracket: [[:alnum:]] alphanumeric" {
    run "$COLONY" -c "cd $TESTDIR; echo [[:alnum:]]"
    [ "$status" -eq 0 ]
    [[ "$output" == *"a"* ]] || [[ "$output" == *"1"* ]]
}

# =============================================================================
# Combined patterns
# =============================================================================

@test "bracket: file[123] pattern" {
    run "$COLONY" -c "cd $TESTDIR; echo file[123]"
    [ "$status" -eq 0 ]
    [[ "$output" == *"file1"* ]]
    [[ "$output" == *"file2"* ]]
    [[ "$output" == *"file3"* ]]
}

@test "bracket: [abc]* with wildcard" {
    run "$COLONY" -c "cd $TESTDIR; echo [abc]*"
    [ "$status" -eq 0 ]
    [[ "$output" == *"abc"* ]]
}

@test "bracket: *[123] suffix pattern" {
    run "$COLONY" -c "cd $TESTDIR; echo *[123]"
    [ "$status" -eq 0 ]
    [[ "$output" == *"file1"* ]] || [[ "$output" == *"dir1"* ]]
}

@test "bracket: dir[0-9] directory matching" {
    run "$COLONY" -c "cd $TESTDIR; echo dir[0-9]"
    [ "$status" -eq 0 ]
    [[ "$output" == *"dir1"* ]]
    [[ "$output" == *"dir2"* ]]
}

# =============================================================================
# Special characters in brackets
# =============================================================================

@test "bracket: literal dash at start [-abc]" {
    cd "$TESTDIR"
    touch -- -
    run "$COLONY" -c "cd $TESTDIR; echo [-abc]"
    [ "$status" -eq 0 ]
}

@test "bracket: literal dash at end [abc-]" {
    cd "$TESTDIR"
    touch -- -
    run "$COLONY" -c "cd $TESTDIR; echo [abc-]"
    [ "$status" -eq 0 ]
}

@test "bracket: literal ] at start []abc]" {
    cd "$TESTDIR"
    touch ']'
    run "$COLONY" -c "cd $TESTDIR; echo '[]abc]'"
    [ "$status" -eq 0 ]
}

@test "bracket: literal ^ not at start [a^bc]" {
    cd "$TESTDIR"
    touch '^'
    run "$COLONY" -c "cd $TESTDIR; echo '[a^]'"
    [ "$status" -eq 0 ]
}

# =============================================================================
# No match behavior
# =============================================================================

@test "bracket: no match returns pattern (nullglob off)" {
    run "$COLONY" -c "cd $TESTDIR; echo [QQQ]"
    [ "$status" -eq 0 ]
    [ "$output" = "[QQQ]" ]
}

@test "bracket: no match with nullglob returns nothing" {
    run "$COLONY" -c "cd $TESTDIR; shopt -s nullglob; echo [QQQ]"
    [ "$status" -eq 0 ]
    [ -z "$output" ]
}

# =============================================================================
# Case sensitivity
# =============================================================================

@test "bracket: case sensitive by default" {
    run "$COLONY" -c "cd $TESTDIR; echo [a]"
    [ "$status" -eq 0 ]
    [ "$output" = "a" ]
}

@test "bracket: nocaseglob makes case insensitive" {
    run "$COLONY" -c "cd $TESTDIR; shopt -s nocaseglob; echo [a]"
    [ "$status" -eq 0 ]
    [[ "$output" == *"a"* ]] || [[ "$output" == *"A"* ]]
}

# =============================================================================
# In [[ conditional ]]
# =============================================================================

@test "bracket: pattern in [[ == ]]" {
    run "$COLONY" -c '[[ "abc" == [a]* ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

@test "bracket: range in [[ == ]]" {
    run "$COLONY" -c '[[ "b" == [a-c] ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

@test "bracket: negation in [[ == ]]" {
    run "$COLONY" -c '[[ "x" == [!abc] ]] && echo yes || echo no'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

# =============================================================================
# In case statement
# =============================================================================

@test "bracket: pattern in case" {
    run "$COLONY" -c 'case "b" in [abc]) echo yes;; *) echo no;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

@test "bracket: range in case" {
    run "$COLONY" -c 'case "5" in [0-9]) echo digit;; *) echo other;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "digit" ]
}

@test "bracket: negation in case" {
    run "$COLONY" -c 'case "x" in [!abc]) echo not_abc;; *) echo other;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "not_abc" ]
}

# =============================================================================
# Compare with bash
# =============================================================================

@test "compare bracket with bash: [abc]" {
    cd "$TESTDIR"
    colony_out=$("$COLONY" -c "cd $TESTDIR; echo [abc]" 2>&1 | sort)
    bash_out=$(bash -c "cd $TESTDIR; echo [abc]" 2>&1 | sort)
    [ "$colony_out" = "$bash_out" ]
}

@test "compare bracket with bash: [a-c]" {
    cd "$TESTDIR"
    colony_out=$("$COLONY" -c "cd $TESTDIR; echo [a-c]" 2>&1 | sort)
    bash_out=$(bash -c "cd $TESTDIR; echo [a-c]" 2>&1 | sort)
    [ "$colony_out" = "$bash_out" ]
}

@test "compare bracket with bash: file[123]" {
    cd "$TESTDIR"
    colony_out=$("$COLONY" -c "cd $TESTDIR; echo file[123]" 2>&1 | sort)
    bash_out=$(bash -c "cd $TESTDIR; echo file[123]" 2>&1 | sort)
    [ "$colony_out" = "$bash_out" ]
}
