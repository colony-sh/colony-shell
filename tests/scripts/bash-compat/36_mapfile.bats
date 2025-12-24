#!/usr/bin/env bats
# Test Colony shell mapfile/readarray builtin for bash compatibility
# mapfile reads lines from stdin into an array

COLONY="${COLONY_BIN}"

setup() {
    TEST_TEMP="$(mktemp -d)"
    export TEST_TEMP
    # Create test files
    printf 'line1\nline2\nline3\n' > "$TEST_TEMP/input.txt"
    printf 'a\nb\nc\nd\ne\n' > "$TEST_TEMP/five_lines.txt"
    printf 'first\nsecond\nthird\n' > "$TEST_TEMP/three_lines.txt"
}

teardown() {
    rm -rf "$TEST_TEMP"
}

# ============================================================================
# Basic mapfile functionality
# ============================================================================

@test "mapfile: basic read into default MAPFILE array" {
    run "$COLONY" -c 'printf "a\nb\nc\n" | { mapfile; echo "${#MAPFILE[@]}"; echo "[${MAPFILE[0]}][${MAPFILE[1]}][${MAPFILE[2]}]"; }'
    [ "$status" -eq 0 ]
    # Check we got 3 lines with their trailing newlines preserved
    [[ "${lines[0]}" = "3" ]]
}

@test "mapfile: basic read with -t strips newlines" {
    run "$COLONY" -c 'printf "a\nb\nc\n" | { mapfile -t arr; echo "${arr[0]}-${arr[1]}-${arr[2]}"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "a-b-c" ]
}

@test "mapfile: readarray alias works same as mapfile" {
    run "$COLONY" -c 'printf "x\ny\nz\n" | { readarray -t arr; echo "${arr[@]}"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "x y z" ]
}

@test "mapfile: read from file via redirection" {
    run "$COLONY" -c "mapfile -t arr < '$TEST_TEMP/input.txt'; echo \"\${arr[@]}\""
    [ "$status" -eq 0 ]
    [ "$output" = "line1 line2 line3" ]
}

@test "mapfile: custom array name" {
    run "$COLONY" -c 'printf "one\ntwo\n" | { mapfile -t myarr; echo "${myarr[0]} ${myarr[1]}"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "one two" ]
}

# ============================================================================
# -n option: limit number of lines
# ============================================================================

@test "mapfile -n: read only first 2 lines" {
    run "$COLONY" -c "mapfile -t -n 2 arr < '$TEST_TEMP/five_lines.txt'; echo \"\${arr[@]}\""
    [ "$status" -eq 0 ]
    [ "$output" = "a b" ]
}

@test "mapfile -n: read only first line" {
    run "$COLONY" -c 'printf "first\nsecond\nthird\n" | { mapfile -t -n 1 arr; echo "${arr[0]}"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "first" ]
}

@test "mapfile -n 0: read all lines (unlimited)" {
    run "$COLONY" -c 'printf "a\nb\nc\n" | { mapfile -t -n 0 arr; echo "${#arr[@]}"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "3" ]
}

# ============================================================================
# -s option: skip lines
# ============================================================================

@test "mapfile -s: skip first line" {
    run "$COLONY" -c "mapfile -t -s 1 arr < '$TEST_TEMP/three_lines.txt'; echo \"\${arr[@]}\""
    [ "$status" -eq 0 ]
    [ "$output" = "second third" ]
}

@test "mapfile -s: skip first 2 lines" {
    run "$COLONY" -c "mapfile -t -s 2 arr < '$TEST_TEMP/five_lines.txt'; echo \"\${arr[@]}\""
    [ "$status" -eq 0 ]
    [ "$output" = "c d e" ]
}

@test "mapfile -s -n: skip and limit combined" {
    run "$COLONY" -c "mapfile -t -s 1 -n 2 arr < '$TEST_TEMP/five_lines.txt'; echo \"\${arr[@]}\""
    [ "$status" -eq 0 ]
    [ "$output" = "b c" ]
}

# ============================================================================
# -O option: start index offset
# ============================================================================

@test "mapfile -O: start at index 2" {
    run "$COLONY" -c 'printf "x\ny\n" | { mapfile -t -O 2 arr; echo "${arr[2]}-${arr[3]}"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "x-y" ]
}

@test "mapfile -O: preserve existing array elements" {
    run "$COLONY" -c 'arr=(a b); printf "c\nd\n" | { mapfile -t -O 2 arr; echo "${arr[@]}"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "a b c d" ]
}

@test "mapfile -O: fills gaps with empty strings" {
    run "$COLONY" -c 'printf "x\n" | { mapfile -t -O 3 arr; echo "${#arr[@]}"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "4" ]
}

# ============================================================================
# -d option: custom delimiter
# ============================================================================

@test "mapfile -d: colon delimiter" {
    run "$COLONY" -c 'printf "a:b:c:" | { mapfile -t -d ":" arr; echo "${arr[@]}"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "a b c" ]
}

