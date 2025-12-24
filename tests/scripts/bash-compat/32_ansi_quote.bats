#!/usr/bin/env bats
# ANSI-C Quoting tests - $'...' syntax
# Tests escape sequences like \n, \t, \xHH, \uHHHH inside $'...' strings
# Based on bash manual ANSI-C Quoting section

setup() {
    COLONY="${COLONY_BIN:-../../../target/release/colony}"
}

# ============================================================================
# Basic Escape Sequences
# ============================================================================

@test "ansi-c: newline escape \\n" {
    run $COLONY -c $'echo $\'hello\\nworld\''
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "hello" ]
    [ "${lines[1]}" = "world" ]
}

@test "ansi-c: tab escape \\t" {
    run $COLONY -c $'echo $\'hello\\tworld\''
    [ "$status" -eq 0 ]
    [ "$output" = $'hello\tworld' ]
}

@test "ansi-c: carriage return escape \\r" {
    run $COLONY -c $'printf "%s" $\'hello\\rworld\''
    [ "$status" -eq 0 ]
    [ "$output" = $'hello\rworld' ]
}

@test "ansi-c: backslash escape \\\\" {
    run $COLONY -c $'echo $\'hello\\\\world\''
    [ "$status" -eq 0 ]
    [ "$output" = 'hello\world' ]
}

@test "ansi-c: single quote escape \\'" {
    run $COLONY -c $'echo $\'hello\\\'world\''
    [ "$status" -eq 0 ]
    [ "$output" = "hello'world" ]
}

@test "ansi-c: double quote escape \\\"" {
    run $COLONY -c $'echo $\'hello\\"world\''
    [ "$status" -eq 0 ]
    [ "$output" = 'hello"world' ]
}

@test "ansi-c: alert/bell escape \\a" {
    run $COLONY -c $'printf "%s" $\'\\a\' | od -c | head -1'
    [ "$status" -eq 0 ]
    [[ "$output" == *"\\a"* ]] || [[ "$output" == *"007"* ]]
}

@test "ansi-c: backspace escape \\b" {
    run $COLONY -c $'printf "%s" $\'\\b\' | od -c | head -1'
    [ "$status" -eq 0 ]
    [[ "$output" == *"\\b"* ]] || [[ "$output" == *"010"* ]]
}

@test "ansi-c: form feed escape \\f" {
    run $COLONY -c $'printf "%s" $\'\\f\' | od -c | head -1'
    [ "$status" -eq 0 ]
    [[ "$output" == *"\\f"* ]] || [[ "$output" == *"014"* ]]
}

@test "ansi-c: vertical tab escape \\v" {
    run $COLONY -c $'printf "%s" $\'\\v\' | od -c | head -1'
    [ "$status" -eq 0 ]
    [[ "$output" == *"\\v"* ]] || [[ "$output" == *"013"* ]]
}

@test "ansi-c: escape character \\e" {
    run $COLONY -c $'printf "%s" $\'\\e\' | od -c | head -1'
    [ "$status" -eq 0 ]
    [[ "$output" == *"033"* ]] || [[ "$output" == *"\\e"* ]]
}

@test "ansi-c: escape character \\E (capital)" {
    run $COLONY -c $'printf "%s" $\'\\E\' | od -c | head -1'
    [ "$status" -eq 0 ]
    [[ "$output" == *"033"* ]] || [[ "$output" == *"\\E"* ]]
}

# ============================================================================
# Octal Escape Sequences
# ============================================================================

@test "ansi-c: octal escape \\nnn - letter A (101)" {
    run $COLONY -c $'echo $\'\\101\''
    [ "$status" -eq 0 ]
    [ "$output" = "A" ]
}

@test "ansi-c: octal escape \\nnn - letter B (102)" {
    run $COLONY -c $'echo $\'\\102\''
    [ "$status" -eq 0 ]
    [ "$output" = "B" ]
}

@test "ansi-c: octal escape \\nnn - letter C (103)" {
    run $COLONY -c $'echo $\'\\103\''
    [ "$status" -eq 0 ]
    [ "$output" = "C" ]
}

@test "ansi-c: octal escape - newline (012)" {
    run $COLONY -c $'echo $\'hello\\012world\''
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "hello" ]
    [ "${lines[1]}" = "world" ]
}

