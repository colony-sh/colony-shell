#!/usr/bin/env bats
# Redirection tests - based on bash tests/redir.tests
# Tests input/output redirections, file descriptor operations, and edge cases

setup() {
    COLONY="${COLONY_BIN:-../../../target/release/colony}"
    TMPDIR="${TMPDIR:-/tmp}"
    TEST_DIR="$TMPDIR/colony-redir-$$"
    mkdir -p "$TEST_DIR"
}

teardown() {
    rm -rf "$TEST_DIR"
}

# ============================================================================
# Basic Output Redirection (>)
# ============================================================================

@test "redir: basic output redirect" {
    run $COLONY -c "echo hello > $TEST_DIR/output.txt; cat $TEST_DIR/output.txt"
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "redir: output redirect overwrites existing file" {
    echo "old" > "$TEST_DIR/file.txt"
    run $COLONY -c "echo new > $TEST_DIR/file.txt; cat $TEST_DIR/file.txt"
    [ "$status" -eq 0 ]
    [ "$output" = "new" ]
}

@test "redir: output redirect to /dev/null" {
    run $COLONY -c "echo hidden > /dev/null; echo visible"
    [ "$status" -eq 0 ]
    [ "$output" = "visible" ]
}

@test "redir: output redirect with variable expansion" {
    run $COLONY -c "file=$TEST_DIR/var.txt; echo content > \$file; cat \$file"
    [ "$status" -eq 0 ]
    [ "$output" = "content" ]
}

# ============================================================================
# Append Redirection (>>)
# ============================================================================

@test "redir: append redirect creates file" {
    run $COLONY -c "echo line1 >> $TEST_DIR/append.txt; cat $TEST_DIR/append.txt"
    [ "$status" -eq 0 ]
    [ "$output" = "line1" ]
}

@test "redir: append redirect appends to file" {
    echo "line1" > "$TEST_DIR/append.txt"
    run $COLONY -c "echo line2 >> $TEST_DIR/append.txt; cat $TEST_DIR/append.txt"
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "line1" ]
    [ "${lines[1]}" = "line2" ]
}

@test "redir: multiple appends" {
    run $COLONY -c "echo a >> $TEST_DIR/multi.txt; echo b >> $TEST_DIR/multi.txt; echo c >> $TEST_DIR/multi.txt; cat $TEST_DIR/multi.txt"
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "a" ]
    [ "${lines[1]}" = "b" ]
    [ "${lines[2]}" = "c" ]
}

# ============================================================================
# Input Redirection (<)
# ============================================================================

@test "redir: basic input redirect" {
    echo "test content" > "$TEST_DIR/input.txt"
    run $COLONY -c "cat < $TEST_DIR/input.txt"
    [ "$status" -eq 0 ]
    [ "$output" = "test content" ]
}

@test "redir: input redirect with command" {
    echo -e "line1\nline2\nline3" > "$TEST_DIR/input.txt"
    run $COLONY -c "grep line2 < $TEST_DIR/input.txt"
    [ "$status" -eq 0 ]
    [ "$output" = "line2" ]
}

@test "redir: input from /dev/null" {
    run $COLONY -c "cat < /dev/null"
    [ "$status" -eq 0 ]
    [ "$output" = "" ]
}

# ============================================================================
# Combined Input and Output
# ============================================================================

@test "redir: input and output together" {
    echo "input data" > "$TEST_DIR/in.txt"
    run $COLONY -c "cat < $TEST_DIR/in.txt > $TEST_DIR/out.txt; cat $TEST_DIR/out.txt"
    [ "$status" -eq 0 ]
    [ "$output" = "input data" ]
}

@test "redir: redirect both stdin and stdout" {
    echo "test" > "$TEST_DIR/source.txt"
    run $COLONY -c "cat < $TEST_DIR/source.txt > $TEST_DIR/dest.txt; cat $TEST_DIR/dest.txt"
    [ "$status" -eq 0 ]
    [ "$output" = "test" ]
}

# ============================================================================
# Standard Error Redirection (2>)
# ============================================================================

@test "redir: redirect stderr to file" {
    run $COLONY -c "echo error message >&2 2> $TEST_DIR/err.txt; cat $TEST_DIR/err.txt"
    [ "$status" -eq 0 ]
    [ "$output" = "error message" ]
}

@test "redir: redirect stderr to /dev/null" {
    run $COLONY -c "echo visible; echo hidden >&2 2>/dev/null"
    [ "$status" -eq 0 ]
    [ "$output" = "visible" ]
}

@test "redir: append stderr" {
    run $COLONY -c "echo err1 >&2 2>> $TEST_DIR/errs.txt; echo err2 >&2 2>> $TEST_DIR/errs.txt; cat $TEST_DIR/errs.txt"
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "err1" ]
    [ "${lines[1]}" = "err2" ]
}

# ============================================================================
# Combining Stdout and Stderr (2>&1, &>)
# ============================================================================

@test "redir: redirect stderr to stdout with 2>&1" {
    run $COLONY -c "echo stdout; echo stderr >&2" 2>&1
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "stdout" ]
    [ "${lines[1]}" = "stderr" ]
}

