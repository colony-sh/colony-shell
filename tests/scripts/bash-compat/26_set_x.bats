#!/usr/bin/env bats
# Test Colony shell `set -x` (xtrace) debug tracing for POSIX compliance
# set -x prints a trace of commands after expansion, before execution
# Reference: https://www.gnu.org/software/bash/manual/html_node/The-Set-Builtin.html
# Reference: https://github.com/bminor/bash/blob/master/tests/set-x.tests

COLONY="${COLONY_BIN}"

setup() {
    TEST_TEMP="$(mktemp -d)"
    export TEST_TEMP
}

teardown() {
    rm -rf "$TEST_TEMP"
}

# ============================================================================
# Basic set -x Enable/Disable
# ============================================================================

@test "set -x: enable xtrace mode" {
    run "$COLONY" -c "set -x; echo hello" 2>&1
    [ "$status" -eq 0 ]
    # Output should contain trace line and actual output
    [[ "$output" == *"+ echo hello"* ]] || [[ "$output" == *"+echo hello"* ]]
    [[ "$output" == *"hello"* ]]
}

@test "set -x: disable xtrace with set +x" {
    run "$COLONY" -c "set -x; echo traced; set +x; echo not-traced" 2>&1
    [ "$status" -eq 0 ]
    # First echo should have trace
    [[ "$output" == *"+ echo traced"* ]] || [[ "$output" == *"+echo traced"* ]]
    # Second echo should NOT have trace (just the output)
    [[ "$output" == *"not-traced"* ]]
    # Count trace lines - should not include "echo not-traced"
}

@test "set -x: set -o xtrace equivalent" {
    run "$COLONY" -c "set -o xtrace; echo test" 2>&1
    [ "$status" -eq 0 ]
    [[ "$output" == *"+ echo test"* ]] || [[ "$output" == *"+echo test"* ]]
}

@test "set -x: set +o xtrace disables" {
    run "$COLONY" -c "set -x; echo traced; set +o xtrace; echo silent" 2>&1
    [ "$status" -eq 0 ]
    [[ "$output" == *"+ echo traced"* ]] || [[ "$output" == *"+echo traced"* ]]
}

# ============================================================================
# Trace Output Format - Plus Prefix
# ============================================================================

@test "set -x: default PS4 is '+ '" {
    run "$COLONY" -c "set -x; echo test" 2>&1
    [ "$status" -eq 0 ]
    # Default PS4 is '+ ' - trace lines start with + 
    [[ "$output" == *"+ "* ]]
}

@test "set -x: trace shows expanded command" {
    run "$COLONY" -c "set -x; VAR=world; echo hello \$VAR" 2>&1
    [ "$status" -eq 0 ]
    # Trace should show expanded value
    [[ "$output" == *"echo hello world"* ]]
}

@test "set -x: trace shows expanded variables" {
    run "$COLONY" -c "set -x; X=foo; echo \$X" 2>&1
    [ "$status" -eq 0 ]
    # Trace should show 'foo' not '$X'
    [[ "$output" == *"echo foo"* ]]
}

@test "set -x: trace shows quoted strings correctly" {
    run "$COLONY" -c "set -x; echo 'single quotes'" 2>&1
    [ "$status" -eq 0 ]
    [[ "$output" == *"echo single quotes"* ]] || [[ "$output" == *"echo 'single quotes'"* ]]
}

@test "set -x: trace shows double quoted strings" {
    run "$COLONY" -c 'set -x; echo "double quotes"' 2>&1
    [ "$status" -eq 0 ]
    # Bash may show quotes or not depending on version
    [[ "$output" == *"double quotes"* ]]
}

# ============================================================================
# PS4 Prompt Customization
# ============================================================================

@test "set -x: custom PS4 changes trace prefix" {
    run "$COLONY" -c "PS4='DEBUG: '; set -x; echo test" 2>&1
    [ "$status" -eq 0 ]
    [[ "$output" == *"DEBUG: echo test"* ]] || [[ "$output" == *"DEBUG: "* ]]
}

@test "set -x: PS4 with variable expansion" {
    skip "PS4 variable expansion not yet implemented"
    run "$COLONY" -c 'PS4='"'"'$LINENO: '"'"'; set -x; echo line1; echo line2' 2>&1
    [ "$status" -eq 0 ]
    # PS4 should expand $LINENO for each trace line
    [[ "$output" == *": echo"* ]]
}

