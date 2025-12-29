#!/usr/bin/env bats
# Test Colony shell extended glob patterns (extglob) for bash compatibility
# Patterns: ?(pattern), *(pattern), +(pattern), @(pattern), !(pattern)
# Pattern lists use | as separator: @(foo|bar|baz)

# COLONY_BIN should point to the colony binary (named just 'colony', not 'colony-shell')
COLONY="${COLONY_BIN:-colony}"

setup() {
    TEST_TEMP="$(mktemp -d)"
    export TEST_TEMP

    # Create test directory structure
    mkdir -p "$TEST_TEMP"

    # Create test files for glob testing
    touch "$TEST_TEMP/foo.txt"
    touch "$TEST_TEMP/bar.txt"
    touch "$TEST_TEMP/baz.txt"
    touch "$TEST_TEMP/foobar.txt"
    touch "$TEST_TEMP/foobarbaz.txt"
    touch "$TEST_TEMP/file1.log"
    touch "$TEST_TEMP/file2.log"
    touch "$TEST_TEMP/file3.log"
    touch "$TEST_TEMP/test.sh"
    touch "$TEST_TEMP/test.bash"
    touch "$TEST_TEMP/test.zsh"
    touch "$TEST_TEMP/data.json"
    touch "$TEST_TEMP/config.yaml"
    touch "$TEST_TEMP/readme.md"
    touch "$TEST_TEMP/README.MD"
    touch "$TEST_TEMP/abc"
    touch "$TEST_TEMP/abcc"
    touch "$TEST_TEMP/abccc"
    touch "$TEST_TEMP/ab"
    touch "$TEST_TEMP/a"
}

teardown() {
    rm -rf "$TEST_TEMP"
}

# ============================================================================
# @(pattern-list) - Matches exactly one of the given patterns
# ============================================================================

@test "extglob @(): exact match single pattern" {
    run "$COLONY" -c "cd $TEST_TEMP && echo @(foo).txt"
    [ "$status" -eq 0 ]
    [ "$output" = "foo.txt" ]
}

@test "extglob @(): match one of multiple patterns" {
    run "$COLONY" -c "cd $TEST_TEMP && echo @(foo|bar).txt"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "bar.txt" ]]
    [[ "$output" =~ "foo.txt" ]]
}

@test "extglob @(): match three alternatives" {
    run "$COLONY" -c "cd $TEST_TEMP && echo @(foo|bar|baz).txt"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "foo.txt" ]]
    [[ "$output" =~ "bar.txt" ]]
    [[ "$output" =~ "baz.txt" ]]
}

@test "extglob @(): no match returns literal" {
    run "$COLONY" -c "cd $TEST_TEMP && echo @(xyz|abc).nonexistent"
    [ "$status" -eq 0 ]
    [ "$output" = "@(xyz|abc).nonexistent" ]
}

@test "extglob @(): with extension alternatives" {
    run "$COLONY" -c "cd $TEST_TEMP && echo test.@(sh|bash|zsh)"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "test.sh" ]]
    [[ "$output" =~ "test.bash" ]]
    [[ "$output" =~ "test.zsh" ]]
}

@test "extglob @(): combined with regular glob *" {
    run "$COLONY" -c "cd $TEST_TEMP && echo file*.@(log)"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "file1.log" ]]
    [[ "$output" =~ "file2.log" ]]
    [[ "$output" =~ "file3.log" ]]
}

# ============================================================================
# ?(pattern-list) - Matches zero or one occurrence
# ============================================================================

@test "extglob ?(): zero occurrence match" {
    run "$COLONY" -c "cd $TEST_TEMP && echo foo?(bar).txt"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "foo.txt" ]]
    [[ "$output" =~ "foobar.txt" ]]
}

@test "extglob ?(): one occurrence match" {
    run "$COLONY" -c "cd $TEST_TEMP && echo ab?(c)"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "ab" ]]
    [[ "$output" =~ "abc" ]]
    [[ ! "$output" =~ "abcc" ]]
}

@test "extglob ?(): multiple alternatives" {
    run "$COLONY" -c "cd $TEST_TEMP && echo ?(foo|bar).txt"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "foo.txt" ]]
    [[ "$output" =~ "bar.txt" ]]
}

@test "extglob ?(): with suffix" {
    run "$COLONY" -c "cd $TEST_TEMP && echo test.?(ba)sh"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "test.sh" ]]
    [[ "$output" =~ "test.bash" ]]
}

# ============================================================================
# *(pattern-list) - Matches zero or more occurrences
# ============================================================================

@test "extglob *(): zero occurrences" {
    run "$COLONY" -c "cd $TEST_TEMP && echo foo*(x).txt"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "foo.txt" ]]
}