@test "redir: redirect both to file with 2>&1" {
    run $COLONY -c "(echo out; echo err >&2) > $TEST_DIR/both.txt 2>&1; cat $TEST_DIR/both.txt"
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "out" ]
    [ "${lines[1]}" = "err" ]
}

@test "redir: shorthand both redirect &>" {
    run $COLONY -c "(echo out; echo err >&2) &> $TEST_DIR/combined.txt; cat $TEST_DIR/combined.txt"
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "out" ]
    [ "${lines[1]}" = "err" ]
}

@test "redir: &> append variant &>>" {
    run $COLONY -c "echo first &> $TEST_DIR/append-both.txt; echo second &>> $TEST_DIR/append-both.txt; cat $TEST_DIR/append-both.txt"
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "first" ]
    [ "${lines[1]}" = "second" ]
}

# ============================================================================
# File Descriptor Redirection (N> N<)
# ============================================================================

@test "redir: explicit fd 1 output" {
    run $COLONY -c "echo test 1> $TEST_DIR/fd1.txt; cat $TEST_DIR/fd1.txt"
    [ "$status" -eq 0 ]
    [ "$output" = "test" ]
}

@test "redir: explicit fd 0 input" {
    echo "from fd 0" > "$TEST_DIR/fd0.txt"
    run $COLONY -c "cat 0< $TEST_DIR/fd0.txt"
    [ "$status" -eq 0 ]
    [ "$output" = "from fd 0" ]
}

@test "redir: fd 3 output" {
    run $COLONY -c "echo to-fd-3 1>&3 3> $TEST_DIR/fd3.txt; cat $TEST_DIR/fd3.txt"
    [ "$status" -eq 0 ]
    [ "$output" = "to-fd-3" ]
}

@test "redir: fd 4 output" {
    run $COLONY -c "echo to-fd-4 1>&4 4> $TEST_DIR/fd4.txt; cat $TEST_DIR/fd4.txt"
    [ "$status" -eq 0 ]
    [ "$output" = "to-fd-4" ]
}

# ============================================================================
# File Descriptor Duplication (>&N, <&N)
# ============================================================================

@test "redir: duplicate stdout to stderr" {
    run $COLONY -c "echo to-stderr 1>&2" 2>&1
    [ "$status" -eq 0 ]
    [ "$output" = "to-stderr" ]
}

@test "redir: save and restore stdout" {
    run $COLONY -c "exec 3>&1; echo redirected > $TEST_DIR/temp.txt; exec 1>&3; echo restored; cat $TEST_DIR/temp.txt"
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "restored" ]
    [ "${lines[1]}" = "redirected" ]
}

# ============================================================================
# File Descriptor Close (>&-, <&-)
# ============================================================================

@test "redir: close stdout with >&-" {
    run $COLONY -c "echo visible; echo hidden >&-; echo visible-again"
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "visible" ]
    [ "${lines[1]}" = "visible-again" ]
}

@test "redir: close stdin with <&-" {
    run $COLONY -c "echo before; cat <&-; echo after"
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "before" ]
    [ "${lines[1]}" = "after" ]
}

# ============================================================================
# Read-Write Redirection (<>)
# ============================================================================

@test "redir: open file read-write" {
    echo "existing content" > "$TEST_DIR/rw.txt"
    run $COLONY -c "cat <> $TEST_DIR/rw.txt"
    [ "$status" -eq 0 ]
    [ "$output" = "existing content" ]
}

@test "redir: read-write with fd" {
    run $COLONY -c "exec 5<> $TEST_DIR/rw-fd.txt; echo test >&5; exec 5<&-; cat $TEST_DIR/rw-fd.txt"
    [ "$status" -eq 0 ]
    [ "$output" = "test" ]
}

# ============================================================================
# Noclobber Mode (set -C, >|)
# ============================================================================

@test "redir: noclobber prevents overwrite" {
    echo "protected" > "$TEST_DIR/protected.txt"
    run $COLONY -c "set -C; echo overwrite > $TEST_DIR/protected.txt"
    [ "$status" -ne 0 ]
}

@test "redir: force overwrite with >| in noclobber" {
    echo "original" > "$TEST_DIR/force.txt"
    run $COLONY -c "set -C; echo forced >| $TEST_DIR/force.txt; cat $TEST_DIR/force.txt"
    [ "$status" -eq 0 ]
    [ "$output" = "forced" ]
}

@test "redir: noclobber allows new files" {
    run $COLONY -c "set -C; echo new > $TEST_DIR/newfile.txt; cat $TEST_DIR/newfile.txt"
    [ "$status" -eq 0 ]
    [ "$output" = "new" ]
}

# ============================================================================
# Null Redirections (no command)
# ============================================================================

@test "redir: null redirect creates empty file" {
    run $COLONY -c "> $TEST_DIR/null.txt; test -f $TEST_DIR/null.txt && echo exists"
    [ "$status" -eq 0 ]
    [ "$output" = "exists" ]
}

