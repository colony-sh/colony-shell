#!/usr/bin/env bats
# Test Colony shell case modification operators for bash compatibility
# Case modification is a bash extension, not strict POSIX
#
# Operators:
#   ${myvar^}   - Uppercase first character
#   ${myvar^^}  - Uppercase all characters
#   ${myvar,}   - Lowercase first character
#   ${myvar,,}  - Lowercase all characters
#   ${myvar~}   - Toggle case of first character
#   ${myvar~~}  - Toggle case of all characters
#
# With pattern:
#   ${myvar^pattern}  - Uppercase chars matching pattern
#   ${myvar,,pattern} - etc.

COLONY="${COLONY_BIN}"

setup() {
    TEST_TEMP="$(mktemp -d)"
    export TEST_TEMP
}

teardown() {
    rm -rf "$TEST_TEMP"
}

# ============================================================================
# Basic Uppercase Operations
# ============================================================================

@test "casemod: uppercase first character with ^" {
    run "$COLONY" -c 'myvar="hello"; echo "${myvar^}"'
    [ "$status" -eq 0 ]
    [ "$output" = "Hello" ]
}

@test "casemod: uppercase all characters with ^^" {
    run "$COLONY" -c 'myvar="hello world"; echo "${myvar^^}"'
    [ "$status" -eq 0 ]
    [ "$output" = "HELLO WORLD" ]
}

@test "casemod: uppercase first - already uppercase" {
    run "$COLONY" -c 'myvar="Hello"; echo "${myvar^}"'
    [ "$status" -eq 0 ]
    [ "$output" = "Hello" ]
}

@test "casemod: uppercase all - already uppercase" {
    run "$COLONY" -c 'myvar="HELLO"; echo "${myvar^^}"'
    [ "$status" -eq 0 ]
    [ "$output" = "HELLO" ]
}

@test "casemod: uppercase first - mixed case" {
    run "$COLONY" -c 'myvar="hELLO wORLD"; echo "${myvar^}"'
    [ "$status" -eq 0 ]
    [ "$output" = "HELLO wORLD" ]
}

# ============================================================================
# Basic Lowercase Operations
# ============================================================================

@test "casemod: lowercase first character with ," {
    run "$COLONY" -c 'myvar="HELLO"; echo "${myvar,}"'
    [ "$status" -eq 0 ]
    [ "$output" = "hELLO" ]
}

@test "casemod: lowercase all characters with ,," {
    run "$COLONY" -c 'myvar="HELLO WORLD"; echo "${myvar,,}"'
    [ "$status" -eq 0 ]
    [ "$output" = "hello world" ]
}

