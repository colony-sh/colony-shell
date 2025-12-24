#!/usr/bin/env bats
# Test Colony shell variable attributes for bash compatibility
# Tests declare -i, -l, -u, -r, -x, -a, -A, -p

COLONY="${COLONY_BIN}"

setup() {
    TEST_TEMP="$(mktemp -d)"
    export TEST_TEMP
}

teardown() {
    rm -rf "$TEST_TEMP"
}

# ============================================================================
# Integer attribute (-i)
# ============================================================================

@test "declare -i: basic integer variable" {
    run "$COLONY" -c 'declare -i num=42; echo $num'
    [ "$status" -eq 0 ]
    [ "$output" = "42" ]
}

@test "declare -i: arithmetic on assignment" {
    run "$COLONY" -c 'declare -i num; num=3+5; echo $num'
    [ "$status" -eq 0 ]
    [ "$output" = "8" ]
}

@test "declare -i: arithmetic expression with variables" {
    run "$COLONY" -c 'declare -i num; a=10; b=5; num=a+b; echo $num'
    [ "$status" -eq 0 ]
    [ "$output" = "15" ]
}

@test "declare -i: string assigned to integer becomes 0" {
    run "$COLONY" -c 'declare -i num=hello; echo $num'
    [ "$status" -eq 0 ]
    [ "$output" = "0" ]
}

@test "declare -i: complex arithmetic expression" {
    run "$COLONY" -c 'declare -i num; num="2*3+4"; echo $num'
    [ "$status" -eq 0 ]
    [ "$output" = "10" ]
}

@test "declare -i: negative numbers" {
    run "$COLONY" -c 'declare -i num=-10; echo $num'
    [ "$status" -eq 0 ]
    [ "$output" = "-10" ]
}

@test "declare -i: arithmetic with parentheses" {
    run "$COLONY" -c 'declare -i num; num="(2+3)*4"; echo $num'
    [ "$status" -eq 0 ]
    [ "$output" = "20" ]
}

@test "declare -i: increment" {
    run "$COLONY" -c 'declare -i num=5; num+=3; echo $num'
    [ "$status" -eq 0 ]
    [ "$output" = "8" ]
}

@test "compare declare -i with bash" {
    run bash -c 'declare -i num; num=3+5; echo $num'
    local bash_output="$output"
    
    run "$COLONY" -c 'declare -i num; num=3+5; echo $num'
    [ "$output" = "$bash_output" ]
}

# ============================================================================
# Lowercase attribute (-l)
# ============================================================================

