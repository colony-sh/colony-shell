#!/usr/bin/env bats
# Test Colony shell comment handling for POSIX compliance

COLONY="${COLONY_BIN}"

@test "comment: simple line comment" {
    run "$COLONY" -c '# this is a comment'
    [ "$status" -eq 0 ]
    [ -z "$output" ]
}

@test "comment: shebang line" {
    run "$COLONY" -c '#!/bin/bash'
    [ "$status" -eq 0 ]
    [ -z "$output" ]
}

@test "comment: shebang followed by command" {
    run "$COLONY" -c '#!/bin/bash
echo hello'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "comment: inline comment after command" {
    run "$COLONY" -c 'echo hello # this is a comment'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "comment: # in middle of word is not a comment" {
    run "$COLONY" -c 'echo foo#bar'
    [ "$status" -eq 0 ]
    [ "$output" = "foo#bar" ]
}

@test "comment: quoted # is not a comment" {
    run "$COLONY" -c 'echo "#hello"'
    [ "$status" -eq 0 ]
    [ "$output" = "#hello" ]
}

@test "comment: single quoted # is not a comment" {
    run "$COLONY" -c "echo '#hello'"
    [ "$status" -eq 0 ]
    [ "$output" = "#hello" ]
}

@test "comment: multiple comments in script" {
    run "$COLONY" -c '
# First comment
echo line1
# Second comment  
echo line2
#!/bin/bash - embedded shebang
echo line3
'
    [ "$status" -eq 0 ]
    [[ "$output" == *"line1"* ]]
    [[ "$output" == *"line2"* ]]
    [[ "$output" == *"line3"* ]]
}

@test "comment: $# variable still works" {
    run "$COLONY" -c 'echo $#'
    [ "$status" -eq 0 ]
    [ "$output" = "0" ]
}

@test "compare comment with bash: inline" {
    colony_out=$("$COLONY" -c 'echo hello # comment' 2>&1)
    bash_out=$(bash -c 'echo hello # comment' 2>&1)
    [ "$colony_out" = "$bash_out" ]
}

@test "compare comment with bash: # in middle of word" {
    colony_out=$("$COLONY" -c 'echo foo#bar' 2>&1)
    bash_out=$(bash -c 'echo foo#bar' 2>&1)
    [ "$colony_out" = "$bash_out" ]
}
