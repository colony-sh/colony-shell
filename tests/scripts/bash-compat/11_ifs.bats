#!/usr/bin/env bats
# IFS (Internal Field Separator) tests - based on bash tests/ifs.tests
# Tests field splitting behavior with different IFS values

setup() {
    COLONY="${COLONY_BIN:-../../../target/release/colony}"
}

# ============================================================================
# Default IFS Behavior (space, tab, newline)
# ============================================================================

@test "ifs: default splitting on spaces" {
    run $COLONY -c 'var="a b c"; for i in $var; do echo $i; done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "a" ]
    [ "${lines[1]}" = "b" ]
    [ "${lines[2]}" = "c" ]
}

@test "ifs: default splitting on tabs" {
    run $COLONY -c 'var="a	b	c"; for i in $var; do echo $i; done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "a" ]
    [ "${lines[1]}" = "b" ]
    [ "${lines[2]}" = "c" ]
}

@test "ifs: default splitting on newlines" {
    run $COLONY -c 'var="a
b
c"; for i in $var; do echo $i; done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "a" ]
    [ "${lines[1]}" = "b" ]
    [ "${lines[2]}" = "c" ]
}

@test "ifs: default splitting with mixed whitespace" {
    run $COLONY -c 'var="a  b	c  d"; for i in $var; do echo $i; done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "a" ]
    [ "${lines[1]}" = "b" ]
    [ "${lines[2]}" = "c" ]
    [ "${lines[3]}" = "d" ]
}

@test "ifs: leading and trailing whitespace stripped" {
    run $COLONY -c 'var="  a b c  "; for i in $var; do echo $i; done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "a" ]
    [ "${lines[1]}" = "b" ]
    [ "${lines[2]}" = "c" ]
}

# ============================================================================
# Custom IFS - Single Character
# ============================================================================

@test "ifs: custom IFS colon" {
    run $COLONY -c 'IFS=:; var="a:b:c"; for i in $var; do echo $i; done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "a" ]
    [ "${lines[1]}" = "b" ]
    [ "${lines[2]}" = "c" ]
}

@test "ifs: custom IFS comma" {
    run $COLONY -c 'IFS=,; var="x,y,z"; for i in $var; do echo $i; done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "x" ]
    [ "${lines[1]}" = "y" ]
    [ "${lines[2]}" = "z" ]
}

@test "ifs: custom IFS preserves other characters" {
    run $COLONY -c 'IFS=:; var="a b:c d"; for i in $var; do echo $i; done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "a b" ]
    [ "${lines[1]}" = "c d" ]
}

# ============================================================================
# IFS with Command Substitution
# ============================================================================

@test "ifs: custom IFS with dollar paren comsub" {
    run $COLONY -c 'IFS=:; var=$(echo a:b:c); for i in $var; do echo $i; done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "a" ]
    [ "${lines[1]}" = "b" ]
    [ "${lines[2]}" = "c" ]
}

@test "ifs: custom IFS with backtick comsub" {
    run $COLONY -c 'IFS=:; var=`echo a:b:c`; for i in $var; do echo $i; done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "a" ]
    [ "${lines[1]}" = "b" ]
    [ "${lines[2]}" = "c" ]
}

@test "ifs: IFS affects comsub word splitting" {
    run $COLONY -c 'IFS=:; for i in $(echo a:b:c); do echo $i; done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "a" ]
    [ "${lines[1]}" = "b" ]
    [ "${lines[2]}" = "c" ]
}

# ============================================================================
# IFS Scoping and Preservation
# ============================================================================

@test "ifs: save and restore IFS" {
    run $COLONY -c 'OIFS=$IFS; IFS=:; var="a:b:c"; IFS=$OIFS; echo $var'
    [ "$status" -eq 0 ]
    [ "$output" = "a:b:c" ]
}

@test "ifs: IFS change affects subsequent operations" {
    run $COLONY -c 'var="a:b:c"; echo $var; IFS=:; echo $var'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "a:b:c" ]
    [ "${lines[1]}" = "a b c" ]
}

@test "ifs: temporary IFS for single command" {
    run $COLONY -c 'var="a:b:c"; IFS=: echo $var; echo done'
    [ "$status" -eq 0 ]
    # IFS assignment before builtin doesn't affect that command in most contexts
    [ "${lines[0]}" = "a:b:c" ]
    [ "${lines[1]}" = "done" ]
}

# ============================================================================
# Empty Fields and Edge Cases
# ============================================================================

@test "ifs: empty field at start" {
    run $COLONY -c 'IFS=:; var=":b:c"; for i in $var; do echo "[$i]"; done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "[]" ]
    [ "${lines[1]}" = "[b]" ]
    [ "${lines[2]}" = "[c]" ]
}

@test "ifs: empty field in middle" {
    run $COLONY -c 'IFS=:; var="a::c"; for i in $var; do echo "[$i]"; done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "[a]" ]
    [ "${lines[1]}" = "[]" ]
    [ "${lines[2]}" = "[c]" ]
}

@test "ifs: empty field at end" {
    run $COLONY -c 'IFS=:; var="a:b:"; for i in $var; do echo "[$i]"; done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "[a]" ]
    [ "${lines[1]}" = "[b]" ]
}

@test "ifs: only delimiter" {
    run $COLONY -c 'IFS=:; var=":"; for i in $var; do echo "[$i]"; done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "[]" ]
}