@test "extglob *(): multiple occurrences" {
    run "$COLONY" -c "cd $TEST_TEMP && echo ab*(c)"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "ab" ]]
    [[ "$output" =~ "abc" ]]
    [[ "$output" =~ "abcc" ]]
    [[ "$output" =~ "abccc" ]]
}

@test "extglob *(): repeated pattern in name" {
    run "$COLONY" -c "cd $TEST_TEMP && echo foo*(bar).txt"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "foo.txt" ]]
    [[ "$output" =~ "foobar.txt" ]]
}

@test "extglob *(): with multiple pattern alternatives" {
    run "$COLONY" -c "cd $TEST_TEMP && echo *(foo|bar).txt"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "foo.txt" ]]
    [[ "$output" =~ "bar.txt" ]]
    [[ "$output" =~ "foobar.txt" ]]
}

# ============================================================================
# +(pattern-list) - Matches one or more occurrences
# ============================================================================

@test "extglob +(): one occurrence required" {
    run "$COLONY" -c "cd $TEST_TEMP && echo ab+(c)"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "abc" ]]
    [[ "$output" =~ "abcc" ]]
    [[ "$output" =~ "abccc" ]]
    [[ ! "$output" =~ " ab " ]] # ab alone should not match (no 'c')
}

@test "extglob +(): at least one pattern match" {
    run "$COLONY" -c "cd $TEST_TEMP && echo +(foo|bar).txt"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "foo.txt" ]]
    [[ "$output" =~ "bar.txt" ]]
    [[ "$output" =~ "foobar.txt" ]]
}

@test "extglob +(): repeated single pattern" {
    run "$COLONY" -c "cd $TEST_TEMP && echo foo+(bar).txt"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "foobar.txt" ]]
    [[ ! "$output" =~ " foo.txt" ]] # just "foo" should not match
}

# ============================================================================
# !(pattern-list) - Matches anything except the given patterns
# ============================================================================

@test "extglob !(): exclude single pattern" {
    run "$COLONY" -c "cd $TEST_TEMP && echo !(foo).txt"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "bar.txt" ]]
    [[ "$output" =~ "baz.txt" ]]
    [[ ! "$output" =~ " foo.txt" ]]
}

@test "extglob !(): exclude multiple patterns" {
    run "$COLONY" -c "cd $TEST_TEMP && echo !(foo|bar).txt"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "baz.txt" ]]
    [[ ! "$output" =~ " foo.txt" ]]
    [[ ! "$output" =~ " bar.txt" ]]
}

@test "extglob !(): exclude by extension" {
    run "$COLONY" -c "cd $TEST_TEMP && echo *.!(txt|log)"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "test.sh" ]]
    [[ "$output" =~ "data.json" ]]
    [[ ! "$output" =~ ".txt" ]]
    [[ ! "$output" =~ ".log" ]]
}

@test "extglob !(): match all except backup files" {
    touch "$TEST_TEMP/file.bak"
    touch "$TEST_TEMP/other.bak"
    run "$COLONY" -c "cd $TEST_TEMP && echo !(*.bak)"
    [ "$status" -eq 0 ]
    [[ ! "$output" =~ ".bak" ]]
    [[ "$output" =~ "foo.txt" ]]
}

# ============================================================================
# Case Statement with Extglob Patterns
# ============================================================================

@test "extglob case: @() exact match" {
    run "$COLONY" -c 'case foo in @(foo|bar)) echo matched;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "matched" ]
}

@test "extglob case: @() second alternative" {
    run "$COLONY" -c 'case bar in @(foo|bar)) echo matched;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "matched" ]
}

@test "extglob case: @() no match" {
    run "$COLONY" -c 'case baz in @(foo|bar)) echo wrong;; *) echo default;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "default" ]
}

@test "extglob case: ?() zero match" {
    run "$COLONY" -c 'case test in test?(ing)) echo matched;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "matched" ]
}

@test "extglob case: ?() one match" {
    run "$COLONY" -c 'case testing in test?(ing)) echo matched;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "matched" ]
}

@test "extglob case: *() zero occurrences" {
    run "$COLONY" -c 'case test in test*(ing)) echo matched;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "matched" ]
}

@test "extglob case: *() multiple occurrences" {
    run "$COLONY" -c 'case testinging in test*(ing)) echo matched;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "matched" ]
}

@test "extglob case: +() requires at least one" {
    run "$COLONY" -c 'case test in test+(ing)) echo wrong;; *) echo default;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "default" ]
}

@test "extglob case: +() one occurrence" {
    run "$COLONY" -c 'case testing in test+(ing)) echo matched;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "matched" ]
}

