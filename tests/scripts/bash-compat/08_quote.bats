#!/usr/bin/env bats
# Quoting tests - based on bash tests/quote.tests
# Tests single quotes, double quotes, backslash escaping, and combinations

setup() {
    COLONY="${COLONY_BIN:-../../../target/release/colony}"
}

# ============================================================================
# Single Quote Tests
# ============================================================================

@test "quote: single quote preserves literal text" {
    run $COLONY -c "echo 'foo
bar'"
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "foo" ]
    [ "${lines[1]}" = "bar" ]
}

@test "quote: single quote preserves multiple newlines" {
    run $COLONY -c "echo 'foo
bar'"
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "foo" ]
    [ "${lines[1]}" = "bar" ]
}

@test "quote: single quote preserves backslash literally" {
    run $COLONY -c "echo 'foo\
bar'"
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'foo\' ]
    [ "${lines[1]}" = "bar" ]
}

# ============================================================================
# Double Quote Tests
# ============================================================================

@test "quote: double quote preserves newlines" {
    run $COLONY -c 'echo "foo
bar"'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "foo" ]
    [ "${lines[1]}" = "bar" ]
}

@test "quote: double quote preserves multiple spaces in newlines" {
    run $COLONY -c 'echo "foo
bar"'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "foo" ]
    [ "${lines[1]}" = "bar" ]
}

@test "quote: double quote with backslash-newline joins lines" {
    run $COLONY -c 'echo "foo\
bar"'
    [ "$status" -eq 0 ]
    [ "$output" = "foobar" ]
}

# ============================================================================
# Backquote Command Substitution with Single Quotes
# ============================================================================

@test "quote: backquote with single quote newline collapses to space" {
    run $COLONY -c "echo \`echo 'foo
bar'\`"
    [ "$status" -eq 0 ]
    [ "$output" = "foo bar" ]
}

@test "quote: backquote with single quote preserves multiple spaces" {
    run $COLONY -c "echo \`echo 'foo  
bar'\`"
    [ "$status" -eq 0 ]
    [ "$output" = "foo bar" ]
}

@test "quote: backquote with single quote and backslash joins" {
    run $COLONY -c "echo \`echo 'foo\
bar'\`"
    [ "$status" -eq 0 ]
    [ "$output" = "foobar" ]
}

# ============================================================================
# Backquote Command Substitution with Double Quotes
# ============================================================================

@test "quote: backquote with double quote newline collapses" {
    run $COLONY -c 'echo `echo "foo
bar"`'
    [ "$status" -eq 0 ]
    [ "$output" = "foo bar" ]
}

@test "quote: backquote with double quote multiple spaces" {
    run $COLONY -c 'echo `echo "foo
bar"`'
    [ "$status" -eq 0 ]
    [ "$output" = "foo bar" ]
}

@test "quote: backquote with double quote backslash-newline" {
    run $COLONY -c 'echo `echo "foo\
bar"`'
    [ "$status" -eq 0 ]
    [ "$output" = "foobar" ]
}

# ============================================================================
# Nested Quotes: Double Quote around Backquote with Single Quote
# ============================================================================

@test "quote: double quote around backquote with single quote preserves newline" {
    run $COLONY -c "echo \"\`echo 'foo
bar'\`\""
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "foo" ]
    [ "${lines[1]}" = "bar" ]
}

@test "quote: double quote around backquote with single quote multiple newlines" {
    run $COLONY -c "echo \"\`echo 'foo
bar'\`\""
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "foo" ]
    [ "${lines[1]}" = "bar" ]
}

@test "quote: double quote around backquote with single quote and backslash" {
    run $COLONY -c "echo \"\`echo 'foo\
bar'\`\""
    [ "$status" -eq 0 ]
    [ "$output" = "foobar" ]
}

# ============================================================================
# Dollar Paren Command Substitution with Single Quotes
# ============================================================================

@test "quote: dollar paren with single quote newline collapses" {
    run $COLONY -c "echo \$(echo 'foo
bar')"
    [ "$status" -eq 0 ]
    [ "$output" = "foo bar" ]
}

