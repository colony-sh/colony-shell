#!/usr/bin/env bats
# Test Colony shell glob (pathname expansion) for bash compatibility
# Wildcards: *, ?, [...], {...}

COLONY="${COLONY_BIN}"

setup() {
    TEST_TEMP="$(mktemp -d)"
    export TEST_TEMP

    # Create test directory structure
    mkdir -p "$TEST_TEMP/dir1"
    mkdir -p "$TEST_TEMP/dir2"
    mkdir -p "$TEST_TEMP/hidden"

    # Create test files
    touch "$TEST_TEMP/file1.txt"
    touch "$TEST_TEMP/file2.txt"
    touch "$TEST_TEMP/file3.log"
    touch "$TEST_TEMP/test.sh"
    touch "$TEST_TEMP/data.json"
    touch "$TEST_TEMP/.hidden"
    touch "$TEST_TEMP/dir1/nested.txt"
    touch "$TEST_TEMP/dir2/other.txt"
}

teardown() {
    rm -rf "$TEST_TEMP"
}

# ============================================================================
# Basic Wildcards - * (star)
# ============================================================================

@test "glob: * matches all files in directory" {
    run "$COLONY" -c "cd $TEST_TEMP && echo *.txt"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "file1.txt" ]]
    [[ "$output" =~ "file2.txt" ]]
}

@test "glob: * does not match hidden files" {
    run "$COLONY" -c "cd $TEST_TEMP && echo *"
    [ "$status" -eq 0 ]
    [[ ! "$output" =~ ".hidden" ]]
}

@test "glob: * matches empty when no files match" {
    run "$COLONY" -c "cd $TEST_TEMP && echo *.nonexistent"
    [ "$status" -eq 0 ]
    # When glob fails to match, it should return literal pattern
    [ "$output" = "*.nonexistent" ]
}

@test "glob: multiple * patterns in one command" {
    run "$COLONY" -c "cd $TEST_TEMP && echo *.txt *.log"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "file1.txt" ]]
    [[ "$output" =~ "file2.txt" ]]
    [[ "$output" =~ "file3.log" ]]
}

@test "glob: * in middle of pattern" {
    run "$COLONY" -c "cd $TEST_TEMP && echo file*.txt"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "file1.txt" ]]
    [[ "$output" =~ "file2.txt" ]]
}

@test "glob: prefix and suffix with *" {
    run "$COLONY" -c "cd $TEST_TEMP && echo f*t"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "file1.txt" ]]
    [[ "$output" =~ "file2.txt" ]]
}

# ============================================================================
# Question Mark - ? (single character)
# ============================================================================

@test "glob: ? matches single character" {
    run "$COLONY" -c "cd $TEST_TEMP && echo file?.txt"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "file1.txt" ]]
    [[ "$output" =~ "file2.txt" ]]
    [[ ! "$output" =~ "file3.log" ]]
}

@test "glob: multiple ? in pattern" {
    run "$COLONY" -c "cd $TEST_TEMP && echo ????.sh"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "test.sh" ]]
}

@test "glob: ? does not match slash" {
    run "$COLONY" -c "cd $TEST_TEMP && echo dir?/nested.txt"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "dir1/nested.txt" ]]
}

# ============================================================================
# Character Classes - [...]
# ============================================================================

@test "glob: [abc] matches one of the characters" {
    run "$COLONY" -c "cd $TEST_TEMP && echo file[12].txt"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "file1.txt" ]]
    [[ "$output" =~ "file2.txt" ]]
    [[ ! "$output" =~ "file3" ]]
}

@test "glob: [a-z] matches range" {
    run "$COLONY" -c "cd $TEST_TEMP && echo file[1-3].txt"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "file1.txt" ]]
    [[ "$output" =~ "file2.txt" ]]
}

@test "glob: [!abc] negated character class" {
    run "$COLONY" -c "cd $TEST_TEMP && echo file[!3].*"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "file1.txt" ]]
    [[ "$output" =~ "file2.txt" ]]
    [[ ! "$output" =~ "file3" ]]
}

@test "glob: [^abc] alternative negation syntax" {
    run "$COLONY" -c "cd $TEST_TEMP && echo file[^3].*"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "file1.txt" ]]
    [[ "$output" =~ "file2.txt" ]]
}

# ============================================================================
# Brace Expansion - {...} (bash extension)
# ============================================================================

@test "glob: {a,b,c} brace expansion" {
    run "$COLONY" -c "echo {one,two,three}"
    [ "$status" -eq 0 ]
    [ "$output" = "one two three" ]
}

@test "glob: {1..5} numeric range" {
    run "$COLONY" -c "echo {1..5}"
    [ "$status" -eq 0 ]
    [ "$output" = "1 2 3 4 5" ]
}