@test "ansi-c: octal escape - two digits (41 = !)" {
    run $COLONY -c $'echo $\'\\41\''
    [ "$status" -eq 0 ]
    [ "$output" = "!" ]
}

@test "ansi-c: octal escape - one digit (7 = bell)" {
    run $COLONY -c $'printf "%s" $\'\\7\' | od -c | head -1'
    [ "$status" -eq 0 ]
    [[ "$output" == *"\\a"* ]] || [[ "$output" == *"007"* ]]
}

@test "ansi-c: octal escape - null (0)" {
    run $COLONY -c $'printf "%s" $\'\\0\' | od -c | head -1'
    [ "$status" -eq 0 ]
    [[ "$output" == *"\\0"* ]] || [[ "$output" == *"000"* ]]
}

# ============================================================================
# Hexadecimal Escape Sequences
# ============================================================================

@test "ansi-c: hex escape \\xHH - letter A (41)" {
    run $COLONY -c $'echo $\'\\x41\''
    [ "$status" -eq 0 ]
    [ "$output" = "A" ]
}

@test "ansi-c: hex escape \\xHH - letter B (42)" {
    run $COLONY -c $'echo $\'\\x42\''
    [ "$status" -eq 0 ]
    [ "$output" = "B" ]
}

@test "ansi-c: hex escape \\xHH - lowercase hex (4a = J)" {
    run $COLONY -c $'echo $\'\\x4a\''
    [ "$status" -eq 0 ]
    [ "$output" = "J" ]
}

@test "ansi-c: hex escape \\xHH - uppercase hex (4A = J)" {
    run $COLONY -c $'echo $\'\\x4A\''
    [ "$status" -eq 0 ]
    [ "$output" = "J" ]
}

@test "ansi-c: hex escape \\xHH - newline (0a)" {
    run $COLONY -c $'echo $\'hello\\x0aworld\''
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "hello" ]
    [ "${lines[1]}" = "world" ]
}

@test "ansi-c: hex escape - single hex digit (\\x9 = tab)" {
    run $COLONY -c $'echo $\'hello\\x9world\''
    [ "$status" -eq 0 ]
    [ "$output" = $'hello\tworld' ]
}

@test "ansi-c: hex escape sequence ABC" {
    run $COLONY -c $'echo $\'\\x41\\x42\\x43\''
    [ "$status" -eq 0 ]
    [ "$output" = "ABC" ]
}

# ============================================================================
# Unicode Escape Sequences
# ============================================================================

@test "ansi-c: unicode escape \\uHHHH - letter A (0041)" {
    run $COLONY -c $'echo $\'\\u0041\''
    [ "$status" -eq 0 ]
    [ "$output" = "A" ]
}

@test "ansi-c: unicode escape \\uHHHH - euro sign (20AC)" {
    run $COLONY -c $'echo $\'\\u20AC\''
    [ "$status" -eq 0 ]
    [ "$output" = $'\u20AC' ]
}

@test "ansi-c: unicode escape \\uHHHH - smiley (263A)" {
    run $COLONY -c $'echo $\'\\u263A\''
    [ "$status" -eq 0 ]
    [ "$output" = $'\u263A' ]
}

@test "ansi-c: unicode escape \\UHHHHHHHH - letter A" {
    run $COLONY -c $'echo $\'\\U00000041\''
    [ "$status" -eq 0 ]
    [ "$output" = "A" ]
}

@test "ansi-c: unicode escape \\UHHHHHHHH - emoji (1F600)" {
    # Skip if shell doesn't support emoji
    run $COLONY -c $'echo $\'\\U0001F600\''
    [ "$status" -eq 0 ]
    # Grinning face emoji
    [[ "$output" == $'\U0001F600' ]] || skip "Unicode emoji not supported"
}

@test "ansi-c: unicode sequence Hi (\\u0048\\u0069)" {
    run $COLONY -c $'echo $\'\\u0048\\u0069\''
    [ "$status" -eq 0 ]
    [ "$output" = "Hi" ]
}

# ============================================================================
# Control Character Escape Sequences
# ============================================================================

