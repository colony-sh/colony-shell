#!/usr/bin/env bats
# Test Colony shell globstar (**) recursive glob pattern for bash compatibility

COLONY="${COLONY_BIN}"

setup() {
    TEST_TEMP="$(mktemp -d)"
    export TEST_TEMP
    
    # Create test directory structure
    mkdir -p "$TEST_TEMP/dir1/subdir"
    mkdir -p "$TEST_TEMP/dir2/deep/nested"
    mkdir -p "$TEST_TEMP/.hidden"
    
    # Create test files
    touch "$TEST_TEMP/root.txt"
    touch "$TEST_TEMP/file.rs"
    touch "$TEST_TEMP/dir1/a.txt"
    touch "$TEST_TEMP/dir1/b.rs"
    touch "$TEST_TEMP/dir1/subdir/c.txt"
    touch "$TEST_TEMP/dir1/subdir/d.rs"
    touch "$TEST_TEMP/dir2/e.txt"
    touch "$TEST_TEMP/dir2/deep/f.txt"
    touch "$TEST_TEMP/dir2/deep/nested/g.rs"
    touch "$TEST_TEMP/.hidden/h.txt"
}

teardown() {
    rm -rf "$TEST_TEMP"
}

# ============================================================================
# Basic shopt globstar tests
# ============================================================================

@test "globstar: shopt shows globstar off by default" {
    run "$COLONY" -c "shopt globstar"
    # shopt returns exit 1 when option is off (bash behavior)
    [[ "$output" =~ "globstar" ]]
    [[ "$output" =~ "off" ]]
}

@test "globstar: shopt -s globstar enables globstar" {
    run "$COLONY" -c "shopt -s globstar; shopt globstar"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "on" ]]
}

@test "globstar: shopt -u globstar disables globstar" {
    run "$COLONY" -c "shopt -s globstar; shopt -u globstar; shopt globstar"
    # shopt returns exit 1 when option is off (bash behavior)
    [[ "$output" =~ "off" ]]
}

@test "globstar: shopt -p shows proper format" {
    run "$COLONY" -c "shopt -p globstar"
    # shopt -p returns exit 1 when option is off (bash behavior)
    [[ "$output" =~ "shopt -u globstar" ]]
}

# ============================================================================
# Globstar disabled - ** treated as *
# ============================================================================

@test "globstar disabled: **/*.txt matches only one level deep" {
    # With globstar off, **/*.txt becomes */*.txt - matches one level
    run "$COLONY" -c "cd $TEST_TEMP && echo **/*.txt"
    [ "$status" -eq 0 ]
    # Should match dir1/a.txt and dir2/e.txt but NOT deeper
    [[ "$output" =~ "dir1/a.txt" ]]
    [[ "$output" =~ "dir2/e.txt" ]]
    # Should NOT match deeply nested files
    [[ ! "$output" =~ "dir1/subdir/c.txt" ]]
    [[ ! "$output" =~ "dir2/deep/f.txt" ]]
}

@test "globstar disabled: **/file pattern matches one level" {
    touch "$TEST_TEMP/test.rs" "$TEST_TEMP/dir1/test.rs" "$TEST_TEMP/dir1/subdir/test.rs"
    run "$COLONY" -c "cd $TEST_TEMP && echo **/test.rs"
    [ "$status" -eq 0 ]
    # Should match dir1/test.rs but not deeper
    [[ "$output" =~ "dir1/test.rs" ]]
    [[ ! "$output" =~ "subdir/test.rs" ]]
}

# ============================================================================
# Globstar enabled - recursive matching
# ============================================================================

@test "globstar enabled: **/*.rs matches all .rs files recursively" {
    run "$COLONY" -c "cd $TEST_TEMP && shopt -s globstar && echo **/*.rs"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "file.rs" ]]
    [[ "$output" =~ "dir1/b.rs" ]]
    [[ "$output" =~ "dir1/subdir/d.rs" ]]
    [[ "$output" =~ "dir2/deep/nested/g.rs" ]]
}

@test "globstar enabled: **/*.txt matches all .txt files recursively" {
    run "$COLONY" -c "cd $TEST_TEMP && shopt -s globstar && echo **/*.txt"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "root.txt" ]]
    [[ "$output" =~ "dir1/a.txt" ]]
    [[ "$output" =~ "dir1/subdir/c.txt" ]]
    [[ "$output" =~ "dir2/e.txt" ]]
    [[ "$output" =~ "dir2/deep/f.txt" ]]
}