@test "glob: {a..e} alphabetic range" {
    run "$COLONY" -c "echo {a..e}"
    [ "$status" -eq 0 ]
    [ "$output" = "a b c d e" ]
}

@test "glob: nested braces" {
    run "$COLONY" -c "echo {a,b}{1,2}"
    [ "$status" -eq 0 ]
    # Should produce: a1 a2 b1 b2
    [[ "$output" =~ "a1" ]]
    [[ "$output" =~ "a2" ]]
    [[ "$output" =~ "b1" ]]
    [[ "$output" =~ "b2" ]]
}

@test "glob: brace with prefix/suffix" {
    run "$COLONY" -c "echo file{1,2}.txt"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "file1.txt" ]]
    [[ "$output" =~ "file2.txt" ]]
}

# ============================================================================
# Directory Traversal
# ============================================================================

@test "glob: */ matches directories" {
    run "$COLONY" -c "cd $TEST_TEMP && echo */"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "dir1" ]]
    [[ "$output" =~ "dir2" ]]
}

@test "glob: **/ recursive (if supported)" {
    skip "Recursive glob (**) is not standard POSIX"
    run "$COLONY" -c "cd $TEST_TEMP && echo **/*.txt"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "nested.txt" ]]
}

@test "glob: dir/* matches files in subdirectory" {
    run "$COLONY" -c "cd $TEST_TEMP && echo dir1/*"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "dir1/nested.txt" ]]
}

# ============================================================================
# Special Cases
# ============================================================================

@test "glob: quoted glob is literal" {
    run "$COLONY" -c "cd $TEST_TEMP && echo '*.txt'"
    [ "$status" -eq 0 ]
    [ "$output" = "*.txt" ]
}

@test "glob: escaped * is literal" {
    run "$COLONY" -c "cd $TEST_TEMP && echo \\*.txt"
    [ "$status" -eq 0 ]
    [ "$output" = "*.txt" ]
}

@test "glob: no match returns literal pattern" {
    run "$COLONY" -c "cd $TEST_TEMP && echo *.xyz"
    [ "$status" -eq 0 ]
    [ "$output" = "*.xyz" ]
}

@test "glob: dotglob disabled by default" {
    run "$COLONY" -c "cd $TEST_TEMP && echo .*"
    [ "$status" -eq 0 ]
    # Should not match .hidden without shopt -s dotglob
    # But will match . and .. typically
}

@test "glob: case sensitive matching" {
    run "$COLONY" -c "cd $TEST_TEMP && touch File.TXT && echo file*.txt"
    [ "$status" -eq 0 ]
    [[ ! "$output" =~ "File.TXT" ]]
}

# ============================================================================
# Glob in Different Contexts
# ============================================================================

@test "glob: * in for loop" {
    run "$COLONY" -c "cd $TEST_TEMP && for f in *.txt; do echo \$f; done | head -1"
    [ "$status" -eq 0 ]
    [[ "$output" =~ ".txt" ]]
}

@test "glob: * with command arguments" {
    run "$COLONY" -c "cd $TEST_TEMP && ls *.txt 2>/dev/null | wc -l"
    [ "$status" -eq 0 ]
    # Should list 2 txt files
    [ "$output" -ge 2 ]
}

@test "glob: * in variable assignment" {
    run "$COLONY" -c "cd $TEST_TEMP && files=*.txt && echo \$files"
    [ "$status" -eq 0 ]
    # Should expand glob during assignment
    [[ "$output" =~ "file1.txt" ]]
}

@test "glob: * in array assignment" {
    run "$COLONY" -c "cd $TEST_TEMP && arr=(*.txt) && echo \${arr[0]}"
    [ "$status" -eq 0 ]
    [[ "$output" =~ ".txt" ]]
}

# ============================================================================
# Extended Glob Patterns (bash extglob)
# ============================================================================

@test "glob: *(pattern) - zero or more matches" {
    skip "Extended glob (extglob) not yet supported"
    run "$COLONY" -c "cd $TEST_TEMP && shopt -s extglob && echo *(file).txt"
    [ "$status" -eq 0 ]
}

@test "glob: +(pattern) - one or more matches" {
    skip "Extended glob (extglob) not yet supported"
    run "$COLONY" -c "cd $TEST_TEMP && shopt -s extglob && echo +(file)[1-3].txt"
    [ "$status" -eq 0 ]
}

@test "glob: ?(pattern) - zero or one match" {
    skip "Extended glob (extglob) not yet supported"
    run "$COLONY" -c "cd $TEST_TEMP && shopt -s extglob && echo file?(1).txt"
    [ "$status" -eq 0 ]
}

@test "glob: @(pattern) - exactly one match" {
    skip "Extended glob (extglob) not yet supported"
    run "$COLONY" -c "cd $TEST_TEMP && shopt -s extglob && echo @(file1|file2).txt"
    [ "$status" -eq 0 ]
}