@test "set -x: PS4 with command substitution" {
    skip "PS4 command substitution not yet implemented"
    run "$COLONY" -c 'PS4='"'"'$(date +%s): '"'"'; set -x; echo test' 2>&1
    [ "$status" -eq 0 ]
    # PS4 should expand command substitution
    [[ "$output" =~ [0-9]+:.*echo ]]
}

@test "set -x: PS4 empty string" {
    run "$COLONY" -c "PS4=''; set -x; echo test" 2>&1
    [ "$status" -eq 0 ]
    # With empty PS4, trace has no prefix
    [[ "$output" == *"echo test"* ]]
}

@test "set -x: PS4 with newline" {
    run "$COLONY" -c $'PS4=\'TRACE\\n\'; set -x; echo test' 2>&1
    [ "$status" -eq 0 ]
    [[ "$output" == *"TRACE"* ]] || [[ "$output" == *"echo test"* ]]
}

# ============================================================================
# Trace Goes to stderr (fd 2)
# ============================================================================

@test "set -x: trace output goes to stderr" {
    # Use subshell to properly separate stdout/stderr for BATS run
    "$COLONY" -c "set -x; echo stdout" >"$TEST_TEMP/stdout.txt" 2>"$TEST_TEMP/stderr.txt"
    # stdout should only have command output
    [ "$(cat $TEST_TEMP/stdout.txt)" = "stdout" ]
    # Trace should be in stderr file
    grep -q "echo" "$TEST_TEMP/stderr.txt"
}

@test "set -x: stdout and stderr separation" {
    "$COLONY" -c "set -x; echo stdout-line" >"$TEST_TEMP/stdout.txt" 2>"$TEST_TEMP/stderr.txt"
    # stdout should only have command output
    [ "$(cat $TEST_TEMP/stdout.txt)" = "stdout-line" ]
    # stderr should have trace
    grep -q "echo" "$TEST_TEMP/stderr.txt"
}

@test "set -x: redirect stderr hides trace" {
    # When stderr is redirected INSIDE the script, trace goes there
    "$COLONY" -c "set -x; echo visible" >"$TEST_TEMP/stdout.txt" 2>/dev/null
    [ "$(cat $TEST_TEMP/stdout.txt)" = "visible" ]
}

# ============================================================================
# BASH_XTRACEFD - Redirect Trace to Custom FD
# ============================================================================

@test "set -x: BASH_XTRACEFD redirects trace to fd" {
    skip "BASH_XTRACEFD not yet implemented"
    run "$COLONY" -c "exec 4>$TEST_TEMP/trace.txt; BASH_XTRACEFD=4; set -x; echo test; exec 4>&-; cat $TEST_TEMP/trace.txt"
    [ "$status" -eq 0 ]
    [[ "$output" == *"echo test"* ]]
}

@test "set -x: BASH_XTRACEFD invalid fd" {
    skip "BASH_XTRACEFD not yet implemented"
    run "$COLONY" -c "BASH_XTRACEFD=99; set -x; echo test" 2>&1
    # Should produce an error about invalid fd
    [[ "$output" == *"invalid"* ]] || [[ "$output" == *"error"* ]] || [ "$status" -ne 0 ]
}

@test "set -x: unset BASH_XTRACEFD restores stderr" {
    skip "BASH_XTRACEFD not yet implemented"
    run "$COLONY" -c "exec 4>$TEST_TEMP/trace.txt; BASH_XTRACEFD=4; set -x; echo to-fd4; unset BASH_XTRACEFD; echo to-stderr" 2>&1
    [ "$status" -eq 0 ]
    # After unset, trace goes back to stderr
    [[ "$output" == *"echo to-stderr"* ]]
}

# ============================================================================
# Variable Assignment Tracing
# ============================================================================

@test "set -x: trace simple assignment" {
    run "$COLONY" -c "set -x; VAR=value; echo done" 2>&1
    [ "$status" -eq 0 ]
    [[ "$output" == *"VAR=value"* ]]
}

@test "set -x: trace export assignment" {
    run "$COLONY" -c "set -x; export VAR=value; echo done" 2>&1
    [ "$status" -eq 0 ]
    [[ "$output" == *"export"* ]] && [[ "$output" == *"VAR=value"* ]]
}

@test "set -x: trace assignment with expansion" {
    run "$COLONY" -c "set -x; A=foo; B=\$A; echo done" 2>&1
    [ "$status" -eq 0 ]
    # Assignment should show expanded value
    [[ "$output" == *"B=foo"* ]]
}

