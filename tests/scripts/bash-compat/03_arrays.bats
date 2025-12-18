#!/usr/bin/env bats
# Test Colony shell array support for bash compatibility
# Arrays are a bash extension, not strict POSIX

COLONY="${COLONY_BIN}"

setup() {
    TEST_TEMP="$(mktemp -d)"
    export TEST_TEMP
}

teardown() {
    rm -rf "$TEST_TEMP"
}

# ============================================================================
# Indexed Arrays - Basic Operations
# ============================================================================

@test "array: declare indexed array" {
    run "$COLONY" -c 'arr=(one two three); echo ${arr[0]}'
    [ "$status" -eq 0 ]
    [ "$output" = "one" ]
}

@test "array: access array elements by index" {
    run "$COLONY" -c 'arr=(a b c d); echo ${arr[2]}'
    [ "$status" -eq 0 ]
    [ "$output" = "c" ]
}

@test "array: last element with negative index" {
    run "$COLONY" -c 'arr=(a b c); echo ${arr[-1]}'
    [ "$status" -eq 0 ]
    [ "$output" = "c" ]
}

@test "array: array length" {
    run "$COLONY" -c 'arr=(one two three four); echo ${#arr[@]}'
    [ "$status" -eq 0 ]
    [ "$output" = "4" ]
}

@test "array: empty array length" {
    run "$COLONY" -c 'arr=(); echo ${#arr[@]}'
    [ "$status" -eq 0 ]
    [ "$output" = "0" ]
}

@test "array: all elements with @" {
    run "$COLONY" -c 'arr=(a b c); echo ${arr[@]}'
    [ "$status" -eq 0 ]
    [ "$output" = "a b c" ]
}

@test "array: all elements with *" {
    run "$COLONY" -c 'arr=(x y z); echo ${arr[*]}'
    [ "$status" -eq 0 ]
    [ "$output" = "x y z" ]
}

@test "array: sparse array" {
    run "$COLONY" -c 'arr=(); arr[0]=a; arr[5]=f; echo ${arr[0]} ${arr[5]}'
    [ "$status" -eq 0 ]
    [ "$output" = "a f" ]
}

@test "array: assign individual elements" {
    run "$COLONY" -c 'arr[0]=first; arr[1]=second; echo ${arr[0]} ${arr[1]}'
    [ "$status" -eq 0 ]
    [ "$output" = "first second" ]
}

@test "array: append with +=" {
    run "$COLONY" -c 'arr=(a b); arr+=(c d); echo ${arr[@]}'
    [ "$status" -eq 0 ]
    [ "$output" = "a b c d" ]
}

# ============================================================================
# Array Expansion and Slicing
# ============================================================================

@test "array: slice with offset" {
    run "$COLONY" -c 'arr=(a b c d e); echo ${arr[@]:2}'
    [ "$status" -eq 0 ]
    [ "$output" = "c d e" ]
}

@test "array: slice with offset and length" {
    run "$COLONY" -c 'arr=(a b c d e); echo ${arr[@]:1:3}'
    [ "$status" -eq 0 ]
    [ "$output" = "b c d" ]
}

@test "array: quoted expansion preserves spaces" {
    run "$COLONY" -c 'arr=("one two" "three four"); for x in "${arr[@]}"; do echo $x; done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "one two" ]
    [ "${lines[1]}" = "three four" ]
}

@test "array: unquoted expansion splits on IFS" {
    run "$COLONY" -c 'arr=("one two" three); echo ${arr[@]}'
    [ "$status" -eq 0 ]
    [ "$output" = "one two three" ]
}

@test "array: indices with !arr[@]" {
    run "$COLONY" -c 'arr=(); arr[1]=a; arr[5]=b; echo ${!arr[@]}'
    [ "$status" -eq 0 ]
    [ "$output" = "1 5" ]
}

# ============================================================================
# Array Iteration
# ============================================================================

@test "array: for loop over elements" {
    run "$COLONY" -c 'arr=(a b c); for x in ${arr[@]}; do echo $x; done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "a" ]
    [ "${lines[1]}" = "b" ]
    [ "${lines[2]}" = "c" ]
}

@test "array: for loop with indices" {
    run "$COLONY" -c 'arr=(x y z); for i in ${!arr[@]}; do echo $i:${arr[$i]}; done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "0:x" ]
    [ "${lines[1]}" = "1:y" ]
    [ "${lines[2]}" = "2:z" ]
}

@test "array: modify elements in loop" {
    run "$COLONY" -c 'arr=(a b c); for i in ${!arr[@]}; do arr[$i]=X; done; echo ${arr[@]}'
    [ "$status" -eq 0 ]
    [ "$output" = "X X X" ]
}

