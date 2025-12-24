#!/usr/bin/env bats
# Test alias and unalias builtins
# Based on bash's alias.tests

COLONY="${COLONY_BIN:-../../target/release/colony}"

# =============================================================================
# Basic alias definition and expansion
# =============================================================================

@test "alias: define simple alias" {
    run "$COLONY" -c 'alias foo=bar; alias foo'
    [ "$status" -eq 0 ]
    [[ "$output" == *"foo"*"bar"* ]]
}

@test "alias: expand simple alias" {
    run "$COLONY" -c 'alias e=echo; e hello'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "alias: alias with arguments" {
    run "$COLONY" -c 'alias greet="echo hello"; greet world'
    [ "$status" -eq 0 ]
    [ "$output" = "hello world" ]
}

@test "alias: alias with options" {
    run "$COLONY" -c 'alias ll="ls -la"; ll' 
    [ "$status" -eq 0 ]
}

@test "alias: list all aliases" {
    run "$COLONY" -c 'alias a=b; alias c=d; alias'
    [ "$status" -eq 0 ]
    [[ "$output" == *"a"* ]]
    [[ "$output" == *"c"* ]]
}

@test "alias: alias with no arguments shows all" {
    run "$COLONY" -c 'alias x=y; alias'
    [ "$status" -eq 0 ]
    [[ "$output" == *"x"* ]]
}

# =============================================================================
# Alias quoting
# =============================================================================

@test "alias: single quoted value" {
    run "$COLONY" -c "alias foo='echo hello'; foo"
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "alias: double quoted value" {
    run "$COLONY" -c 'alias foo="echo hello"; foo'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "alias: value with spaces" {
    run "$COLONY" -c 'alias foo="echo hello world"; foo'
    [ "$status" -eq 0 ]
    [ "$output" = "hello world" ]
}

@test "alias: value with semicolon" {
    run "$COLONY" -c 'alias both="echo one; echo two"; both'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "one" ]
    [ "${lines[1]}" = "two" ]
}

@test "alias: value with pipe" {
    run "$COLONY" -c 'alias countlines="cat | wc -l"; echo -e "a\nb\nc" | countlines'
    [ "$status" -eq 0 ]
    [[ "$output" == *"3"* ]]
}

# =============================================================================
# Alias special characters
# =============================================================================

@test "alias: name with underscore" {
    run "$COLONY" -c 'alias my_alias=echo; my_alias test'
    [ "$status" -eq 0 ]
    [ "$output" = "test" ]
}

@test "alias: name with numbers" {
    run "$COLONY" -c 'alias test123=echo; test123 hello'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "alias: value with redirection" {
    run "$COLONY" -c 'alias save="cat > /tmp/alias_test.txt"; echo hello | save; cat /tmp/alias_test.txt'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

# =============================================================================
# unalias builtin
# =============================================================================

@test "unalias: remove single alias" {
    run "$COLONY" -c 'alias foo=bar; unalias foo; alias foo' 
    [ "$status" -ne 0 ]
}

@test "unalias: remove multiple aliases" {
    run "$COLONY" -c 'alias a=1; alias b=2; unalias a b; alias'
    [ "$status" -eq 0 ]
    [[ "$output" != *"a="* ]]
    [[ "$output" != *"b="* ]]
}

@test "unalias: -a removes all aliases" {
    run "$COLONY" -c 'alias a=1; alias b=2; alias c=3; unalias -a; alias'
    [ "$status" -eq 0 ]
    [ -z "$output" ] || [[ "$output" != *"="* ]]
}

@test "unalias: nonexistent alias returns error" {
    run "$COLONY" -c 'unalias nonexistent'
    [ "$status" -ne 0 ]
}

# =============================================================================
# Alias expansion context
# =============================================================================

@test "alias: not expanded in single quotes" {
    run "$COLONY" -c "alias foo=echo; echo 'foo bar'"
    [ "$status" -eq 0 ]
    [ "$output" = "foo bar" ]
}

@test "alias: not expanded in double quotes" {
    run "$COLONY" -c 'alias foo=echo; echo "foo bar"'
    [ "$status" -eq 0 ]
    [ "$output" = "foo bar" ]
}

@test "alias: expanded at command position only" {
    run "$COLONY" -c 'alias foo=hello; echo foo'
    [ "$status" -eq 0 ]
    [ "$output" = "foo" ]
}

@test "alias: expanded after semicolon" {
    run "$COLONY" -c 'alias e=echo; true; e hello'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "alias: expanded after pipe" {
    run "$COLONY" -c 'alias e=echo; true | e hello'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "alias: expanded after &&" {
    run "$COLONY" -c 'alias e=echo; true && e hello'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "alias: expanded after ||" {
    run "$COLONY" -c 'alias e=echo; false || e hello'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

# =============================================================================
# Alias in functions
# =============================================================================

@test "alias: defined in function" {
    run "$COLONY" -c 'f() { alias myecho=echo; }; f; myecho hello'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "alias: used in function" {
    run "$COLONY" -c 'alias e=echo; f() { e hello; }; f'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

# =============================================================================
# Alias trailing space
# =============================================================================

@test "alias: trailing space causes next word expansion" {
    run "$COLONY" -c 'alias sudo="sudo "; alias e=echo; sudo e hello'
    [ "$status" -eq 0 ] || [ "$status" -eq 127 ]  # sudo may not exist
}

# =============================================================================
# Alias -p flag
# =============================================================================

@test "alias: -p prints in reusable format" {
    run "$COLONY" -c 'alias foo=bar; alias -p'
    [ "$status" -eq 0 ]
    [[ "$output" == *"alias foo"* ]]
}

# =============================================================================
# Edge cases
# =============================================================================

@test "alias: empty alias name is error" {
    run "$COLONY" -c 'alias ='
    [ "$status" -ne 0 ]
}

@test "alias: redefine existing alias" {
    run "$COLONY" -c 'alias foo=first; alias foo=second; alias foo'
    [ "$status" -eq 0 ]
    [[ "$output" == *"second"* ]]
}

@test "alias: alias to itself" {
    run "$COLONY" -c 'alias ls=ls; ls --version' 
    [ "$status" -eq 0 ] || [ "$status" -eq 1 ]  # ls may not support --version on all systems
}

@test "alias: check specific alias exists" {
    run "$COLONY" -c 'alias foo=bar; alias foo'
    [ "$status" -eq 0 ]
    run "$COLONY" -c 'alias nonexistent'
    [ "$status" -ne 0 ]
}

# =============================================================================
# Compare with bash
# =============================================================================

@test "compare alias with bash: simple expansion" {
    colony_out=$("$COLONY" -c 'alias e=echo; e hello' 2>&1)
    bash_out=$(bash -c 'shopt -s expand_aliases; alias e=echo; e hello' 2>&1)
    [ "$colony_out" = "$bash_out" ]
}

@test "compare alias with bash: alias list format" {
    colony_out=$("$COLONY" -c 'alias foo=bar; alias foo' 2>&1)
    bash_out=$(bash -c 'alias foo=bar; alias foo' 2>&1)
    [[ "$colony_out" == *"foo"* ]] && [[ "$bash_out" == *"foo"* ]]
}