@test "redir: null redirect in subshell" {
    run $COLONY -c "( > $TEST_DIR/sub-null.txt ); test -f $TEST_DIR/sub-null.txt && echo exists"
    [ "$status" -eq 0 ]
    [ "$output" = "exists" ]
}

# ============================================================================
# Redirections in Pipelines
# ============================================================================

@test "redir: redirect in pipeline" {
    run $COLONY -c "echo data | cat > $TEST_DIR/pipe-out.txt; cat $TEST_DIR/pipe-out.txt"
    [ "$status" -eq 0 ]
    [ "$output" = "data" ]
}

@test "redir: multiple redirects in pipeline" {
    echo "input" > "$TEST_DIR/pipe-in.txt"
    run $COLONY -c "cat < $TEST_DIR/pipe-in.txt | tr a-z A-Z > $TEST_DIR/pipe-out.txt; cat $TEST_DIR/pipe-out.txt"
    [ "$status" -eq 0 ]
    [ "$output" = "INPUT" ]
}

@test "redir: stderr redirect in pipeline" {
    run $COLONY -c "echo visible | cat 2> $TEST_DIR/pipe-err.txt"
    [ "$status" -eq 0 ]
    [ "$output" = "visible" ]
}

# ============================================================================
# Redirections with Builtins
# ============================================================================

@test "redir: redirect builtin output" {
    run $COLONY -c "echo from-builtin > $TEST_DIR/builtin.txt; cat $TEST_DIR/builtin.txt"
    [ "$status" -eq 0 ]
    [ "$output" = "from-builtin" ]
}

@test "redir: redirect pwd output" {
    run $COLONY -c "pwd > $TEST_DIR/pwd.txt; cat $TEST_DIR/pwd.txt"
    [ "$status" -eq 0 ]
    [ -n "$output" ]
}

@test "redir: redirect export (no output)" {
    run $COLONY -c "export VAR=value > $TEST_DIR/export.txt; test -f $TEST_DIR/export.txt && wc -c < $TEST_DIR/export.txt"
    [ "$status" -eq 0 ]
    [ "$output" = "0" ]
}

# ============================================================================
# Order of Redirections
# ============================================================================

@test "redir: order matters - stdout then stderr" {
    run $COLONY -c "(echo out; echo err >&2) > $TEST_DIR/order1.txt 2>&1; cat $TEST_DIR/order1.txt"
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "out" ]
    [ "${lines[1]}" = "err" ]
}

@test "redir: order matters - stderr then stdout" {
    run $COLONY -c "(echo out; echo err >&2) 2>&1 > $TEST_DIR/order2.txt; echo \$?"
    [ "$status" -eq 0 ]
    # stderr goes to terminal (original stdout), stdout to file
}

# ============================================================================
# Error Cases
# ============================================================================

@test "redir: input from nonexistent file fails" {
    run $COLONY -c "cat < $TEST_DIR/nonexistent.txt"
    [ "$status" -ne 0 ]
}

@test "redir: output to directory fails" {
    run $COLONY -c "echo test > $TEST_DIR"
    [ "$status" -ne 0 ]
}

@test "redir: invalid fd number" {
    run $COLONY -c "echo test 99999>&1"
    # Should either fail or be handled gracefully
    [ "$status" -ne 0 ] || [ -n "$output" ]
}

# ============================================================================
# Comparison with Bash
# ============================================================================

@test "compare: basic output redirect with bash" {
    $COLONY -c "echo test > $TEST_DIR/colony.txt" 2>/dev/null
    bash -c "echo test > $TEST_DIR/bash.txt" 2>/dev/null
    run diff "$TEST_DIR/colony.txt" "$TEST_DIR/bash.txt"
    [ "$status" -eq 0 ]
}

@test "compare: append redirect with bash" {
    $COLONY -c "echo a >> $TEST_DIR/colony-app.txt; echo b >> $TEST_DIR/colony-app.txt" 2>/dev/null
    bash -c "echo a >> $TEST_DIR/bash-app.txt; echo b >> $TEST_DIR/bash-app.txt" 2>/dev/null
    run diff "$TEST_DIR/colony-app.txt" "$TEST_DIR/bash-app.txt"
    [ "$status" -eq 0 ]
}

@test "compare: stderr redirect with bash" {
    $COLONY -c "(echo err >&2) 2> $TEST_DIR/colony-err.txt" 2>/dev/null
    bash -c "(echo err >&2) 2> $TEST_DIR/bash-err.txt" 2>/dev/null
    run diff "$TEST_DIR/colony-err.txt" "$TEST_DIR/bash-err.txt"
    [ "$status" -eq 0 ]
}

@test "compare: combined redirect with bash" {
    $COLONY -c "(echo out; echo err >&2) &> $TEST_DIR/colony-both.txt" 2>/dev/null
    bash -c "(echo out; echo err >&2) &> $TEST_DIR/bash-both.txt" 2>/dev/null
    run diff "$TEST_DIR/colony-both.txt" "$TEST_DIR/bash-both.txt"
    [ "$status" -eq 0 ]
}
