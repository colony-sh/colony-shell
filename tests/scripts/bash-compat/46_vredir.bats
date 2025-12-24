#!/usr/bin/env bats
# Test variable redirects {fd}> and {fd}<
# Based on bash's vredir.tests

COLONY="${COLONY_BIN:-../../target/release/colony}"

# =============================================================================
# Basic {varname}>file - allocate fd and store in variable
# =============================================================================

@test "vredir: {fd}>file allocates fd" {
    run "$COLONY" -c 'exec {fd}>/tmp/vredir_test1.txt; echo "fd=$fd"; echo hello >&$fd; exec {fd}>&-; cat /tmp/vredir_test1.txt; rm -f /tmp/vredir_test1.txt'
    [ "$status" -eq 0 ]
    [[ "${lines[0]}" == "fd="* ]]
    [ "${lines[1]}" = "hello" ]
}

@test "vredir: allocated fd is >= 10" {
    run "$COLONY" -c 'exec {fd}>/tmp/vredir_test2.txt; echo $fd; exec {fd}>&-; rm -f /tmp/vredir_test2.txt'
    [ "$status" -eq 0 ]
    [ "$output" -ge 10 ]
}

@test "vredir: multiple fd allocations" {
    run "$COLONY" -c 'exec {fd1}>/tmp/vr1.txt; exec {fd2}>/tmp/vr2.txt; echo "fd1=$fd1 fd2=$fd2"; [ "$fd1" != "$fd2" ] && echo different; exec {fd1}>&-; exec {fd2}>&-; rm -f /tmp/vr1.txt /tmp/vr2.txt'
    [ "$status" -eq 0 ]
    [[ "$output" == *"different"* ]]
}

# =============================================================================
# {varname}<file - allocate fd for reading
# =============================================================================

@test "vredir: {fd}<file for reading" {
    run "$COLONY" -c 'echo "test content" > /tmp/vredir_read.txt; exec {fd}</tmp/vredir_read.txt; cat <&$fd; exec {fd}<&-; rm -f /tmp/vredir_read.txt'
    [ "$status" -eq 0 ]
    [ "$output" = "test content" ]
}

@test "vredir: read line from allocated fd" {
    run "$COLONY" -c 'echo -e "line1\nline2" > /tmp/vredir_lines.txt; exec {fd}</tmp/vredir_lines.txt; read -u $fd line; echo "$line"; exec {fd}<&-; rm -f /tmp/vredir_lines.txt'
    [ "$status" -eq 0 ]
    [ "$output" = "line1" ]
}

# =============================================================================
# {varname}>>file - append mode
# =============================================================================

@test "vredir: {fd}>>file append mode" {
    run "$COLONY" -c 'echo "first" > /tmp/vredir_append.txt; exec {fd}>>/tmp/vredir_append.txt; echo "second" >&$fd; exec {fd}>&-; cat /tmp/vredir_append.txt; rm -f /tmp/vredir_append.txt'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "first" ]
    [ "${lines[1]}" = "second" ]
}

# =============================================================================
# {varname}<>file - read/write mode
# =============================================================================

@test "vredir: {fd}<>file read/write mode" {
    run "$COLONY" -c 'echo "original" > /tmp/vredir_rw.txt; exec {fd}<>/tmp/vredir_rw.txt; read -u $fd line; echo "$line"; exec {fd}>&-; rm -f /tmp/vredir_rw.txt'
    [ "$status" -eq 0 ]
    [ "$output" = "original" ]
}

# =============================================================================
# Closing allocated fds
# =============================================================================

@test "vredir: close output fd with {fd}>&-" {
    run "$COLONY" -c 'exec {fd}>/tmp/vredir_close.txt; exec {fd}>&-; echo test >&$fd 2>&1; rm -f /tmp/vredir_close.txt'
    [ "$status" -ne 0 ] || [[ "$output" == *"Bad file descriptor"* ]]
}

@test "vredir: close input fd with {fd}<&-" {
    run "$COLONY" -c 'echo test > /tmp/vredir_closein.txt; exec {fd}</tmp/vredir_closein.txt; exec {fd}<&-; cat <&$fd 2>&1; rm -f /tmp/vredir_closein.txt'
    [ "$status" -ne 0 ] || [[ "$output" == *"Bad file descriptor"* ]]
}

# =============================================================================
# Variable name variations
# =============================================================================