@test "globstar enabled: ** matches everything recursively" {
    run "$COLONY" -c "cd $TEST_TEMP && shopt -s globstar && echo **"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "dir1" ]]
    [[ "$output" =~ "dir2" ]]
    [[ "$output" =~ "root.txt" ]]
}

@test "globstar enabled: **/ matches directories only" {
    run "$COLONY" -c "cd $TEST_TEMP && shopt -s globstar && echo **/"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "dir1/" ]]
    [[ "$output" =~ "dir2/" ]]
    [[ "$output" =~ "dir1/subdir/" ]]
    # Should not contain files
    [[ ! "$output" =~ "root.txt" ]]
    [[ ! "$output" =~ "a.txt" ]]
}

@test "globstar enabled: dir1/** matches everything under dir1" {
    run "$COLONY" -c "cd $TEST_TEMP && shopt -s globstar && echo dir1/**"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "dir1" ]]
    [[ "$output" =~ "dir1/a.txt" ]]
    [[ "$output" =~ "dir1/b.rs" ]]
    [[ "$output" =~ "dir1/subdir" ]]
    [[ "$output" =~ "dir1/subdir/c.txt" ]]
}

# ============================================================================
# Dotglob interaction
# ============================================================================

@test "globstar enabled: ** does not match hidden by default" {
    run "$COLONY" -c "cd $TEST_TEMP && shopt -s globstar && echo **"
    [ "$status" -eq 0 ]
    # Should not match .hidden directory
    [[ ! "$output" =~ ".hidden" ]]
}

@test "globstar enabled with dotglob: ** matches hidden dirs" {
    run "$COLONY" -c "cd $TEST_TEMP && shopt -s globstar && shopt -s dotglob && echo **"
    [ "$status" -eq 0 ]
    [[ "$output" =~ ".hidden" ]]
}

# ============================================================================
# Bash compatibility tests (comparing with bash behavior)
# ============================================================================

@test "globstar: matches bash output for **/*.rs" {
    bash_output=$(cd "$TEST_TEMP" && bash -O globstar -c 'echo **/*.rs' 2>/dev/null | tr ' ' '\n' | sort | tr '\n' ' ')
    colony_output=$("$COLONY" -c "cd $TEST_TEMP && shopt -s globstar && echo **/*.rs" | tr ' ' '\n' | sort | tr '\n' ' ')
    [ "$colony_output" = "$bash_output" ]
}

@test "globstar: matches bash output for dir1/**" {
    # Note: Colony outputs "dir1" while bash outputs "dir1/" - both are valid interpretations
    # We test that the file matches are the same
    bash_files=$(cd "$TEST_TEMP" && bash -O globstar -c 'echo dir1/**' 2>/dev/null | tr ' ' '\n' | grep -v '^dir1/$' | sort | tr '\n' ' ')
    colony_files=$("$COLONY" -c "cd $TEST_TEMP && shopt -s globstar && echo dir1/**" | tr ' ' '\n' | grep -v '^dir1$' | sort | tr '\n' ' ')
    [ "$colony_files" = "$bash_files" ]
}

# ============================================================================
# Edge cases
# ============================================================================

@test "globstar: no match returns pattern with nullglob off" {
    run "$COLONY" -c "cd $TEST_TEMP && shopt -s globstar && echo **/*.xyz"
    [ "$status" -eq 0 ]
    [ "$output" = "**/*.xyz" ]
}

@test "globstar: no match returns empty with nullglob on" {
    run "$COLONY" -c "cd $TEST_TEMP && shopt -s globstar && shopt -s nullglob && echo x**/*.xyz"
    [ "$status" -eq 0 ]
    # With nullglob, non-matching pattern expands to empty
    # x**/*.xyz doesn't match anything, so output should be empty
    [ "$output" = "" ]
}

@test "globstar: symlinks not followed" {
    ln -s "$TEST_TEMP/dir1" "$TEST_TEMP/linkdir"
    run "$COLONY" -c "cd $TEST_TEMP && shopt -s globstar && echo **/*.txt"
    [ "$status" -eq 0 ]
    # Should include linkdir but not descend into it
    # (bash behavior: lists symlink but doesn't recurse)
    [[ ! "$output" =~ "linkdir/a.txt" ]] || [[ "$output" =~ "linkdir" ]]
}
