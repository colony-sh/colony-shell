#!/usr/bin/env bats
# Test operator precedence in arithmetic and logical expressions
# Based on bash's precedence.tests

COLONY="${COLONY_BIN:-../../target/release/colony}"

# =============================================================================
# Arithmetic operator precedence
# =============================================================================

@test "precedence: multiplication before addition" {
    run "$COLONY" -c 'echo $((2 + 3 * 4))'
    [ "$status" -eq 0 ]
    [ "$output" = "14" ]
}

@test "precedence: division before subtraction" {
    run "$COLONY" -c 'echo $((20 - 12 / 4))'
    [ "$status" -eq 0 ]
    [ "$output" = "17" ]
}

@test "precedence: parentheses override" {
    run "$COLONY" -c 'echo $(((2 + 3) * 4))'
    [ "$status" -eq 0 ]
    [ "$output" = "20" ]
}

@test "precedence: modulo same as multiply/divide" {
    run "$COLONY" -c 'echo $((10 + 15 % 4))'
    [ "$status" -eq 0 ]
    [ "$output" = "13" ]
}

@test "precedence: exponentiation before multiply" {
    run "$COLONY" -c 'echo $((2 * 3 ** 2))'
    [ "$status" -eq 0 ]
    [ "$output" = "18" ]
}

@test "precedence: exponentiation right associative" {
    run "$COLONY" -c 'echo $((2 ** 3 ** 2))'
    [ "$status" -eq 0 ]
    [ "$output" = "512" ]
}

# =============================================================================
# Unary operators
# =============================================================================

@test "precedence: unary minus before multiply" {
    run "$COLONY" -c 'echo $((-3 * 2))'
    [ "$status" -eq 0 ]
    [ "$output" = "-6" ]
}

@test "precedence: unary plus" {
    run "$COLONY" -c 'echo $((+5 + 3))'
    [ "$status" -eq 0 ]
    [ "$output" = "8" ]
}

@test "precedence: logical not" {
    run "$COLONY" -c 'echo $((!0))'
    [ "$status" -eq 0 ]
    [ "$output" = "1" ]
}

@test "precedence: bitwise not" {
    run "$COLONY" -c 'echo $((~0))'
    [ "$status" -eq 0 ]
    [ "$output" = "-1" ]
}

# =============================================================================
# Comparison operators
# =============================================================================

@test "precedence: comparison before logical and" {
    run "$COLONY" -c 'echo $((1 < 2 && 3 < 4))'
    [ "$status" -eq 0 ]
    [ "$output" = "1" ]
}

@test "precedence: equality before logical or" {
    run "$COLONY" -c 'echo $((1 == 1 || 2 == 3))'
    [ "$status" -eq 0 ]
    [ "$output" = "1" ]
}

@test "precedence: less than" {
    run "$COLONY" -c 'echo $((3 < 5))'
    [ "$status" -eq 0 ]
    [ "$output" = "1" ]
}

@test "precedence: greater than" {
    run "$COLONY" -c 'echo $((5 > 3))'
    [ "$status" -eq 0 ]
    [ "$output" = "1" ]
}

@test "precedence: less than or equal" {
    run "$COLONY" -c 'echo $((3 <= 3))'
    [ "$status" -eq 0 ]
    [ "$output" = "1" ]
}

@test "precedence: greater than or equal" {
    run "$COLONY" -c 'echo $((3 >= 3))'
    [ "$status" -eq 0 ]
    [ "$output" = "1" ]
}

@test "precedence: equality" {
    run "$COLONY" -c 'echo $((5 == 5))'
    [ "$status" -eq 0 ]
    [ "$output" = "1" ]
}

@test "precedence: inequality" {
    run "$COLONY" -c 'echo $((5 != 3))'
    [ "$status" -eq 0 ]
    [ "$output" = "1" ]
}

# =============================================================================
# Bitwise operators
# =============================================================================