@test "ifs: empty string splits to nothing" {
    run $COLONY -c 'IFS=:; var=""; for i in $var; do echo "[$i]"; done; echo done'
    [ "$status" -eq 0 ]
    [ "$output" = "done" ]
}

# ============================================================================
# Multi-Character IFS
# ============================================================================

@test "ifs: IFS with multiple delimiters" {
    run $COLONY -c 'IFS=:,; var="a:b,c"; for i in $var; do echo $i; done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "a" ]
    [ "${lines[1]}" = "b" ]
    [ "${lines[2]}" = "c" ]
}

@test "ifs: IFS with colon and space" {
    run $COLONY -c 'IFS=": "; var="a:b c"; for i in $var; do echo $i; done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "a" ]
    [ "${lines[1]}" = "b" ]
    [ "${lines[2]}" = "c" ]
}

# ============================================================================
# IFS in Function Scope
# ============================================================================

@test "ifs: local IFS in function" {
    run $COLONY -c 'f() { local IFS=:; echo $1; }; f "a:b:c:d"'
    [ "$status" -eq 0 ]
    [ "$output" = "a:b:c:d" ]
}

@test "ifs: function IFS doesn't affect caller" {
    # Function sets IFS=: which causes word splitting inside the function
    # After function returns, caller's IFS should still be space
    run $COLONY -c 'f() { IFS=:; var="a:b:c"; echo $var; }; IFS=" "; f; var="x y"; echo $var'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "a b c" ]  # IFS=: causes splitting inside function
    [ "${lines[1]}" = "x y" ]    # Caller's IFS unchanged
}

# ============================================================================
# Quoted Variables (No Splitting)
# ============================================================================

@test "ifs: quoted variable prevents splitting" {
    run $COLONY -c 'IFS=:; var="a:b:c"; echo "$var"'
    [ "$status" -eq 0 ]
    [ "$output" = "a:b:c" ]
}

@test "ifs: unquoted vs quoted in for loop" {
    run $COLONY -c 'IFS=:; var="a:b"; for i in $var; do echo unquoted-$i; done; for i in "$var"; do echo quoted-$i; done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "unquoted-a" ]
    [ "${lines[1]}" = "unquoted-b" ]
    [ "${lines[2]}" = "quoted-a b" ]
}

# ============================================================================
# IFS with Special Characters
# ============================================================================

@test "ifs: empty IFS disables splitting" {
    run $COLONY -c 'IFS=""; var="a b c"; echo $var'
    [ "$status" -eq 0 ]
    [ "$output" = "a b c" ]
}

@test "ifs: IFS with newline character" {
    run $COLONY -c 'IFS=$'"'"'\n'"'"'; var="a
b
c"; for i in $var; do echo $i; done'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "a" ]
    [ "${lines[1]}" = "b" ]
    [ "${lines[2]}" = "c" ]
}

# ============================================================================
# IFS and Parameter Expansion
# ============================================================================

@test "ifs: IFS affects parameter expansion word splitting" {
    # ${var} is word split by IFS, then echo rejoins with spaces
    run $COLONY -c 'IFS=:; var="a:b:c"; echo ${var}'
    [ "$status" -eq 0 ]
    [ "$output" = "a b c" ]
}

@test "ifs: IFS with default parameter expansion" {
    run $COLONY -c 'IFS=:; echo ${undefined:-a:b:c}'
    [ "$status" -eq 0 ]
    [ "$output" = "a b c" ]
}

# ============================================================================
# IFS in Different Contexts
# ============================================================================

@test "ifs: IFS in array context" {
    run $COLONY -c 'IFS=:; arr=(a:b:c); echo ${#arr[@]}; echo ${arr[0]}'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "1" ]
    [ "${lines[1]}" = "a b c" ]
}

@test "ifs: IFS with read builtin" {
    run $COLONY -c 'IFS=:; echo "a:b:c" | read x y z; echo $x $y $z'
    [ "$status" -eq 0 ]
    [ "$output" = "" ]
}

# ============================================================================
# IFS and Glob Patterns
# ============================================================================

@test "ifs: IFS doesn't affect glob expansion" {
    run $COLONY -c 'IFS=:; echo *' 
    [ "$status" -eq 0 ]
    # Output should be space-separated filenames, not colon-separated
    [[ "$output" =~ " " ]]
}

# ============================================================================
# Comparison with Bash
# ============================================================================

@test "compare: default IFS with bash" {
    run $COLONY -c 'var="a b c"; for i in $var; do echo $i; done'
    colony_output="$output"
    run bash -c 'var="a b c"; for i in $var; do echo $i; done'
    [ "$colony_output" = "$output" ]
}

@test "compare: custom IFS with bash" {
    run $COLONY -c 'IFS=:; var="a:b:c"; for i in $var; do echo $i; done'
    colony_output="$output"
    run bash -c 'IFS=:; var="a:b:c"; for i in $var; do echo $i; done'
    [ "$colony_output" = "$output" ]
}

@test "compare: empty fields with bash" {
    run $COLONY -c 'IFS=:; var="a::c"; for i in $var; do echo "[$i]"; done'
    colony_output="$output"
    run bash -c 'IFS=:; var="a::c"; for i in $var; do echo "[$i]"; done'
    [ "$colony_output" = "$output" ]
}

@test "compare: quoted variable with bash" {
    run $COLONY -c 'IFS=:; var="a:b:c"; echo "$var"'
    colony_output="$output"
    run bash -c 'IFS=:; var="a:b:c"; echo "$var"'
    [ "$colony_output" = "$output" ]
}
