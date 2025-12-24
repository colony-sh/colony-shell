#!/usr/bin/env bats

# Programmable Completion Tests (complete, compgen)
# Tests the bash-compatible programmable completion builtins

COLONY="${COLONY_BIN}"

# =============================================================================
# complete builtin - basic functionality
# =============================================================================

@test "complete: builtin exists" {
    run "$COLONY" -c 'complete -W "foo bar" cmd1; complete'
    [ "$status" -eq 0 ]
    [[ "$output" =~ "cmd1" ]]
}

@test "complete -W: word list completion" {
    run "$COLONY" -c 'complete -W "alpha beta gamma" testcmd; complete -p testcmd'
    [ "$status" -eq 0 ]
    [[ "$output" =~ "-W" ]]
}

@test "complete -F: function completion" {
    run "$COLONY" -c 'complete -F _myfunc mycmd; complete -p mycmd'
    [ "$status" -eq 0 ]
}

@test "complete -f: file completion" {
    run "$COLONY" -c 'complete -f filecmd; complete -p filecmd'
    [ "$status" -eq 0 ]
}

@test "complete -d: directory completion" {
    run "$COLONY" -c 'complete -d dircmd; complete -p dircmd'
    [ "$status" -eq 0 ]
}

# =============================================================================
# compgen builtin
# =============================================================================

@test "compgen -W: generate word completions" {
    run "$COLONY" -c 'compgen -W "alpha beta gamma" -- al'
    [ "$status" -eq 0 ]
    [ "$output" = "alpha" ]
}

@test "compgen -f: generate file completions" {
    run "$COLONY" -c 'cd /tmp && compgen -f -- /'
    [ "$status" -eq 0 ]
}

@test "compgen -d: generate directory completions" {
    run "$COLONY" -c 'cd /tmp && compgen -d -- /'
    [ "$status" -eq 0 ]
}

@test "compgen -c: generate command completions" {
    run "$COLONY" -c 'compgen -c -- ec'
    [ "$status" -eq 0 ]
    [[ "$output" =~ "echo" ]]
}

@test "compgen -v: generate variable completions" {
    run "$COLONY" -c 'export MYVAR=test; compgen -v -- MY'
    [ "$status" -eq 0 ]
    [[ "$output" =~ "MYVAR" ]]
}