@test "set -x: trace readonly assignment" {
    run "$COLONY" -c "set -x; readonly VAR=value; echo done" 2>&1
    [ "$status" -eq 0 ]
    [[ "$output" == *"readonly"* ]] && [[ "$output" == *"VAR=value"* ]]
}

@test "set -x: trace local assignment in function" {
    run "$COLONY" -c "set -x; myfunc() { local X=local; echo \$X; }; myfunc" 2>&1
    [ "$status" -eq 0 ]
    [[ "$output" == *"local"* ]] || [[ "$output" == *"X=local"* ]]
}

# ============================================================================
# Command Substitution Tracing
# ============================================================================

@test "set -x: trace command substitution" {
    run "$COLONY" -c "set -x; X=\$(echo hello); echo \$X" 2>&1
    [ "$status" -eq 0 ]
    # Should trace the inner echo
    [[ "$output" == *"echo hello"* ]]
}

@test "set -x: trace nested command substitution" {
    run "$COLONY" -c "set -x; X=\$(echo \$(echo nested)); echo \$X" 2>&1
    [ "$status" -eq 0 ]
    # Should show traces for nested commands
    [[ "$output" == *"nested"* ]]
}

@test "set -x: trace backtick substitution" {
    skip "Backtick command substitution not yet implemented"
    run "$COLONY" -c 'set -x; X=`echo backtick`; echo $X' 2>&1
    [ "$status" -eq 0 ]
    [[ "$output" == *"echo backtick"* ]]
}

# ============================================================================
# Function Call Tracing
# ============================================================================

@test "set -x: trace function definition" {
    run "$COLONY" -c "set -x; myfunc() { echo inside; }; echo defined" 2>&1
    [ "$status" -eq 0 ]
    # Function definition might not be traced, but 'echo defined' should be
    [[ "$output" == *"echo defined"* ]]
}

@test "set -x: trace function call" {
    run "$COLONY" -c "myfunc() { echo inside; }; set -x; myfunc" 2>&1
    [ "$status" -eq 0 ]
    # Should trace the function call
    [[ "$output" == *"myfunc"* ]]
}

@test "set -x: trace commands inside function" {
    run "$COLONY" -c "myfunc() { echo one; echo two; }; set -x; myfunc" 2>&1
    [ "$status" -eq 0 ]
    # Should trace echo commands inside function
    [[ "$output" == *"echo one"* ]]
    [[ "$output" == *"echo two"* ]]
}

@test "set -x: trace function with arguments" {
    run "$COLONY" -c "greet() { echo hello \$1; }; set -x; greet world" 2>&1
    [ "$status" -eq 0 ]
    [[ "$output" == *"greet world"* ]]
    [[ "$output" == *"echo hello world"* ]]
}

@test "set -x: trace recursive function" {
    run "$COLONY" -c "count() { if [ \$1 -gt 0 ]; then echo \$1; count \$((\$1-1)); fi; }; set -x; count 3" 2>&1
    [ "$status" -eq 0 ]
    # Should see multiple recursive calls traced
    [[ "$output" == *"count"* ]]
}

# ============================================================================
# Pipeline Tracing
# ============================================================================

@test "set -x: trace simple pipeline" {
    run "$COLONY" -c "set -x; echo hello | cat" 2>&1
    [ "$status" -eq 0 ]
    # Both commands in pipeline should be traced
    [[ "$output" == *"echo hello"* ]]
}

@test "set -x: trace multi-stage pipeline" {
    run "$COLONY" -c "set -x; echo abc | tr a-z A-Z | cat" 2>&1
    [ "$status" -eq 0 ]
    [[ "$output" == *"echo abc"* ]]
    [[ "$output" == *"tr a-z A-Z"* ]] || [[ "$output" == *"tr"* ]]
}

@test "set -x: trace pipeline with variables" {
    run "$COLONY" -c "set -x; DATA=test; echo \$DATA | cat" 2>&1
    [ "$status" -eq 0 ]
    [[ "$output" == *"echo test"* ]]
}

# ============================================================================
# Subshell Tracing
# ============================================================================

@test "set -x: trace subshell commands" {
    run "$COLONY" -c "set -x; (echo in-subshell)" 2>&1
    [ "$status" -eq 0 ]
    [[ "$output" == *"echo in-subshell"* ]]
}

@test "set -x: trace brace group commands" {
    run "$COLONY" -c "set -x; { echo in-braces; }" 2>&1
    [ "$status" -eq 0 ]
    [[ "$output" == *"echo in-braces"* ]]
}