@test "quote: dollar paren with single quote multiple spaces" {
    run $COLONY -c "echo \$(echo 'foo
bar')"
    [ "$status" -eq 0 ]
    [ "$output" = "foo bar" ]
}

@test "quote: dollar paren with single quote and backslash" {
    run $COLONY -c "echo \$(echo 'foo\
bar')"
    [ "$status" -eq 0 ]
    [ "$output" = 'foo\ bar' ]
}

# ============================================================================
# Dollar Paren Command Substitution with Double Quotes
# ============================================================================

@test "quote: dollar paren with double quote newline collapses" {
    run $COLONY -c 'echo $(echo "foo
bar")'
    [ "$status" -eq 0 ]
    [ "$output" = "foo bar" ]
}

@test "quote: dollar paren with double quote multiple spaces" {
    run $COLONY -c 'echo $(echo "foo
bar")'
    [ "$status" -eq 0 ]
    [ "$output" = "foo bar" ]
}

@test "quote: dollar paren with double quote backslash-newline" {
    run $COLONY -c 'echo $(echo "foo\
bar")'
    [ "$status" -eq 0 ]
    [ "$output" = "foobar" ]
}

# ============================================================================
# Nested: Double Quote around Dollar Paren with Single Quote
# ============================================================================

@test "quote: double quote around dollar paren with single quote preserves newline" {
    run $COLONY -c "echo \"\$(echo 'foo
bar')\""
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "foo" ]
    [ "${lines[1]}" = "bar" ]
}

@test "quote: double quote around dollar paren with single quote multiple newlines" {
    run $COLONY -c "echo \"\$(echo 'foo
bar')\""
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "foo" ]
    [ "${lines[1]}" = "bar" ]
}

@test "quote: double quote around dollar paren with single quote and backslash" {
    run $COLONY -c "echo \"\$(echo 'foo\
bar')\""
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'foo\' ]
    [ "${lines[1]}" = "bar" ]
}

# ============================================================================
# Backslash Escaping - Strings Ending in Backslashes
# ============================================================================