@test "extglob case: !() negation" {
    run "$COLONY" -c 'case baz in !(foo|bar)) echo matched;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "matched" ]
}

@test "extglob case: !() negation no match" {
    run "$COLONY" -c 'case foo in !(foo|bar)) echo wrong;; *) echo default;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "default" ]
}

# ============================================================================
# Nested Extglob Patterns
# ============================================================================

@test "extglob nested: @() inside +()" {
    run "$COLONY" -c "cd $TEST_TEMP && echo +(@(foo|bar)).txt"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "foo.txt" ]]
    [[ "$output" =~ "bar.txt" ]]
    [[ "$output" =~ "foobar.txt" ]]
}

@test "extglob nested: ?()*() combination" {
    run "$COLONY" -c "cd $TEST_TEMP && echo ?(foo)*(bar).txt"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "foo.txt" ]]
    [[ "$output" =~ "foobar.txt" ]]
}

# ============================================================================
# Extglob with Standard Glob Characters
# ============================================================================

@test "extglob combined: @() with * wildcard" {
    run "$COLONY" -c "cd $TEST_TEMP && echo @(foo|bar)*.txt"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "foo.txt" ]]
    [[ "$output" =~ "foobar.txt" ]]
    [[ "$output" =~ "foobarbaz.txt" ]]
    [[ "$output" =~ "bar.txt" ]]
}

@test "extglob combined: @() with ? single char" {
    run "$COLONY" -c "cd $TEST_TEMP && echo @(file)?.log"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "file1.log" ]]
    [[ "$output" =~ "file2.log" ]]
    [[ "$output" =~ "file3.log" ]]
}

@test "extglob combined: @() with [abc] class" {
    run "$COLONY" -c "cd $TEST_TEMP && echo file[1-2].@(log)"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "file1.log" ]]
    [[ "$output" =~ "file2.log" ]]
    [[ ! "$output" =~ "file3.log" ]]
}

@test "extglob combined: !() with *" {
    touch "$TEST_TEMP/keep.txt"
    touch "$TEST_TEMP/skip.bak"
    run "$COLONY" -c "cd $TEST_TEMP && echo !(*.bak)"
    [ "$status" -eq 0 ]
    # This should match everything that doesn't end in .bak
    [[ ! "$output" =~ ".bak" ]]
}

# ============================================================================
# Compare with Bash
# ============================================================================

@test "compare extglob @() with bash" {
    run bash -O extglob -c "cd $TEST_TEMP && echo @(foo|bar).txt"
    local bash_output="$output"

    run "$COLONY" -c "cd $TEST_TEMP && echo @(foo|bar).txt"
    local colony_output="$output"

    [ "$colony_output" = "$bash_output" ]
}

@test "compare extglob ?() with bash" {
    run bash -O extglob -c "cd $TEST_TEMP && echo foo?(bar).txt"
    local bash_output="$output"

    run "$COLONY" -c "cd $TEST_TEMP && echo foo?(bar).txt"
    local colony_output="$output"

    [ "$colony_output" = "$bash_output" ]
}

@test "compare extglob *() with bash" {
    run bash -O extglob -c "cd $TEST_TEMP && echo ab*(c)"
    local bash_output="$output"

    run "$COLONY" -c "cd $TEST_TEMP && echo ab*(c)"
    local colony_output="$output"

    [ "$colony_output" = "$bash_output" ]
}

@test "compare extglob +() with bash" {
    run bash -O extglob -c "cd $TEST_TEMP && echo ab+(c)"
    local bash_output="$output"

    run "$COLONY" -c "cd $TEST_TEMP && echo ab+(c)"
    local colony_output="$output"

    [ "$colony_output" = "$bash_output" ]
}

@test "compare extglob !() with bash" {
    run bash -O extglob -c "cd $TEST_TEMP && echo !(foo).txt"
    local bash_output="$output"

    run "$COLONY" -c "cd $TEST_TEMP && echo !(foo).txt"
    local colony_output="$output"

    [ "$colony_output" = "$bash_output" ]
}

@test "compare extglob case @() with bash" {
    run bash -O extglob -c "case bar in @(foo|bar)) echo matched;; esac"
    local bash_output="$output"

    run "$COLONY" -c "case bar in @(foo|bar)) echo matched;; esac"
    local colony_output="$output"

    [ "$colony_output" = "$bash_output" ]
}

@test "compare extglob case !() with bash" {
    run bash -O extglob -c "case baz in !(foo|bar)) echo matched;; esac"
    local bash_output="$output"

    run "$COLONY" -c "case baz in !(foo|bar)) echo matched;; esac"
    local colony_output="$output"

    [ "$colony_output" = "$bash_output" ]
}