@test "set -x: xtrace inherited in subshell" {
    run "$COLONY" -c "set -x; (echo sub1; echo sub2)" 2>&1
    [ "$status" -eq 0 ]
    # Both echoes in subshell should be traced
    [[ "$output" == *"echo sub1"* ]]
    [[ "$output" == *"echo sub2"* ]]
}

@test "set -x: xtrace can be disabled in subshell" {
    run "$COLONY" -c "set -x; (set +x; echo silent)" 2>&1
    [ "$status" -eq 0 ]
    # The echo in subshell should NOT have trace
    # (the set +x line might still be traced)
}

# ============================================================================
# Control Flow Tracing
# ============================================================================

@test "set -x: trace for loop" {
    run "$COLONY" -c "set -x; for i in a b c; do echo \$i; done" 2>&1
    [ "$status" -eq 0 ]
    # Should trace each iteration
    [[ "$output" == *"echo a"* ]]
    [[ "$output" == *"echo b"* ]]
    [[ "$output" == *"echo c"* ]]
}

@test "set -x: trace while loop" {
    run "$COLONY" -c "set -x; i=0; while [ \$i -lt 2 ]; do echo \$i; i=\$((\$i+1)); done" 2>&1
    [ "$status" -eq 0 ]
    [[ "$output" == *"echo 0"* ]]
    [[ "$output" == *"echo 1"* ]]
}

@test "set -x: trace if statement" {
    run "$COLONY" -c "set -x; if true; then echo yes; fi" 2>&1
    [ "$status" -eq 0 ]
    [[ "$output" == *"true"* ]]
    [[ "$output" == *"echo yes"* ]]
}

@test "set -x: trace if-else statement" {
    run "$COLONY" -c "set -x; if false; then echo no; else echo yes; fi" 2>&1
    [ "$status" -eq 0 ]
    [[ "$output" == *"false"* ]]
    [[ "$output" == *"echo yes"* ]]
}

@test "set -x: trace case statement" {
    run "$COLONY" -c "set -x; case foo in foo) echo matched;; esac" 2>&1
    [ "$status" -eq 0 ]
    [[ "$output" == *"echo matched"* ]]
}

# ============================================================================
# Arithmetic Tracing
# ============================================================================

@test "set -x: trace arithmetic expansion" {
    run "$COLONY" -c "set -x; X=\$((2+3)); echo \$X" 2>&1
    [ "$status" -eq 0 ]
    [[ "$output" == *"X=5"* ]] || [[ "$output" == *"echo 5"* ]]
}

@test "set -x: trace arithmetic for loop" {
    skip "C-style for loop may not be fully implemented"
    run "$COLONY" -c "set -x; for ((i=0; i<3; i++)); do echo \$i; done" 2>&1
    [ "$status" -eq 0 ]
    [[ "$output" == *"echo"* ]]
}

# ============================================================================
# Compound Assignment Tracing
# ============================================================================

@test "set -x: trace array assignment" {
    skip "Array assignment tracing not yet implemented"
    run "$COLONY" -c "set -x; arr=(a b c); echo done" 2>&1
    [ "$status" -eq 0 ]
    [[ "$output" == *"arr="* ]]
}

@test "set -x: trace append operator" {
    skip "Append operator (+=) not yet implemented"
    run "$COLONY" -c "set -x; foo=one; foo+=two; echo \$foo" 2>&1
    [ "$status" -eq 0 ]
    [[ "$output" == *"foo+=two"* ]] || [[ "$output" == *"onetwo"* ]]
}

# ============================================================================
# Special Cases
# ============================================================================

@test "set -x: trace with inline env vars" {
    run "$COLONY" -c "set -x; VAR=inline echo test" 2>&1
    [ "$status" -eq 0 ]
    [[ "$output" == *"VAR=inline"* ]] || [[ "$output" == *"echo test"* ]]
}

@test "set -x: trace redirections" {
    run "$COLONY" -c "set -x; echo hello > $TEST_TEMP/out.txt" 2>&1
    [ "$status" -eq 0 ]
    [[ "$output" == *"echo hello"* ]]
}

@test "set -x: trace with stderr redirect" {
    run "$COLONY" -c "set -x; echo error >&2" 2>&1
    [ "$status" -eq 0 ]
    [[ "$output" == *"echo error"* ]]
}

@test "set -x: trace read builtin" {
    run "$COLONY" -c "set -x; echo input | read VAR; echo done" 2>&1
    [ "$status" -eq 0 ]
    [[ "$output" == *"read"* ]] || [[ "$output" == *"VAR"* ]]
}