@test "declare -l: basic lowercase" {
    run "$COLONY" -c 'declare -l str="HELLO"; echo $str'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "declare -l: mixed case" {
    run "$COLONY" -c 'declare -l str="HeLLo WoRLD"; echo $str'
    [ "$status" -eq 0 ]
    [ "$output" = "hello world" ]
}

@test "declare -l: subsequent assignments also lowercase" {
    run "$COLONY" -c 'declare -l str; str="FIRST"; str="SECOND"; echo $str'
    [ "$status" -eq 0 ]
    [ "$output" = "second" ]
}

@test "declare -l: already lowercase" {
    run "$COLONY" -c 'declare -l str="already lowercase"; echo $str'
    [ "$status" -eq 0 ]
    [ "$output" = "already lowercase" ]
}

@test "compare declare -l with bash" {
    run bash -c 'declare -l str="HELLO WORLD"; echo $str'
    local bash_output="$output"
    
    run "$COLONY" -c 'declare -l str="HELLO WORLD"; echo $str'
    [ "$output" = "$bash_output" ]
}

# ============================================================================
# Uppercase attribute (-u)
# ============================================================================

@test "declare -u: basic uppercase" {
    run "$COLONY" -c 'declare -u str="hello"; echo $str'
    [ "$status" -eq 0 ]
    [ "$output" = "HELLO" ]
}

@test "declare -u: mixed case" {
    run "$COLONY" -c 'declare -u str="HeLLo WoRLD"; echo $str'
    [ "$status" -eq 0 ]
    [ "$output" = "HELLO WORLD" ]
}

@test "declare -u: subsequent assignments also uppercase" {
    run "$COLONY" -c 'declare -u str; str="first"; str="second"; echo $str'
    [ "$status" -eq 0 ]
    [ "$output" = "SECOND" ]
}

@test "declare -u: already uppercase" {
    run "$COLONY" -c 'declare -u str="ALREADY UPPERCASE"; echo $str'
    [ "$status" -eq 0 ]
    [ "$output" = "ALREADY UPPERCASE" ]
}

@test "compare declare -u with bash" {
    run bash -c 'declare -u str="hello world"; echo $str'
    local bash_output="$output"
    
    run "$COLONY" -c 'declare -u str="hello world"; echo $str'
    [ "$output" = "$bash_output" ]
}

# ============================================================================
# Readonly attribute (-r)
# ============================================================================

@test "declare -r: basic readonly" {
    run "$COLONY" -c 'declare -r const="value"; echo $const'
    [ "$status" -eq 0 ]
    [ "$output" = "value" ]
}

@test "declare -r: cannot modify readonly" {
    run "$COLONY" -c 'declare -r const="value"; const="newvalue"; echo $const'
    [ "$status" -ne 0 ]
}

@test "declare -r: cannot unset readonly" {
    run "$COLONY" -c 'declare -r const="value"; unset const'
    [ "$status" -ne 0 ]
}

@test "readonly builtin: same as declare -r" {
    run "$COLONY" -c 'readonly myconst="constant"; echo $myconst'
    [ "$status" -eq 0 ]
    [ "$output" = "constant" ]
}

@test "readonly builtin: cannot modify" {
    run "$COLONY" -c 'readonly myconst="constant"; myconst="changed"'
    [ "$status" -ne 0 ]
}

# ============================================================================
# Export attribute (-x)
# ============================================================================

@test "declare -x: export variable" {
    run "$COLONY" -c 'declare -x MYVAR="exported"; echo $MYVAR'
    [ "$status" -eq 0 ]
    [ "$output" = "exported" ]
}

@test "declare -x: child inherits exported variable" {
    run "$COLONY" -c 'declare -x MYVAR="inherited"; /bin/sh -c "echo \$MYVAR"'
    [ "$status" -eq 0 ]
    [ "$output" = "inherited" ]
}

@test "export builtin: same as declare -x" {
    run "$COLONY" -c 'export MYVAR="exported"; /bin/sh -c "echo \$MYVAR"'
    [ "$status" -eq 0 ]
    [ "$output" = "exported" ]
}

# ============================================================================
# Array declarations (-a, -A)
# ============================================================================

@test "declare -a: indexed array" {
    run "$COLONY" -c 'declare -a arr=(one two three); echo ${arr[1]}'
    [ "$status" -eq 0 ]
    [ "$output" = "two" ]
}

@test "declare -A: associative array" {
    run "$COLONY" -c 'declare -A map=([key1]=val1 [key2]=val2); echo ${map[key1]}'
    [ "$status" -eq 0 ]
    [ "$output" = "val1" ]
}

# ============================================================================
# Combined attributes
# ============================================================================

@test "declare -ir: readonly integer" {
    run "$COLONY" -c 'declare -ir num=42; echo $num'
    [ "$status" -eq 0 ]
    [ "$output" = "42" ]
}

@test "declare -ir: cannot modify readonly integer" {
    run "$COLONY" -c 'declare -ir num=42; num=100'
    [ "$status" -ne 0 ]
}

@test "declare -lx: lowercase and export" {
    run "$COLONY" -c 'declare -lx str="HELLO"; echo $str'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "declare -ux: uppercase and export" {
    run "$COLONY" -c 'declare -ux str="hello"; echo $str'
    [ "$status" -eq 0 ]
    [ "$output" = "HELLO" ]
}

# ============================================================================
# Print variables (-p)
# ============================================================================

@test "declare -p: print variable with attributes" {
    run "$COLONY" -c 'declare -i num=42; declare -p num'
    [ "$status" -eq 0 ]
    [[ "$output" == *"declare -i num="* ]] || [[ "$output" == *"declare -i num=\"42\""* ]]
}

@test "declare -p: print readonly variable" {
    run "$COLONY" -c 'declare -r const="value"; declare -p const'
    [ "$status" -eq 0 ]
    [[ "$output" == *"declare -r const="* ]] || [[ "$output" == *"readonly"* ]]
}

@test "declare -p: print exported variable" {
    run "$COLONY" -c 'declare -x exported="value"; declare -p exported'
    [ "$status" -eq 0 ]
    [[ "$output" == *"declare -x exported="* ]] || [[ "$output" == *"export"* ]]
}

@test "declare -p: print all declared variables" {
    run "$COLONY" -c 'declare -i num=42; declare -p | grep -c "declare "'
    [ "$status" -eq 0 ]
    # Should have at least one declared variable
    [ "$output" -ge 1 ]
}

# ============================================================================
# Typeset (alias for declare)
# ============================================================================

@test "typeset: alias for declare" {
    run "$COLONY" -c 'typeset -i num=10; echo $num'
    [ "$status" -eq 0 ]
    [ "$output" = "10" ]
}

@test "typeset -l: lowercase" {
    run "$COLONY" -c 'typeset -l str="HELLO"; echo $str'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

# ============================================================================
# Local with attributes
# ============================================================================

@test "local -i: local integer in function" {
    run "$COLONY" -c 'f() { local -i num; num=3+5; echo $num; }; f'
    [ "$status" -eq 0 ]
    [ "$output" = "8" ]
}

@test "local -r: local readonly in function" {
    run "$COLONY" -c 'f() { local -r const="value"; echo $const; }; f'
    [ "$status" -eq 0 ]
    [ "$output" = "value" ]
}

# ============================================================================
# Edge cases
# ============================================================================

@test "integer variable: division" {
    run "$COLONY" -c 'declare -i num; num=10/3; echo $num'
    [ "$status" -eq 0 ]
    [ "$output" = "3" ]
}

@test "integer variable: modulo" {
    run "$COLONY" -c 'declare -i num; num=10%3; echo $num'
    [ "$status" -eq 0 ]
    [ "$output" = "1" ]
}

@test "integer variable: bitwise" {
    run "$COLONY" -c 'declare -i num; num="5|3"; echo $num'
    [ "$status" -eq 0 ]
    [ "$output" = "7" ]
}

@test "lowercase: special characters unchanged" {
    run "$COLONY" -c 'declare -l str="ABC123!@#"; echo $str'
    [ "$status" -eq 0 ]
    [ "$output" = "abc123!@#" ]
}

@test "uppercase: special characters unchanged" {
    run "$COLONY" -c 'declare -u str="abc123!@#"; echo $str'
    [ "$status" -eq 0 ]
    [ "$output" = "ABC123!@#" ]
}

@test "unset removes attributes" {
    run "$COLONY" -c 'declare -i num=42; unset num; num="hello"; echo $num'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

# ============================================================================
# Bash comparison tests
# ============================================================================

@test "compare integer arithmetic with bash" {
    run bash -c 'declare -i num; num="5*3+2"; echo $num'
    local bash_output="$output"
    
    run "$COLONY" -c 'declare -i num; num="5*3+2"; echo $num'
    [ "$output" = "$bash_output" ]
}

@test "compare lowercase with bash" {
    run bash -c 'declare -l str="ABC XYZ"; echo $str'
    local bash_output="$output"
    
    run "$COLONY" -c 'declare -l str="ABC XYZ"; echo $str'
    [ "$output" = "$bash_output" ]
}

@test "compare uppercase with bash" {
    run bash -c 'declare -u str="abc xyz"; echo $str'
    local bash_output="$output"
    
    run "$COLONY" -c 'declare -u str="abc xyz"; echo $str'
    [ "$output" = "$bash_output" ]
}