@test "precedence: bitwise and before bitwise or" {
    run "$COLONY" -c 'echo $((1 | 2 & 3))'
    [ "$status" -eq 0 ]
    [ "$output" = "3" ]
}

@test "precedence: bitwise xor between and and or" {
    run "$COLONY" -c 'echo $((1 | 2 ^ 3))'
    [ "$status" -eq 0 ]
    [ "$output" = "1" ]
}

@test "precedence: shift before comparison" {
    run "$COLONY" -c 'echo $((1 << 2 < 10))'
    [ "$status" -eq 0 ]
    [ "$output" = "1" ]
}

@test "precedence: left shift" {
    run "$COLONY" -c 'echo $((1 << 4))'
    [ "$status" -eq 0 ]
    [ "$output" = "16" ]
}

@test "precedence: right shift" {
    run "$COLONY" -c 'echo $((16 >> 2))'
    [ "$status" -eq 0 ]
    [ "$output" = "4" ]
}

# =============================================================================
# Logical operators
# =============================================================================

@test "precedence: logical and before logical or" {
    run "$COLONY" -c 'echo $((0 || 1 && 1))'
    [ "$status" -eq 0 ]
    [ "$output" = "1" ]
}

@test "precedence: logical and short circuit" {
    run "$COLONY" -c 'x=5; echo $((0 && (x=10))); echo $x'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "0" ]
    [ "${lines[1]}" = "5" ]
}

@test "precedence: logical or short circuit" {
    run "$COLONY" -c 'x=5; echo $((1 || (x=10))); echo $x'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "1" ]
    [ "${lines[1]}" = "5" ]
}

# =============================================================================
# Ternary operator
# =============================================================================

@test "precedence: ternary operator" {
    run "$COLONY" -c 'echo $((1 ? 2 : 3))'
    [ "$status" -eq 0 ]
    [ "$output" = "2" ]
}

@test "precedence: ternary false branch" {
    run "$COLONY" -c 'echo $((0 ? 2 : 3))'
    [ "$status" -eq 0 ]
    [ "$output" = "3" ]
}

@test "precedence: ternary with expressions" {
    run "$COLONY" -c 'echo $((5 > 3 ? 10 : 20))'
    [ "$status" -eq 0 ]
    [ "$output" = "10" ]
}

@test "precedence: nested ternary" {
    run "$COLONY" -c 'echo $((1 ? 2 ? 3 : 4 : 5))'
    [ "$status" -eq 0 ]
    [ "$output" = "3" ]
}

# =============================================================================
# Assignment operators
# =============================================================================

@test "precedence: assignment lowest precedence" {
    run "$COLONY" -c 'echo $((x = 2 + 3)); echo $x'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "5" ]
    [ "${lines[1]}" = "5" ]
}

@test "precedence: compound assignment +=" {
    run "$COLONY" -c 'x=5; echo $((x += 3)); echo $x'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "8" ]
    [ "${lines[1]}" = "8" ]
}

@test "precedence: compound assignment -=" {
    run "$COLONY" -c 'x=5; echo $((x -= 3)); echo $x'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "2" ]
    [ "${lines[1]}" = "2" ]
}

@test "precedence: compound assignment *=" {
    run "$COLONY" -c 'x=5; echo $((x *= 3)); echo $x'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "15" ]
    [ "${lines[1]}" = "15" ]
}

@test "precedence: compound assignment /=" {
    run "$COLONY" -c 'x=15; echo $((x /= 3)); echo $x'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "5" ]
    [ "${lines[1]}" = "5" ]
}

@test "precedence: compound assignment %=" {
    run "$COLONY" -c 'x=17; echo $((x %= 5)); echo $x'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "2" ]
    [ "${lines[1]}" = "2" ]
}

# =============================================================================
# Increment/Decrement operators
# =============================================================================