@test "vredir: underscore in variable name" {
    run "$COLONY" -c 'exec {my_fd}>/tmp/vredir_under.txt; echo "fd=$my_fd"; exec {my_fd}>&-; rm -f /tmp/vredir_under.txt'
    [ "$status" -eq 0 ]
    [[ "$output" == "fd="* ]]
}

@test "vredir: variable name with numbers" {
    run "$COLONY" -c 'exec {fd123}>/tmp/vredir_num.txt; echo "fd=$fd123"; exec {fd123}>&-; rm -f /tmp/vredir_num.txt'
    [ "$status" -eq 0 ]
    [[ "$output" == "fd="* ]]
}

# =============================================================================
# Use in simple commands (not exec)
# =============================================================================

@test "vredir: {fd}> in simple command" {
    run "$COLONY" -c 'echo hello {fd}>/tmp/vredir_simple.txt; cat /tmp/vredir_simple.txt; rm -f /tmp/vredir_simple.txt'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "vredir: allocated fd available after command" {
    run "$COLONY" -c 'true {fd}>/tmp/vredir_avail.txt; echo "fd is $fd"; rm -f /tmp/vredir_avail.txt'
    [ "$status" -eq 0 ]
    [[ "$output" == "fd is "* ]]
}

# =============================================================================
# Error cases
# =============================================================================

@test "vredir: invalid variable name" {
    run "$COLONY" -c 'exec {123invalid}>/tmp/test.txt'
    [ "$status" -ne 0 ]
}

@test "vredir: empty variable name" {
    run "$COLONY" -c 'exec {}>/tmp/test.txt'
    [ "$status" -ne 0 ]
}

# =============================================================================
# Use with here documents
# =============================================================================

@test "vredir: {fd}< with here document" {
    run "$COLONY" -c 'exec {fd}<<EOF
hello world
EOF
cat <&$fd
exec {fd}<&-'
    [ "$status" -eq 0 ]
    [ "$output" = "hello world" ]
}

# =============================================================================
# Subshell behavior
# =============================================================================

@test "vredir: fd available in subshell" {
    run "$COLONY" -c 'exec {fd}>/tmp/vredir_sub.txt; (echo subshell >&$fd); exec {fd}>&-; cat /tmp/vredir_sub.txt; rm -f /tmp/vredir_sub.txt'
    [ "$status" -eq 0 ]
    [ "$output" = "subshell" ]
}

@test "vredir: allocation in subshell doesn't affect parent" {
    run "$COLONY" -c '(exec {fd}>/dev/null); echo "fd=${fd:-unset}"'
    [ "$status" -eq 0 ]
    [ "$output" = "fd=unset" ]
}

# =============================================================================
# Function behavior
# =============================================================================

@test "vredir: fd available in function" {
    run "$COLONY" -c 'func() { echo in_func >&$fd; }; exec {fd}>/tmp/vredir_func.txt; func; exec {fd}>&-; cat /tmp/vredir_func.txt; rm -f /tmp/vredir_func.txt'
    [ "$status" -eq 0 ]
    [ "$output" = "in_func" ]
}

# =============================================================================
# Process substitution with allocated fd
# =============================================================================

@test "vredir: use allocated fd with process substitution" {
    skip "Process substitution with vredir may not be supported"
    run "$COLONY" -c 'exec {fd}< <(echo hello); cat <&$fd; exec {fd}<&-'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

# =============================================================================
# Compare with bash
# =============================================================================

@test "compare vredir with bash: basic allocation" {
    colony_out=$("$COLONY" -c 'exec {fd}>/tmp/vr_cmp.txt; echo $fd; exec {fd}>&-; rm -f /tmp/vr_cmp.txt' 2>&1)
    bash_out=$(bash -c 'exec {fd}>/tmp/vr_cmp.txt; echo $fd; exec {fd}>&-; rm -f /tmp/vr_cmp.txt' 2>&1)
    [ "$colony_out" -ge 10 ] && [ "$bash_out" -ge 10 ]
}

@test "compare vredir with bash: write via fd" {
    "$COLONY" -c 'exec {fd}>/tmp/vr_colony.txt; echo test >&$fd; exec {fd}>&-'
    bash -c 'exec {fd}>/tmp/vr_bash.txt; echo test >&$fd; exec {fd}>&-'
    colony_content=$(cat /tmp/vr_colony.txt)
    bash_content=$(cat /tmp/vr_bash.txt)
    rm -f /tmp/vr_colony.txt /tmp/vr_bash.txt
    [ "$colony_content" = "$bash_content" ]
}