# ============================================================================
# Array in Commands
# ============================================================================

@test "array: pass array elements as command arguments" {
    run "$COLONY" -c 'files=(file1 file2 file3); echo ${files[@]}'
    [ "$status" -eq 0 ]
    [ "$output" = "file1 file2 file3" ]
}

@test "array: array from command substitution" {
    run "$COLONY" -c 'arr=($(echo one two three)); echo ${arr[1]}'
    [ "$status" -eq 0 ]
    [ "$output" = "two" ]
}

@test "array: array length after modification" {
    run "$COLONY" -c 'arr=(a b); arr+=(c); echo ${#arr[@]}'
    [ "$status" -eq 0 ]
    [ "$output" = "3" ]
}

# ============================================================================
# Associative Arrays (bash 4+)
# ============================================================================

@test "associative array: declare with -A" {
    run "$COLONY" -c 'declare -A map; map[key1]=value1; echo ${map[key1]}'
    [ "$status" -eq 0 ]
    [ "$output" = "value1" ]
}

@test "associative array: assign multiple keys" {
    run "$COLONY" -c 'declare -A map=([a]=1 [b]=2); echo ${map[a]} ${map[b]}'
    [ "$status" -eq 0 ]
    [ "$output" = "1 2" ]
}

@test "associative array: all keys" {
    run "$COLONY" -c 'declare -A map=([x]=1 [y]=2); echo ${!map[@]}'
    [ "$status" -eq 0 ]
    # Keys can be in any order
    [[ "$output" =~ "x" ]] && [[ "$output" =~ "y" ]]
}

@test "associative array: all values" {
    run "$COLONY" -c 'declare -A map=([a]=one [b]=two); echo ${map[@]}'
    [ "$status" -eq 0 ]
    # Values can be in any order
    [[ "$output" =~ "one" ]] && [[ "$output" =~ "two" ]]
}

@test "associative array: length" {
    run "$COLONY" -c 'declare -A map=([a]=1 [b]=2 [c]=3); echo ${#map[@]}'
    [ "$status" -eq 0 ]
    [ "$output" = "3" ]
}

# ============================================================================
# Bash Compatibility Comparison
# ============================================================================

@test "compare indexed array with bash" {
    run bash -c 'arr=(one two three); echo ${arr[1]}'
    local bash_output="$output"

    run "$COLONY" -c 'arr=(one two three); echo ${arr[1]}'
    local colony_output="$output"

    [ "$colony_output" = "$bash_output" ]
}

@test "compare array length with bash" {
    run bash -c 'arr=(a b c d e); echo ${#arr[@]}'
    local bash_output="$output"

    run "$COLONY" -c 'arr=(a b c d e); echo ${#arr[@]}'
    local colony_output="$output"

    [ "$colony_output" = "$bash_output" ]
}

@test "compare array slice with bash" {
    run bash -c 'arr=(a b c d e); echo ${arr[@]:1:3}'
    local bash_output="$output"

    run "$COLONY" -c 'arr=(a b c d e); echo ${arr[@]:1:3}'
    local colony_output="$output"

    [ "$colony_output" = "$bash_output" ]
}

@test "compare array iteration with bash" {
    run bash -c 'arr=(x y z); for v in ${arr[@]}; do echo $v; done'
    local bash_output="$output"

    run "$COLONY" -c 'arr=(x y z); for v in ${arr[@]}; do echo $v; done'
    local colony_output="$output"

    [ "$colony_output" = "$bash_output" ]
}

# ============================================================================
# Edge Cases
# ============================================================================

@test "array: undefined element is empty" {
    run "$COLONY" -c 'arr=(a b c); echo ">${arr[10]}<"'
    [ "$status" -eq 0 ]
    [ "$output" = "><" ]
}

@test "array: element with spaces" {
    run "$COLONY" -c 'arr=("hello world"); echo ${arr[0]}'
    [ "$status" -eq 0 ]
    [ "$output" = "hello world" ]
}

@test "array: empty array iteration" {
    run "$COLONY" -c 'arr=(); for x in ${arr[@]}; do echo $x; done; echo done'
    [ "$status" -eq 0 ]
    [ "$output" = "done" ]
}

@test "array: unset element" {
    run "$COLONY" -c 'arr=(a b c); unset arr[1]; echo ${arr[0]} ${arr[2]}'
    [ "$status" -eq 0 ]
    [ "$output" = "a c" ]
}

@test "array: unset entire array" {
    run "$COLONY" -c 'arr=(a b c); unset arr; echo ${#arr[@]}'
    [ "$status" -eq 0 ]
    [ "$output" = "0" ]
}