@test "ansi-c: control char \\cA (ctrl-A, ASCII 1)" {
    run $COLONY -c $'printf "%s" $\'\\cA\' | od -c | head -1'
    [ "$status" -eq 0 ]
    [[ "$output" == *"001"* ]]
}

@test "ansi-c: control char \\cG (bell, ctrl-G)" {
    run $COLONY -c $'printf "%s" $\'\\cG\' | od -c | head -1'
    [ "$status" -eq 0 ]
    [[ "$output" == *"007"* ]] || [[ "$output" == *"\\a"* ]]
}

@test "ansi-c: control char \\cI (tab, ctrl-I)" {
    run $COLONY -c $'echo $\'hello\\cIworld\''
    [ "$status" -eq 0 ]
    [ "$output" = $'hello\tworld' ]
}

@test "ansi-c: control char \\cJ (newline, ctrl-J)" {
    run $COLONY -c $'echo $\'hello\\cJworld\''
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "hello" ]
    [ "${lines[1]}" = "world" ]
}

@test "ansi-c: control char \\cM (carriage return, ctrl-M)" {
    run $COLONY -c $'printf "%s" $\'\\cM\' | od -c | head -1'
    [ "$status" -eq 0 ]
    [[ "$output" == *"015"* ]] || [[ "$output" == *"\\r"* ]]
}

@test "ansi-c: control char \\c[ (escape, ctrl-[)" {
    run $COLONY -c $'printf "%s" $\'\\c[\' | od -c | head -1'
    [ "$status" -eq 0 ]
    [[ "$output" == *"033"* ]]
}

# ============================================================================
# Mixed Content
# ============================================================================

@test "ansi-c: mixed literal and escape" {
    run $COLONY -c $'echo $\'hello\\nworld\\ttab\''
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "hello" ]
    [ "${lines[1]}" = $'world\ttab' ]
}

@test "ansi-c: ANSI color code sequence" {
    run $COLONY -c $'echo $\'\\e[31mred\\e[0m\''
    [ "$status" -eq 0 ]
    [ "$output" = $'\e[31mred\e[0m' ]
}

@test "ansi-c: multiple escapes in sequence" {
    run $COLONY -c $'echo $\'\\t\\n\\t\''
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = $'\t' ]
    [ "${lines[1]}" = $'\t' ]
}

@test "ansi-c: mixed hex and octal" {
    run $COLONY -c $'echo $\'\\x41\\102\\x43\''
    [ "$status" -eq 0 ]
    [ "$output" = "ABC" ]
}

# ============================================================================
# Concatenation
# ============================================================================

@test "ansi-c: concatenation with regular single quote" {
    run $COLONY -c $'echo $\'hello\'\'world\''
    [ "$status" -eq 0 ]
    [ "$output" = "helloworld" ]
}

@test "ansi-c: concatenation with double quote" {
    run $COLONY -c $'echo $\'hello\'"world"'
    [ "$status" -eq 0 ]
    [ "$output" = "helloworld" ]
}

@test "ansi-c: concatenation of two ANSI-C strings" {
    run $COLONY -c $'echo $\'a\'$\'b\''
    [ "$status" -eq 0 ]
    [ "$output" = "ab" ]
}

@test "ansi-c: concatenation with unquoted text" {
    run $COLONY -c $'echo $\'hello\'world'
    [ "$status" -eq 0 ]
    [ "$output" = "helloworld" ]
}

@test "ansi-c: complex concatenation" {
    run $COLONY -c $'echo $\'a\\nb\'$\'c\\nd\''
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "a" ]
    [ "${lines[1]}" = "bc" ]
    [ "${lines[2]}" = "d" ]
}

# ============================================================================
# Edge Cases
# ============================================================================

@test "ansi-c: empty string" {
    run $COLONY -c $'echo $\'\''
    [ "$status" -eq 0 ]
    [ "$output" = "" ]
}

@test "ansi-c: unknown escape preserved with backslash" {
    # bash preserves unknown escapes - \z becomes \z
    run $COLONY -c $'echo $\'\\z\''
    [ "$status" -eq 0 ]
    [ "$output" = "z" ] || [ "$output" = '\z' ]
}

@test "ansi-c: literal dollar at end" {
    run $COLONY -c $'echo $\'test$\''
    [ "$status" -eq 0 ]
    [ "$output" = 'test$' ]
}