@test "mapfile -d: space delimiter" {
    run "$COLONY" -c 'printf "one two three " | { mapfile -t -d " " arr; echo "${arr[0]}|${arr[1]}|${arr[2]}"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "one|two|three" ]
}

@test "mapfile -d: NUL delimiter (empty string)" {
    # Using printf with \0
    run "$COLONY" -c 'printf "a\0b\0c\0" | { mapfile -t -d "" arr; echo "${arr[@]}"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "a b c" ]
}

# ============================================================================
# Array operations after mapfile
# ============================================================================

@test "mapfile: array length" {
    run "$COLONY" -c 'printf "1\n2\n3\n4\n5\n" | { mapfile -t arr; echo "${#arr[@]}"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "5" ]
}

@test "mapfile: iterate over array" {
    run "$COLONY" -c 'printf "a\nb\nc\n" | { mapfile -t arr; for x in "${arr[@]}"; do echo "[$x]"; done; }'
    [ "$status" -eq 0 ]
    [ "$output" = $'[a]\n[b]\n[c]' ]
}

@test "mapfile: array indices" {
    run "$COLONY" -c 'printf "x\ny\nz\n" | { mapfile -t arr; echo "${!arr[@]}"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "0 1 2" ]
}

# ============================================================================
# Edge cases
# ============================================================================

@test "mapfile: empty input creates empty array" {
    run "$COLONY" -c 'printf "" | { mapfile -t arr; echo "${#arr[@]}"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "0" ]
}

@test "mapfile: single line without trailing newline" {
    run "$COLONY" -c 'printf "single" | { mapfile -t arr; echo "${arr[0]}"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "single" ]
}

@test "mapfile: preserves whitespace in lines" {
    run "$COLONY" -c 'printf "  spaces  \n" | { mapfile -t arr; echo "[${arr[0]}]"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "[  spaces  ]" ]
}

@test "mapfile: handles blank lines" {
    run "$COLONY" -c 'printf "a\n\nb\n" | { mapfile -t arr; echo "${#arr[@]}"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "3" ]
}

@test "mapfile: blank line content is empty string" {
    run "$COLONY" -c 'printf "a\n\nb\n" | { mapfile -t arr; echo "[${arr[1]}]"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "[]" ]
}

# ============================================================================
# Error handling
# ============================================================================

@test "mapfile: -n requires argument" {
    run "$COLONY" -c 'printf "a\n" | mapfile -n'
    [ "$status" -ne 0 ]
}

@test "mapfile: -s requires argument" {
    run "$COLONY" -c 'printf "a\n" | mapfile -s'
    [ "$status" -ne 0 ]
}

@test "mapfile: -O requires argument" {
    run "$COLONY" -c 'printf "a\n" | mapfile -O'
    [ "$status" -ne 0 ]
}

@test "mapfile: -d requires argument" {
    run "$COLONY" -c 'printf "a\n" | mapfile -d'
    [ "$status" -ne 0 ]
}

@test "mapfile: invalid option" {
    run "$COLONY" -c 'printf "a\n" | mapfile -Z arr'
    [ "$status" -ne 0 ]
}

# ============================================================================
# Combined options
# ============================================================================

@test "mapfile: -t -n -s combined" {
    run "$COLONY" -c "mapfile -t -s 1 -n 2 arr < '$TEST_TEMP/five_lines.txt'; echo \"\${arr[@]}\""
    [ "$status" -eq 0 ]
    [ "$output" = "b c" ]
}

@test "mapfile: all options combined" {
    # Skip 1, read 2, start at index 1
    run "$COLONY" -c 'printf "a\nb\nc\nd\n" | { mapfile -t -s 1 -n 2 -O 1 arr; echo "${arr[1]}-${arr[2]}"; }'
    [ "$status" -eq 0 ]
    [ "$output" = "b-c" ]
}

# ============================================================================
# Comparison with bash (if available)
# ============================================================================

@test "mapfile: matches bash behavior - basic" {
    if ! command -v bash &>/dev/null; then
        skip "bash not available for comparison"
    fi
    
    bash_out=$(printf "x\ny\nz\n" | bash -c 'mapfile -t arr; echo "${arr[@]}"')
    colony_out=$(printf "x\ny\nz\n" | "$COLONY" -c '{ mapfile -t arr; echo "${arr[@]}"; }')
    
    [ "$colony_out" = "$bash_out" ]
}

@test "mapfile: matches bash behavior - skip and limit" {
    if ! command -v bash &>/dev/null; then
        skip "bash not available for comparison"  
    fi
    
    bash_out=$(printf "1\n2\n3\n4\n5\n" | bash -c 'mapfile -t -s 1 -n 2 arr; echo "${arr[@]}"')
    colony_out=$(printf "1\n2\n3\n4\n5\n" | "$COLONY" -c '{ mapfile -t -s 1 -n 2 arr; echo "${arr[@]}"; }')
    
    [ "$colony_out" = "$bash_out" ]
}