@test "casemod: lowercase first - already lowercase" {
    run "$COLONY" -c 'myvar="hello"; echo "${myvar,}"'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "casemod: lowercase all - already lowercase" {
    run "$COLONY" -c 'myvar="hello"; echo "${myvar,,}"'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "casemod: lowercase first - mixed case" {
    run "$COLONY" -c 'myvar="HeLLo WoRLd"; echo "${myvar,}"'
    [ "$status" -eq 0 ]
    [ "$output" = "heLLo WoRLd" ]
}

# ============================================================================
# Toggle Case Operations
# ============================================================================

@test "casemod: toggle first character with ~" {
    run "$COLONY" -c 'myvar="hello"; echo "${myvar~}"'
    [ "$status" -eq 0 ]
    [ "$output" = "Hello" ]
}

@test "casemod: toggle first character uppercase to lowercase" {
    run "$COLONY" -c 'myvar="HELLO"; echo "${myvar~}"'
    [ "$status" -eq 0 ]
    [ "$output" = "hELLO" ]
}

@test "casemod: toggle all characters with ~~" {
    run "$COLONY" -c 'myvar="HeLLo WoRLd"; echo "${myvar~~}"'
    [ "$status" -eq 0 ]
    [ "$output" = "hEllO wOrlD" ]
}

@test "casemod: toggle all - all lowercase" {
    run "$COLONY" -c 'myvar="hello"; echo "${myvar~~}"'
    [ "$status" -eq 0 ]
    [ "$output" = "HELLO" ]
}

@test "casemod: toggle all - all uppercase" {
    run "$COLONY" -c 'myvar="HELLO"; echo "${myvar~~}"'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

# ============================================================================
# Empty String and Edge Cases
# ============================================================================

@test "casemod: uppercase first - empty string" {
    run "$COLONY" -c 'myvar=""; echo ">${myvar^}<"'
    [ "$status" -eq 0 ]
    [ "$output" = "><" ]
}

@test "casemod: uppercase all - empty string" {
    run "$COLONY" -c 'myvar=""; echo ">${myvar^^}<"'
    [ "$status" -eq 0 ]
    [ "$output" = "><" ]
}

@test "casemod: lowercase first - empty string" {
    run "$COLONY" -c 'myvar=""; echo ">${myvar,}<"'
    [ "$status" -eq 0 ]
    [ "$output" = "><" ]
}

@test "casemod: lowercase all - empty string" {
    run "$COLONY" -c 'myvar=""; echo ">${myvar,,}<"'
    [ "$status" -eq 0 ]
    [ "$output" = "><" ]
}

@test "casemod: toggle first - empty string" {
    run "$COLONY" -c 'myvar=""; echo ">${myvar~}<"'
    [ "$status" -eq 0 ]
    [ "$output" = "><" ]
}

@test "casemod: toggle all - empty string" {
    run "$COLONY" -c 'myvar=""; echo ">${myvar~~}<"'
    [ "$status" -eq 0 ]
    [ "$output" = "><" ]
}

@test "casemod: single character uppercase" {
    run "$COLONY" -c 'myvar="a"; echo "${myvar^}"'
    [ "$status" -eq 0 ]
    [ "$output" = "A" ]
}

@test "casemod: single character lowercase" {
    run "$COLONY" -c 'myvar="A"; echo "${myvar,}"'
    [ "$status" -eq 0 ]
    [ "$output" = "a" ]
}

@test "casemod: single character toggle" {
    run "$COLONY" -c 'myvar="a"; echo "${myvar~}"'
    [ "$status" -eq 0 ]
    [ "$output" = "A" ]
}

# ============================================================================
# Non-alphabetic Characters (Should Be Unchanged)
# ============================================================================

@test "casemod: uppercase first - starts with number" {
    run "$COLONY" -c 'myvar="123abc"; echo "${myvar^}"'
    [ "$status" -eq 0 ]
    [ "$output" = "123abc" ]
}

@test "casemod: uppercase all - with numbers" {
    run "$COLONY" -c 'myvar="abc123def"; echo "${myvar^^}"'
    [ "$status" -eq 0 ]
    [ "$output" = "ABC123DEF" ]
}

@test "casemod: special characters unchanged" {
    run "$COLONY" -c "myvar='hello!@#'; echo \"\${myvar^^}\""
    [ "$status" -eq 0 ]
    [ "$output" = "HELLO!@#" ]
}

@test "casemod: toggle with punctuation" {
    run "$COLONY" -c 'myvar="Hello, World!"; echo "${myvar~~}"'
    [ "$status" -eq 0 ]
    [ "$output" = "hELLO, wORLD!" ]
}

# ============================================================================
# Unset Variables
# ============================================================================

@test "casemod: uppercase first - unset myvariable" {
    run "$COLONY" -c 'unset myvar; echo ">${myvar^}<"'
    [ "$status" -eq 0 ]
    [ "$output" = "><" ]
}

@test "casemod: uppercase all - unset myvariable" {
    run "$COLONY" -c 'unset myvar; echo ">${myvar^^}<"'
    [ "$status" -eq 0 ]
    [ "$output" = "><" ]
}

# ============================================================================
# Array Operations
# ============================================================================

@test "casemod: uppercase all array elements" {
    run "$COLONY" -c 'arr=(hello world); echo "${arr[@]^^}"'
    [ "$status" -eq 0 ]
    [ "$output" = "HELLO WORLD" ]
}

@test "casemod: uppercase first in each array element" {
    run "$COLONY" -c 'arr=(hello world); echo "${arr[@]^}"'
    [ "$status" -eq 0 ]
    [ "$output" = "Hello World" ]
}

@test "casemod: lowercase all array elements" {
    run "$COLONY" -c 'arr=(HELLO WORLD); echo "${arr[@],,}"'
    [ "$status" -eq 0 ]
    [ "$output" = "hello world" ]
}

@test "casemod: lowercase first in each array element" {
    run "$COLONY" -c 'arr=(HELLO WORLD); echo "${arr[@],}"'
    [ "$status" -eq 0 ]
    [ "$output" = "hELLO wORLD" ]
}

@test "casemod: toggle all in array elements" {
    run "$COLONY" -c 'arr=(Hello World); echo "${arr[@]~~}"'
    [ "$status" -eq 0 ]
    [ "$output" = "hELLO wORLD" ]
}

@test "casemod: single array element" {
    run "$COLONY" -c 'arr=(hello world); echo "${arr[0]^^}"'
    [ "$status" -eq 0 ]
    [ "$output" = "HELLO" ]
}

@test "casemod: array with * subscript" {
    run "$COLONY" -c 'arr=(hello world); echo "${arr[*]^^}"'
    [ "$status" -eq 0 ]
    [ "$output" = "HELLO WORLD" ]
}

# ============================================================================
# Bash Compatibility Comparison
# ============================================================================

@test "compare uppercase first with bash" {
    run bash -c 'myvar="hello world"; echo "${myvar^}"'
    local bash_output="$output"
    
    run "$COLONY" -c 'myvar="hello world"; echo "${myvar^}"'
    local colony_output="$output"
    
    [ "$colony_output" = "$bash_output" ]
}

@test "compare uppercase all with bash" {
    run bash -c 'myvar="hello world"; echo "${myvar^^}"'
    local bash_output="$output"
    
    run "$COLONY" -c 'myvar="hello world"; echo "${myvar^^}"'
    local colony_output="$output"
    
    [ "$colony_output" = "$bash_output" ]
}

@test "compare lowercase first with bash" {
    run bash -c 'myvar="HELLO WORLD"; echo "${myvar,}"'
    local bash_output="$output"
    
    run "$COLONY" -c 'myvar="HELLO WORLD"; echo "${myvar,}"'
    local colony_output="$output"
    
    [ "$colony_output" = "$bash_output" ]
}

@test "compare lowercase all with bash" {
    run bash -c 'myvar="HELLO WORLD"; echo "${myvar,,}"'
    local bash_output="$output"
    
    run "$COLONY" -c 'myvar="HELLO WORLD"; echo "${myvar,,}"'
    local colony_output="$output"
    
    [ "$colony_output" = "$bash_output" ]
}

@test "compare toggle first with bash" {
    run bash -c 'myvar="hello"; echo "${myvar~}"'
    local bash_output="$output"
    
    run "$COLONY" -c 'myvar="hello"; echo "${myvar~}"'
    local colony_output="$output"
    
    [ "$colony_output" = "$bash_output" ]
}

@test "compare toggle all with bash" {
    run bash -c 'myvar="HeLLo WoRLd"; echo "${myvar~~}"'
    local bash_output="$output"
    
    run "$COLONY" -c 'myvar="HeLLo WoRLd"; echo "${myvar~~}"'
    local colony_output="$output"
    
    [ "$colony_output" = "$bash_output" ]
}

@test "compare array uppercase with bash" {
    run bash -c 'arr=(hello world); echo "${arr[@]^^}"'
    local bash_output="$output"
    
    run "$COLONY" -c 'arr=(hello world); echo "${arr[@]^^}"'
    local colony_output="$output"
    
    [ "$colony_output" = "$bash_output" ]
}

# ============================================================================
# Combined with Other Expansions
# ============================================================================

@test "casemod: with default value" {
    run "$COLONY" -c 'unset myvar; echo "${myvar:-hello}"; echo "${myvar,,}"'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "hello" ]
    [ "${lines[1]}" = "" ]
}

@test "casemod: uppercase in double quotes" {
    run "$COLONY" -c 'myvar="hello world"; echo "Result: ${myvar^^}"'
    [ "$status" -eq 0 ]
    [ "$output" = "Result: HELLO WORLD" ]
}

@test "casemod: in command substitution" {
    run "$COLONY" -c 'myvar="hello"; result=$(echo "${myvar^^}"); echo "$result"'
    [ "$status" -eq 0 ]
    [ "$output" = "HELLO" ]
}

@test "casemod: in for loop" {
    run "$COLONY" -c 'for word in hello world; do echo "${word^}"; done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "Hello" ]
    [ "${lines[1]}" = "World" ]
}

@test "casemod: multiple variables" {
    run "$COLONY" -c 'a="hello"; b="WORLD"; echo "${a^^} ${b,,}"'
    [ "$status" -eq 0 ]
    [ "$output" = "HELLO world" ]
}

# ============================================================================
# Unicode Support (Basic)
# ============================================================================

@test "casemod: basic ASCII only" {
    # This test verifies basic ASCII works; Unicode support is optional
    run "$COLONY" -c 'myvar="cafe"; echo "${myvar^^}"'
    [ "$status" -eq 0 ]
    [ "$output" = "CAFE" ]
}