@test "precedence: pre-increment" {
    run "$COLONY" -c 'x=5; echo $((++x)); echo $x'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "6" ]
    [ "${lines[1]}" = "6" ]
}

@test "precedence: post-increment" {
    run "$COLONY" -c 'x=5; echo $((x++)); echo $x'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "5" ]
    [ "${lines[1]}" = "6" ]
}

@test "precedence: pre-decrement" {
    run "$COLONY" -c 'x=5; echo $((--x)); echo $x'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "4" ]
    [ "${lines[1]}" = "4" ]
}

@test "precedence: post-decrement" {
    run "$COLONY" -c 'x=5; echo $((x--)); echo $x'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "5" ]
    [ "${lines[1]}" = "4" ]
}

# =============================================================================
# Comma operator
# =============================================================================

@test "precedence: comma operator" {
    run "$COLONY" -c 'echo $((1, 2, 3))'
    [ "$status" -eq 0 ]
    [ "$output" = "3" ]
}

@test "precedence: comma with assignments" {
    run "$COLONY" -c 'echo $((x=1, y=2, x+y))'
    [ "$status" -eq 0 ]
    [ "$output" = "3" ]
}

# =============================================================================
# Complex expressions
# =============================================================================

@test "precedence: complex expression 1" {
    run "$COLONY" -c 'echo $((2 + 3 * 4 - 5 / 2))'
    [ "$status" -eq 0 ]
    [ "$output" = "12" ]
}

@test "precedence: complex expression 2" {
    run "$COLONY" -c 'echo $((1 + 2 << 3))'
    [ "$status" -eq 0 ]
    [ "$output" = "24" ]
}

@test "precedence: complex expression 3" {
    run "$COLONY" -c 'echo $((5 < 3 + 4))'
    [ "$status" -eq 0 ]
    [ "$output" = "1" ]
}

@test "precedence: complex expression 4" {
    run "$COLONY" -c 'echo $((1 && 2 || 0 && 3))'
    [ "$status" -eq 0 ]
    [ "$output" = "1" ]
}

# =============================================================================
# Command list precedence (&&, ||)
# =============================================================================

@test "precedence: command && before ||" {
    run "$COLONY" -c 'false && echo a || echo b'
    [ "$status" -eq 0 ]
    [ "$output" = "b" ]
}

@test "precedence: command || after &&" {
    run "$COLONY" -c 'true && echo a || echo b'
    [ "$status" -eq 0 ]
    [ "$output" = "a" ]
}

@test "precedence: multiple && chained" {
    run "$COLONY" -c 'true && true && echo yes'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

@test "precedence: multiple || chained" {
    run "$COLONY" -c 'false || false || echo yes'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

# =============================================================================
# Compare with bash
# =============================================================================

@test "compare precedence with bash: multiply add" {
    colony_out=$("$COLONY" -c 'echo $((2 + 3 * 4))' 2>&1)
    bash_out=$(bash -c 'echo $((2 + 3 * 4))' 2>&1)
    [ "$colony_out" = "$bash_out" ]
}

@test "compare precedence with bash: exponent" {
    colony_out=$("$COLONY" -c 'echo $((2 ** 3 ** 2))' 2>&1)
    bash_out=$(bash -c 'echo $((2 ** 3 ** 2))' 2>&1)
    [ "$colony_out" = "$bash_out" ]
}

@test "compare precedence with bash: ternary" {
    colony_out=$("$COLONY" -c 'echo $((5 > 3 ? 10 : 20))' 2>&1)
    bash_out=$(bash -c 'echo $((5 > 3 ? 10 : 20))' 2>&1)
    [ "$colony_out" = "$bash_out" ]
}

@test "compare precedence with bash: logical" {
    colony_out=$("$COLONY" -c 'echo $((1 || 0 && 0))' 2>&1)
    bash_out=$(bash -c 'echo $((1 || 0 && 0))' 2>&1)
    [ "$colony_out" = "$bash_out" ]
}