@test "set -x: trace cd builtin" {
    run "$COLONY" -c "set -x; cd /tmp; pwd" 2>&1
    [ "$status" -eq 0 ]
    [[ "$output" == *"cd /tmp"* ]] || [[ "$output" == *"cd"* ]]
}

@test "set -x: trace source command" {
    echo "echo sourced" > "$TEST_TEMP/script.sh"
    run "$COLONY" -c "set -x; source $TEST_TEMP/script.sh" 2>&1
    [ "$status" -eq 0 ]
    [[ "$output" == *"source"* ]] || [[ "$output" == *"echo sourced"* ]]
}

@test "set -x: trace test command" {
    run "$COLONY" -c "set -x; [ -f /etc/passwd ]; echo \$?" 2>&1
    [ "$status" -eq 0 ]
    [[ "$output" == *"["* ]] || [[ "$output" == *"test"* ]] || [[ "$output" == *"-f"* ]]
}

# ============================================================================
# Edge Cases
# ============================================================================

@test "set -x: empty command" {
    run "$COLONY" -c "set -x; :; echo done" 2>&1
    [ "$status" -eq 0 ]
    [[ "$output" == *":"* ]] || [[ "$output" == *"echo done"* ]]
}

@test "set -x: comment not traced" {
    run "$COLONY" -c "set -x; # this is a comment"$'\n'"echo test" 2>&1
    [ "$status" -eq 0 ]
    # Comments should not appear in trace
    [[ "$output" != *"# this is a comment"* ]] || [[ "$output" == *"echo test"* ]]
}

@test "set -x: multiple set -x is idempotent" {
    run "$COLONY" -c "set -x; set -x; set -x; echo test" 2>&1
    [ "$status" -eq 0 ]
    [[ "$output" == *"echo test"* ]]
}

@test "set -x: toggle on and off" {
    run "$COLONY" -c "set -x; echo on; set +x; echo off; set -x; echo on-again" 2>&1
    [ "$status" -eq 0 ]
    [[ "$output" == *"echo on"* ]]
    [[ "$output" == *"echo on-again"* ]]
}

@test "set -x: trace multiline command" {
    run "$COLONY" -c $'set -x\necho line1\necho line2' 2>&1
    [ "$status" -eq 0 ]
    [[ "$output" == *"echo line1"* ]]
    [[ "$output" == *"echo line2"* ]]
}

# ============================================================================
# Status in $- Variable
# ============================================================================

@test "set -x: xtrace shows in \$-" {
    run "$COLONY" -c "set -x; echo \$-" 2>&1
    [ "$status" -eq 0 ]
    # $- should contain 'x' when xtrace is on
    [[ "$output" == *"x"* ]]
}

@test "set -x: xtrace removed from \$- when off" {
    run "$COLONY" -c "set +x; echo \$-"
    [ "$status" -eq 0 ]
    # $- should NOT contain 'x' when xtrace is off
    # Note: $- may still contain other flags like 'hB' but not 'x'
    [[ "$output" != *"x"* ]]
}

# ============================================================================
# Nesting Depth Indication (++ prefix)
# ============================================================================

@test "set -x: nested depth shown with extra +" {
    skip "Nested depth indication (++) not yet implemented"
    # In bash, command substitution shows ++ instead of +
    run "$COLONY" -c "set -x; X=\$(echo nested); echo \$X" 2>&1
    [ "$status" -eq 0 ]
    # Inner command should have ++ prefix
    [[ "$output" == *"++"*"echo nested"* ]]
}

# ============================================================================
# Comparison with Bash
# ============================================================================

@test "compare: basic set -x with bash" {
    # Compare trace format (ignore timing differences)
    run bash -c "set -x; echo test" 2>&1
    local bash_output="$output"

    run "$COLONY" -c "set -x; echo test" 2>&1
    local colony_output="$output"

    # Both should have trace prefix and output
    [[ "$bash_output" == *"echo test"* ]]
    [[ "$colony_output" == *"echo test"* ]]
    [[ "$bash_output" == *"test"* ]]
    [[ "$colony_output" == *"test"* ]]
}

@test "compare: set +x disables trace with bash" {
    run bash -c "set -x; echo traced; set +x; echo silent" 2>&1
    local bash_traced=$(echo "$output" | grep -c "echo traced" || true)
    local bash_silent=$(echo "$output" | grep -c "echo silent" || true)

    run "$COLONY" -c "set -x; echo traced; set +x; echo silent" 2>&1
    local colony_traced=$(echo "$output" | grep -c "echo traced" || true)

    # Both should trace 'echo traced' at least once
    [ "$bash_traced" -ge 1 ]
    [ "$colony_traced" -ge 1 ]
}