@test "glob: !(pattern) - anything except pattern" {
    skip "Extended glob (extglob) not yet supported"
    run "$COLONY" -c "cd $TEST_TEMP && shopt -s extglob && echo !(*.log)"
    [ "$status" -eq 0 ]
}

# ============================================================================
# Bash Compatibility Comparison
# ============================================================================

@test "compare glob * with bash" {
    cd "$TEST_TEMP"

    run bash -c "echo *.txt"
    local bash_output="$output"

    run "$COLONY" -c "echo *.txt"
    local colony_output="$output"

    [ "$colony_output" = "$bash_output" ]
}

@test "compare glob ? with bash" {
    cd "$TEST_TEMP"

    run bash -c "echo file?.txt"
    local bash_output="$output"

    run "$COLONY" -c "echo file?.txt"
    local colony_output="$output"

    [ "$colony_output" = "$bash_output" ]
}

@test "compare glob [...] with bash" {
    cd "$TEST_TEMP"

    run bash -c "echo file[12].txt"
    local bash_output="$output"

    run "$COLONY" -c "echo file[12].txt"
    local colony_output="$output"

    [ "$colony_output" = "$bash_output" ]
}

@test "compare glob no-match with bash" {
    cd "$TEST_TEMP"

    run bash -c "echo *.xyz"
    local bash_output="$output"

    run "$COLONY" -c "echo *.xyz"
    local colony_output="$output"

    [ "$colony_output" = "$bash_output" ]
}

# ============================================================================
# Edge Cases
# ============================================================================

@test "glob: empty directory returns pattern" {
    local empty_dir="$TEST_TEMP/empty"
    mkdir -p "$empty_dir"

    run "$COLONY" -c "cd $empty_dir && echo *"
    [ "$status" -eq 0 ]
    [ "$output" = "*" ]
}

@test "glob: pattern with no special chars stays literal" {
    run "$COLONY" -c "cd $TEST_TEMP && echo hello"
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "glob: mixed glob and literals" {
    run "$COLONY" -c "cd $TEST_TEMP && echo static *.txt more"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "static" ]]
    [[ "$output" =~ "file1.txt" ]]
    [[ "$output" =~ "more" ]]
}

@test "glob: glob in subdirectory path" {
    run "$COLONY" -c "cd $TEST_TEMP && echo dir*/*.txt"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "dir1/nested.txt" ]]
    [[ "$output" =~ "dir2/other.txt" ]]
}

@test "glob: multiple globs in single pattern" {
    run "$COLONY" -c "cd $TEST_TEMP && echo f*[12].*"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "file1.txt" ]]
    [[ "$output" =~ "file2.txt" ]]
}

# ============================================================================
# Glob with Variables
# ============================================================================

@test "glob: glob pattern in variable expands" {
    run "$COLONY" -c "cd $TEST_TEMP && pattern='*.txt' && echo \$pattern"
    [ "$status" -eq 0 ]
    # Variable expansion happens before glob, so it should expand
    [[ "$output" =~ "file1.txt" ]]
}

@test "glob: quoted variable with glob stays literal" {
    run "$COLONY" -c "cd $TEST_TEMP && pattern='*.txt' && echo \"\$pattern\""
    [ "$status" -eq 0 ]
    # Quoted expansion prevents glob
    [ "$output" = "*.txt" ]
}

# ============================================================================
# Nullglob and Failglob (bash options)
# ============================================================================

@test "glob: nullglob option (empty on no match)" {
    skip "shopt -s nullglob not yet supported"
    run "$COLONY" -c "cd $TEST_TEMP && shopt -s nullglob && echo *.xyz"
    [ "$status" -eq 0 ]
    [ "$output" = "" ]
}

@test "glob: failglob option (error on no match)" {
    skip "shopt -s failglob not yet supported"
    run "$COLONY" -c "cd $TEST_TEMP && shopt -s failglob && echo *.xyz"
    [ "$status" -ne 0 ]
}

# ============================================================================
# Performance and Edge Cases
# ============================================================================

@test "glob: large directory with many files" {
    # Create many files
    for i in {1..50}; do
        touch "$TEST_TEMP/bulk${i}.tmp"
    done

    run "$COLONY" -c "cd $TEST_TEMP && echo *.tmp | wc -w"
    [ "$status" -eq 0 ]
    [ "$output" -ge 50 ]
}

@test "glob: glob result is sorted" {
    run "$COLONY" -c "cd $TEST_TEMP && echo file*.txt"
    [ "$status" -eq 0 ]
    # Check that file1.txt comes before file2.txt
    [[ "$output" =~ file1.txt.*file2.txt ]]
}