@test "quote: single quote preserves trailing backslash" {
    run $COLONY -c "echo 'string \'"
    [ "$status" -eq 0 ]
    [ "$output" = 'string \' ]
}

@test "quote: double quote preserves trailing backslash" {
    run $COLONY -c 'echo "string \\"'
    [ "$status" -eq 0 ]
    [ "$output" = 'string \' ]
}

@test "quote: unquoted trailing backslash" {
    run $COLONY -c 'echo string\ \\'
    [ "$status" -eq 0 ]
    [ "$output" = 'string \' ]
}

@test "quote: parameter expansion with single quote and trailing backslash" {
    run $COLONY -c "echo \${foo:-'string \'}"
    [ "$status" -eq 0 ]
    [ "$output" = 'string \' ]
}

@test "quote: parameter expansion with double quote and trailing backslash" {
    run $COLONY -c 'echo "${foo:-string \\}"'
    [ "$status" -eq 0 ]
    [ "$output" = 'string \' ]
}

@test "quote: parameter expansion with backslash and brace" {
    run $COLONY -c 'echo ${foo:-string \\\}}'
    [ "$status" -eq 0 ]
    [ "$output" = 'string \}' ]
}

# ============================================================================
# Special Characters in Quotes
# ============================================================================

@test "quote: backquote inside double quotes" {
    run $COLONY -c 'echo "${foo:-`echo bar`}"'
    [ "$status" -eq 0 ]
    [ "$output" = "bar" ]
}

@test "quote: dollar sign in single quotes stays literal" {
    run $COLONY -c "echo '\$foo'"
    [ "$status" -eq 0 ]
    [ "$output" = '$foo' ]
}

@test "quote: dollar sign in double quotes allows expansion" {
    run $COLONY -c 'foo=bar; echo "$foo"'
    [ "$status" -eq 0 ]
    [ "$output" = "bar" ]
}

@test "quote: escaped dollar in double quotes stays literal" {
    run $COLONY -c 'echo "\$foo"'
    [ "$status" -eq 0 ]
    [ "$output" = '$foo' ]
}

# ============================================================================
# Empty Strings and Whitespace
# ============================================================================

@test "quote: empty single quotes" {
    run $COLONY -c "echo ''"
    [ "$status" -eq 0 ]
    [ "$output" = "" ]
}

@test "quote: empty double quotes" {
    run $COLONY -c 'echo ""'
    [ "$status" -eq 0 ]
    [ "$output" = "" ]
}

@test "quote: single space in single quotes" {
    run $COLONY -c "echo ' '"
    [ "$status" -eq 0 ]
    [ "$output" = " " ]
}

@test "quote: single space in double quotes" {
    run $COLONY -c 'echo " "'
    [ "$status" -eq 0 ]
    [ "$output" = " " ]
}

@test "quote: multiple spaces in single quotes preserved" {
    run $COLONY -c "echo 'foo   bar'"
    [ "$status" -eq 0 ]
    [ "$output" = "foo   bar" ]
}

@test "quote: multiple spaces in double quotes preserved" {
    run $COLONY -c 'echo "foo   bar"'
    [ "$status" -eq 0 ]
    [ "$output" = "foo   bar" ]
}

# ============================================================================
# Quote Concatenation
# ============================================================================

@test "quote: adjacent single and double quotes" {
    run $COLONY -c "echo 'foo'\"bar\""
    [ "$status" -eq 0 ]
    [ "$output" = "foobar" ]
}

@test "quote: single quote with unquoted text" {
    run $COLONY -c "echo foo'bar'baz"
    [ "$status" -eq 0 ]
    [ "$output" = "foobarbaz" ]
}

@test "quote: double quote with unquoted text" {
    run $COLONY -c 'echo foo"bar"baz'
    [ "$status" -eq 0 ]
    [ "$output" = "foobarbaz" ]
}

@test "quote: alternating quotes" {
    run $COLONY -c "echo 'a'\"b\"'c'"
    [ "$status" -eq 0 ]
    [ "$output" = "abc" ]
}

# ============================================================================
# Edge Cases
# ============================================================================

@test "quote: escaped single quote outside quotes" {
    run $COLONY -c "echo test\\'ing"
    [ "$status" -eq 0 ]
    [ "$output" = "test'ing" ]
}

@test "quote: backslash before double quote in double quotes" {
    run $COLONY -c 'echo "test\"ing"'
    [ "$status" -eq 0 ]
    [ "$output" = 'test"ing' ]
}

@test "quote: backslash before dollar in double quotes" {
    run $COLONY -c 'foo=bar; echo "test\$foo"'
    [ "$status" -eq 0 ]
    [ "$output" = 'test$foo' ]
}

@test "quote: backslash before backslash in double quotes" {
    run $COLONY -c 'echo "test\\ing"'
    [ "$status" -eq 0 ]
    [ "$output" = 'test\ing' ]
}

@test "quote: backslash before other char in double quotes preserved" {
    run $COLONY -c 'echo "test\ning"'
    [ "$status" -eq 0 ]
    [ "$output" = 'test\ning' ]
}

# ============================================================================
# Comparison with Bash
# ============================================================================

@test "compare: basic single quote with bash" {
    run $COLONY -c "echo 'hello world'"
    colony_output="$output"
    run bash -c "echo 'hello world'"
    [ "$colony_output" = "$output" ]
}

@test "compare: basic double quote with bash" {
    run $COLONY -c 'echo "hello world"'
    colony_output="$output"
    run bash -c 'echo "hello world"'
    [ "$colony_output" = "$output" ]
}

@test "compare: variable expansion in double quotes with bash" {
    run $COLONY -c 'foo=bar; echo "value: $foo"'
    colony_output="$output"
    run bash -c 'foo=bar; echo "value: $foo"'
    [ "$colony_output" = "$output" ]
}

@test "compare: escaped characters in double quotes with bash" {
    run $COLONY -c 'echo "test\$foo\"bar\\"'
    colony_output="$output"
    run bash -c 'echo "test\$foo\"bar\\"'
    [ "$colony_output" = "$output" ]
}