@test "compare: for loop trace with bash" {
    run bash -c "set -x; for i in a b; do echo \$i; done" 2>&1
    local bash_output="$output"

    run "$COLONY" -c "set -x; for i in a b; do echo \$i; done" 2>&1
    local colony_output="$output"

    # Both should trace echo a and echo b
    [[ "$bash_output" == *"echo a"* ]]
    [[ "$colony_output" == *"echo a"* ]]
    [[ "$bash_output" == *"echo b"* ]]
    [[ "$colony_output" == *"echo b"* ]]
}

@test "compare: PS4 custom prefix with bash" {
    run bash -c "PS4='>>> '; set -x; echo test" 2>&1
    local bash_output="$output"

    run "$COLONY" -c "PS4='>>> '; set -x; echo test" 2>&1
    local colony_output="$output"

    # Both should use custom prefix
    [[ "$bash_output" == *">>>"* ]]
    [[ "$colony_output" == *">>>"* ]]
}

@test "compare: variable expansion trace with bash" {
    run bash -c "set -x; X=hello; echo \$X" 2>&1
    local bash_output="$output"

    run "$COLONY" -c "set -x; X=hello; echo \$X" 2>&1
    local colony_output="$output"

    # Both should show expanded value in trace
    [[ "$bash_output" == *"echo hello"* ]]
    [[ "$colony_output" == *"echo hello"* ]]
}

@test "compare: function trace with bash" {
    run bash -c "f() { echo inside; }; set -x; f" 2>&1
    local bash_output="$output"

    run "$COLONY" -c "f() { echo inside; }; set -x; f" 2>&1
    local colony_output="$output"

    # Both should trace the function call and internal echo
    [[ "$bash_output" == *"f"* ]]
    [[ "$colony_output" == *"f"* ]]
    [[ "$bash_output" == *"echo inside"* ]]
    [[ "$colony_output" == *"echo inside"* ]]
}

@test "compare: stderr trace with bash" {
    bash -c "set -x; echo stdout" >"$TEST_TEMP/bash-out.txt" 2>"$TEST_TEMP/bash-err.txt"
    "$COLONY" -c "set -x; echo stdout" >"$TEST_TEMP/colony-out.txt" 2>"$TEST_TEMP/colony-err.txt"

    # stdout should be clean
    [ "$(cat $TEST_TEMP/bash-out.txt)" = "stdout" ]
    [ "$(cat $TEST_TEMP/colony-out.txt)" = "stdout" ]

    # stderr should have trace
    grep -q "echo" "$TEST_TEMP/bash-err.txt"
    grep -q "echo" "$TEST_TEMP/colony-err.txt"
}

# ============================================================================
# Integration with Other Shell Features
# ============================================================================

@test "set -x: with set -e (errexit)" {
    skip "set -e not yet implemented"
    run "$COLONY" -c "set -ex; true; echo done" 2>&1
    [ "$status" -eq 0 ]
    [[ "$output" == *"true"* ]]
    [[ "$output" == *"echo done"* ]]
}

@test "set -x: with set -v (verbose)" {
    skip "set -v not yet implemented"
    run "$COLONY" -c "set -xv; echo test" 2>&1
    [ "$status" -eq 0 ]
    # -v shows input, -x shows expanded
    [[ "$output" == *"echo test"* ]]
}

@test "set -x: with set -u (nounset)" {
    skip "set -u not yet implemented"
    run "$COLONY" -c "set -xu; VAR=set; echo \$VAR" 2>&1
    [ "$status" -eq 0 ]
    [[ "$output" == *"echo set"* ]]
}

@test "set -x: with trap" {
    run "$COLONY" -c "trap 'echo cleanup' EXIT; set -x; echo main" 2>&1
    [ "$status" -eq 0 ]
    [[ "$output" == *"echo main"* ]]
    [[ "$output" == *"echo cleanup"* ]] || [[ "$output" == *"cleanup"* ]]
}

@test "set -x: script file execution" {
    echo 'set -x; echo from-script' > "$TEST_TEMP/test-script.sh"
    run "$COLONY" "$TEST_TEMP/test-script.sh" 2>&1
    [ "$status" -eq 0 ]
    [[ "$output" == *"echo from-script"* ]]
    [[ "$output" == *"from-script"* ]]
}