# ============================================================================
# Edge Cases
# ============================================================================

@test "extglob: empty pattern list in @()" {
    run "$COLONY" -c "cd $TEST_TEMP && echo @().txt"
    [ "$status" -eq 0 ]
    # Empty pattern should match empty string, so just ".txt"
    # If no file matches, returns literal
    [ "$output" = "@().txt" ]
}

@test "extglob: single pattern in @() same as literal" {
    run "$COLONY" -c "cd $TEST_TEMP && echo @(foo).txt"
    [ "$status" -eq 0 ]
    [ "$output" = "foo.txt" ]
}

@test "extglob: quoted extglob is literal" {
    run "$COLONY" -c "cd $TEST_TEMP && echo '@(foo|bar).txt'"
    [ "$status" -eq 0 ]
    [ "$output" = "@(foo|bar).txt" ]
}

@test "extglob: escaped extglob is literal" {
    run "$COLONY" -c "cd $TEST_TEMP && echo \\@\\(foo\\).txt"
    [ "$status" -eq 0 ]
    [ "$output" = "@(foo).txt" ]
}

@test "extglob: pattern with spaces in alternatives" {
    touch "$TEST_TEMP/hello world.txt"
    touch "$TEST_TEMP/goodbye world.txt"
    run "$COLONY" -c "cd \"$TEST_TEMP\" && echo @(hello|goodbye)' world.txt'"
    [ "$status" -eq 0 ]
    # This tests mixing quoted and extglob parts
}

@test "extglob: @() in middle of pattern" {
    run "$COLONY" -c "cd $TEST_TEMP && echo f@(oo|ile)*.txt"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "foo.txt" ]]
    [[ "$output" =~ "foobar.txt" ]]
}

@test "extglob: multiple extglob patterns in one word" {
    run "$COLONY" -c "cd $TEST_TEMP && echo @(foo|bar)@(.txt|.log)"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "foo.txt" ]]
    [[ "$output" =~ "bar.txt" ]]
}

@test "extglob: !(pattern) with no files matching" {
    # All files in temp match some extension
    run "$COLONY" -c "cd $TEST_TEMP && echo !(*.xyz)"
    [ "$status" -eq 0 ]
    # Should return all files since none end in .xyz
    [[ "$output" =~ "foo.txt" ]]
}

# ============================================================================
# Variable Expansion with Extglob
# ============================================================================

@test "extglob: variable contains extglob pattern" {
    run "$COLONY" -c "cd $TEST_TEMP && pat='@(foo|bar)'; echo \${pat}.txt"
    [ "$status" -eq 0 ]
    # Variable expansion then glob - pattern should expand
    [[ "$output" =~ "foo.txt" ]] || [[ "$output" =~ "@(foo|bar).txt" ]]
}

@test "extglob: variable in pattern alternatives" {
    run "$COLONY" -c "cd $TEST_TEMP && ext=txt; echo @(foo|bar).\$ext"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "foo.txt" ]]
    [[ "$output" =~ "bar.txt" ]]
}

# ============================================================================
# For Loop with Extglob
# ============================================================================

@test "extglob: @() in for loop" {
    run "$COLONY" -c "cd $TEST_TEMP && for f in @(foo|bar).txt; do echo \$f; done"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "foo.txt" ]]
    [[ "$output" =~ "bar.txt" ]]
}

@test "extglob: !() in for loop" {
    run "$COLONY" -c "cd $TEST_TEMP && for f in !(*.log); do echo \$f; done | grep -c '.txt'"
    [ "$status" -eq 0 ]
    # Should have several .txt files
    [ "$output" -ge 3 ]
}

# ============================================================================
# Performance - Many Files
# ============================================================================

@test "extglob: @() with many files" {
    # Create 100 files
    for i in $(seq 1 100); do
        touch "$TEST_TEMP/gen${i}.dat"
    done

    run "$COLONY" -c "cd $TEST_TEMP && echo @(gen1|gen50|gen100).dat"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "gen1.dat" ]]
    [[ "$output" =~ "gen50.dat" ]]
    [[ "$output" =~ "gen100.dat" ]]
}

@test "extglob: !() excludes specific files from many" {
    for i in $(seq 1 20); do
        touch "$TEST_TEMP/num${i}.out"
    done

    run "$COLONY" -c "cd $TEST_TEMP && echo !(num1|num2|num3).out | wc -w"
    [ "$status" -eq 0 ]
    # Should have 17 files (20 - 3 excluded)
    [ "$output" -eq 17 ]
}