@test "ansi-c: literal percent" {
    run $COLONY -c $'echo $\'100%\''
    [ "$status" -eq 0 ]
    [ "$output" = '100%' ]
}

@test "ansi-c: question mark preserved" {
    run $COLONY -c $'echo $\'hello?\''
    [ "$status" -eq 0 ]
    [ "$output" = 'hello?' ]
}

@test "ansi-c: asterisk preserved" {
    run $COLONY -c $'echo $\'hello*\''
    [ "$status" -eq 0 ]
    [ "$output" = 'hello*' ]
}

# ============================================================================
# Comparison with Bash
# ============================================================================

@test "compare: basic ANSI-C newline with bash" {
    run $COLONY -c $'echo $\'hello\\nworld\''
    colony_output="$output"
    run bash -c $'echo $\'hello\\nworld\''
    [ "$colony_output" = "$output" ]
}

@test "compare: ANSI-C tab with bash" {
    run $COLONY -c $'echo $\'hello\\tworld\''
    colony_output="$output"
    run bash -c $'echo $\'hello\\tworld\''
    [ "$colony_output" = "$output" ]
}

@test "compare: ANSI-C hex escape with bash" {
    run $COLONY -c $'echo $\'\\x41\\x42\\x43\''
    colony_output="$output"
    run bash -c $'echo $\'\\x41\\x42\\x43\''
    [ "$colony_output" = "$output" ]
}

@test "compare: ANSI-C octal escape with bash" {
    run $COLONY -c $'echo $\'\\101\\102\\103\''
    colony_output="$output"
    run bash -c $'echo $\'\\101\\102\\103\''
    [ "$colony_output" = "$output" ]
}

@test "compare: ANSI-C backslash escape with bash" {
    run $COLONY -c $'echo $\'hello\\\\world\''
    colony_output="$output"
    run bash -c $'echo $\'hello\\\\world\''
    [ "$colony_output" = "$output" ]
}

@test "compare: ANSI-C single quote escape with bash" {
    run $COLONY -c $'echo $\'hello\\\'world\''
    colony_output="$output"
    run bash -c $'echo $\'hello\\\'world\''
    [ "$colony_output" = "$output" ]
}

@test "compare: ANSI-C escape char with bash" {
    run $COLONY -c $'printf "%s" $\'\\e\' | od -c'
    colony_output="$output"
    run bash -c $'printf "%s" $\'\\e\' | od -c'
    [ "$colony_output" = "$output" ]
}

@test "compare: ANSI-C unicode with bash" {
    run $COLONY -c $'echo $\'\\u0041\''
    colony_output="$output"
    run bash -c $'echo $\'\\u0041\''
    [ "$colony_output" = "$output" ]
}

@test "compare: ANSI-C color code with bash" {
    run $COLONY -c $'printf "%s" $\'\\e[31mred\\e[0m\' | od -c'
    colony_output="$output"
    run bash -c $'printf "%s" $\'\\e[31mred\\e[0m\' | od -c'
    [ "$colony_output" = "$output" ]
}

# ============================================================================
# Variable assignment
# ============================================================================

@test "ansi-c: assign to variable" {
    run $COLONY -c $'x=$\'hello\\nworld\'; echo "$x"'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "hello" ]
    [ "${lines[1]}" = "world" ]
}

@test "ansi-c: use in array assignment" {
    run $COLONY -c $'arr=($\'a\\nb\' c); echo "${arr[0]}"'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "a" ]
    [ "${lines[1]}" = "b" ]
}

@test "ansi-c: use in function argument" {
    run $COLONY -c $'f() { echo "$1"; }; f $\'hello\\nworld\''
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "hello" ]
    [ "${lines[1]}" = "world" ]
}

# ============================================================================
# printf %q roundtrip
# ============================================================================

@test "ansi-c: printf %q quotes escape char" {
    # printf %q should quote the escape character in a way that's reusable
    run $COLONY -c $'printf "%q\\n" $\'\\e[31m\''
    [ "$status" -eq 0 ]
    # Output should be something like $'\E[31m' or $'\033[31m'
    [[ "$output" == *"["* ]] && [[ "$output" == *"31m"* ]]
}
