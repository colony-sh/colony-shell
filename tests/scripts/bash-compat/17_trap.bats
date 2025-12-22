#!/usr/bin/env bats
# Test Colony shell trap builtin for POSIX compliance
# trap - set signal handlers, execute commands on signals or shell events
# Reference: https://www.gnu.org/software/bash/manual/html_node/Bourne-Shell-Builtins.html#index-trap

COLONY="${COLONY_BIN}"

setup() {
    TEST_TEMP="$(mktemp -d)"
    export TEST_TEMP
}

teardown() {
    rm -rf "$TEST_TEMP"
}

# ============================================================================
# Basic Trap Syntax - trap 'commands' SIGNAL
# ============================================================================

@test "trap: basic syntax - trap 'cmd' SIGNAL" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap 'echo trapped' INT; echo done"
    [ "$status" -eq 0 ]
    [ "$output" = "done" ]
}

@test "trap: trap with double quotes" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap \"echo trapped\" INT; echo done"
    [ "$status" -eq 0 ]
    [ "$output" = "done" ]
}

@test "trap: trap with multiple commands" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap 'echo first; echo second' EXIT; exit 0"
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "first" ]
    [ "${lines[1]}" = "second" ]
}

@test "trap: trap with variable in command" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c 'MSG=hello; trap "echo \$MSG" EXIT; exit 0'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "trap: trap command with single quotes preserves literal" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "MSG=hello; trap 'echo \$MSG' EXIT; MSG=goodbye; exit 0"
    [ "$status" -eq 0 ]
    [ "$output" = "goodbye" ]
}

# ============================================================================
# Trap on EXIT
# ============================================================================

@test "trap: EXIT runs on normal exit" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap 'echo cleanup' EXIT; echo main"
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "main" ]
    [ "${lines[1]}" = "cleanup" ]
}

@test "trap: EXIT runs on explicit exit 0" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap 'echo cleanup' EXIT; exit 0"
    [ "$status" -eq 0 ]
    [ "$output" = "cleanup" ]
}

@test "trap: EXIT runs on explicit exit non-zero" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap 'echo cleanup' EXIT; exit 42"
    [ "$status" -eq 42 ]
    [ "$output" = "cleanup" ]
}

@test "trap: EXIT preserves exit code" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap 'echo cleanup' EXIT; exit 7"
    [ "$status" -eq 7 ]
    [ "$output" = "cleanup" ]
}

@test "trap: EXIT runs after script completion" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap 'echo done' EXIT; echo one; echo two; echo three"
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "one" ]
    [ "${lines[1]}" = "two" ]
    [ "${lines[2]}" = "three" ]
    [ "${lines[3]}" = "done" ]
}

@test "trap: EXIT can write to temp file" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap 'echo trapped > $TEST_TEMP/trap.txt' EXIT; exit 0"
    [ "$status" -eq 0 ]
    [ -f "$TEST_TEMP/trap.txt" ]
    [ "$(cat $TEST_TEMP/trap.txt)" = "trapped" ]
}

# ============================================================================
# Trap on ERR
# ============================================================================

@test "trap: ERR runs on command failure" {
    skip "trap ERR not yet implemented"
    run "$COLONY" -c "trap 'echo error occurred' ERR; false; echo after"
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "error occurred" ]
    [ "${lines[1]}" = "after" ]
}

@test "trap: ERR does not run on success" {
    skip "trap ERR not yet implemented"
    run "$COLONY" -c "trap 'echo error' ERR; true; echo done"
    [ "$status" -eq 0 ]
    [ "$output" = "done" ]
}

@test "trap: ERR with set -e" {
    skip "trap ERR not yet implemented"
    run "$COLONY" -c "set -e; trap 'echo caught' ERR; false; echo unreached"
    [ "$status" -ne 0 ]
    [ "$output" = "caught" ]
}

@test "trap: ERR multiple failures" {
    skip "trap ERR not yet implemented"
    run "$COLONY" -c "trap 'echo err' ERR; false; false; true"
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "err" ]
    [ "${lines[1]}" = "err" ]
}

# ============================================================================
# Trap on DEBUG
# ============================================================================

@test "trap: DEBUG runs before each command" {
    skip "trap DEBUG not yet implemented"
    run "$COLONY" -c "trap 'echo DEBUG' DEBUG; echo one; echo two"
    [ "$status" -eq 0 ]
    # DEBUG runs before each command
    [[ "$output" == *"DEBUG"* ]]
}

@test "trap: DEBUG shows command about to run" {
    skip "trap DEBUG not yet implemented"
    run "$COLONY" -c 'trap '"'"'echo "running: $BASH_COMMAND"'"'"' DEBUG; echo test'
    [ "$status" -eq 0 ]
    [[ "$output" == *"running:"* ]]
}

# ============================================================================
# Trap on RETURN
# ============================================================================

@test "trap: RETURN runs after function returns" {
    skip "trap RETURN not yet implemented"
    run "$COLONY" -c "trap 'echo returned' RETURN; myfunc() { echo in-func; }; myfunc"
    [ "$status" -eq 0 ]
    [[ "$output" == *"returned"* ]]
}

# ============================================================================
# Trap on Signals - INT, TERM, HUP
# ============================================================================

@test "trap: INT signal handler can be set" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap 'echo interrupted' INT; echo ready"
    [ "$status" -eq 0 ]
    [ "$output" = "ready" ]
}

@test "trap: TERM signal handler can be set" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap 'echo terminated' TERM; echo ready"
    [ "$status" -eq 0 ]
    [ "$output" = "ready" ]
}

@test "trap: HUP signal handler can be set" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap 'echo hangup' HUP; echo ready"
    [ "$status" -eq 0 ]
    [ "$output" = "ready" ]
}

@test "trap: QUIT signal handler can be set" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap 'echo quit' QUIT; echo ready"
    [ "$status" -eq 0 ]
    [ "$output" = "ready" ]
}

@test "trap: USR1 signal handler can be set" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap 'echo user1' USR1; echo ready"
    [ "$status" -eq 0 ]
    [ "$output" = "ready" ]
}

@test "trap: USR2 signal handler can be set" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap 'echo user2' USR2; echo ready"
    [ "$status" -eq 0 ]
    [ "$output" = "ready" ]
}

@test "trap: signal by number (2 = INT)" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap 'echo signum' 2; echo ready"
    [ "$status" -eq 0 ]
    [ "$output" = "ready" ]
}

@test "trap: signal with SIG prefix (SIGINT)" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap 'echo sig' SIGINT; echo ready"
    [ "$status" -eq 0 ]
    [ "$output" = "ready" ]
}

# ============================================================================
# Resetting Traps - trap - SIGNAL
# ============================================================================

@test "trap: reset trap with dash" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap 'echo trapped' EXIT; trap - EXIT; exit 0"
    [ "$status" -eq 0 ]
    [ "$output" = "" ]
}

@test "trap: reset trap restores default action" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap 'echo custom' EXIT; trap - EXIT; echo done"
    [ "$status" -eq 0 ]
    [ "$output" = "done" ]
}

@test "trap: reset multiple signals" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap 'echo x' INT TERM; trap - INT TERM; echo done"
    [ "$status" -eq 0 ]
    [ "$output" = "done" ]
}

@test "trap: reset with empty string (different from dash)" {
    skip "trap builtin not yet implemented"
    # Empty string ignores the signal, dash restores default
    run "$COLONY" -c "trap '' INT; echo done"
    [ "$status" -eq 0 ]
    [ "$output" = "done" ]
}

@test "trap: ignore signal with empty string" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap '' EXIT; echo done"
    [ "$status" -eq 0 ]
    [ "$output" = "done" ]
}

# ============================================================================
# Listing Traps - trap with no arguments
# ============================================================================

@test "trap: list traps with no arguments" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap 'echo test' EXIT; trap"
    [ "$status" -eq 0 ]
    [[ "$output" == *"EXIT"* ]]
}

@test "trap: list shows all set traps" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap 'echo exit' EXIT; trap 'echo int' INT; trap"
    [ "$status" -eq 0 ]
    [[ "$output" == *"EXIT"* ]]
    [[ "$output" == *"INT"* ]]
}

@test "trap: list with -p flag" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap 'echo test' EXIT; trap -p"
    [ "$status" -eq 0 ]
    [[ "$output" == *"trap"* ]]
}

@test "trap: list specific signal with -p" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap 'echo exit' EXIT; trap 'echo int' INT; trap -p EXIT"
    [ "$status" -eq 0 ]
    [[ "$output" == *"EXIT"* ]]
    [[ "$output" != *"INT"* ]] || [[ "$output" == *"EXIT"* ]]
}

@test "trap: list shows nothing when no traps set" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap"
    [ "$status" -eq 0 ]
    # Output may be empty or show only default traps
}

@test "trap: list output can be re-executed" {
    skip "trap builtin not yet implemented"
    # The output of trap should be valid shell commands
    run "$COLONY" -c "trap 'echo hi' EXIT; trap | head -1"
    [ "$status" -eq 0 ]
    [[ "$output" == trap* ]] || [ -z "$output" ]
}

# ============================================================================
# Trap Inheritance in Subshells
# ============================================================================

@test "trap: subshell does not inherit trap" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap 'echo parent' EXIT; (echo subshell)"
    [ "$status" -eq 0 ]
    # Subshell exits without parent's trap
    [ "${lines[0]}" = "subshell" ]
    [ "${lines[1]}" = "parent" ]
}

@test "trap: subshell can set its own trap" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "(trap 'echo child-exit' EXIT; echo in-subshell)"
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "in-subshell" ]
    [ "${lines[1]}" = "child-exit" ]
}

@test "trap: ignored signals inherited by subshell" {
    skip "trap builtin not yet implemented"
    # When a signal is ignored (trap ''), subshells inherit that
    run "$COLONY" -c "trap '' INT; (trap -p INT)"
    [ "$status" -eq 0 ]
    # Subshell should show INT is ignored
}

@test "trap: command substitution inherits ignored signals" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap '' INT; echo \$(trap -p INT)"
    [ "$status" -eq 0 ]
}

@test "trap: pipeline components and trap inheritance" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap 'echo parent' EXIT; echo test | cat"
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "test" ]
    [ "${lines[1]}" = "parent" ]
}

# ============================================================================
# Trap in Functions
# ============================================================================

@test "trap: function can set trap" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "setup() { trap 'echo cleanup' EXIT; }; setup; echo main"
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "main" ]
    [ "${lines[1]}" = "cleanup" ]
}

@test "trap: trap set in function persists after return" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "set_trap() { trap 'echo done' EXIT; }; set_trap; echo continued"
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "continued" ]
    [ "${lines[1]}" = "done" ]
}

@test "trap: local trap in function" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "outer() { trap 'echo outer' EXIT; inner; }; inner() { trap 'echo inner' RETURN; }; outer"
    [ "$status" -eq 0 ]
}

@test "trap: function return triggers RETURN trap" {
    skip "trap RETURN not yet implemented"
    run "$COLONY" -c "myfunc() { trap 'echo ret' RETURN; echo in-func; }; myfunc; echo after"
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "in-func" ]
    [ "${lines[1]}" = "ret" ]
    [ "${lines[2]}" = "after" ]
}

# ============================================================================
# Multiple Signals - trap 'cmd' SIG1 SIG2
# ============================================================================

@test "trap: set same handler for multiple signals" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap 'echo caught' INT TERM HUP; echo done"
    [ "$status" -eq 0 ]
    [ "$output" = "done" ]
}

@test "trap: multiple signals in trap -p" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap 'echo x' INT TERM; trap -p INT TERM"
    [ "$status" -eq 0 ]
    [[ "$output" == *"INT"* ]]
    [[ "$output" == *"TERM"* ]]
}

@test "trap: reset multiple signals at once" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap 'echo x' INT TERM; trap - INT TERM; trap"
    [ "$status" -eq 0 ]
}

@test "trap: EXIT and ERR together" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap 'echo handler' EXIT ERR; echo done"
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "done" ]
    [ "${lines[1]}" = "handler" ]
}

# ============================================================================
# Edge Cases and Special Scenarios
# ============================================================================

@test "trap: trap in loop" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "for i in 1 2; do trap 'echo loop-exit' EXIT; done; echo after"
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "after" ]
    [ "${lines[1]}" = "loop-exit" ]
}

@test "trap: trap in conditional" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "if true; then trap 'echo cond-exit' EXIT; fi; echo done"
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "done" ]
    [ "${lines[1]}" = "cond-exit" ]
}

@test "trap: trap command can fail" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap 'exit 99' EXIT"
    [ "$status" -eq 99 ]
}

@test "trap: trap with complex command" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap 'for i in a b; do echo \$i; done' EXIT"
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "a" ]
    [ "${lines[1]}" = "b" ]
}

@test "trap: trap accesses shell variables" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "COUNT=0; trap 'echo count=\$COUNT' EXIT; COUNT=5; exit 0"
    [ "$status" -eq 0 ]
    [ "$output" = "count=5" ]
}

@test "trap: nested trap commands" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap 'trap - EXIT; echo first' EXIT; echo main"
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "main" ]
    [ "${lines[1]}" = "first" ]
}

@test "trap: trap with redirection" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap 'echo cleanup > $TEST_TEMP/trap-redir.txt' EXIT; exit 0"
    [ "$status" -eq 0 ]
    [ -f "$TEST_TEMP/trap-redir.txt" ]
}

@test "trap: trap with pipeline" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap 'echo one; echo two' EXIT | cat"
    [ "$status" -eq 0 ]
}

# ============================================================================
# Error Cases
# ============================================================================

@test "trap: invalid signal name fails" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap 'echo x' INVALID"
    [ "$status" -ne 0 ]
}

@test "trap: invalid signal number fails" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap 'echo x' 999"
    [ "$status" -ne 0 ]
}

@test "trap: cannot trap SIGKILL" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap 'echo x' KILL"
    [ "$status" -ne 0 ]
}

@test "trap: cannot trap SIGSTOP" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap 'echo x' STOP"
    [ "$status" -ne 0 ]
}

@test "trap: trap -l lists signals" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap -l"
    [ "$status" -eq 0 ]
    [[ "$output" == *"INT"* ]] || [[ "$output" == *"HUP"* ]]
}

# ============================================================================
# Bash Compatibility Comparisons
# ============================================================================

@test "compare: EXIT trap with bash" {
    skip "trap builtin not yet implemented"
    run bash -c "trap 'echo cleanup' EXIT; echo main"
    local bash_output="$output"

    run "$COLONY" -c "trap 'echo cleanup' EXIT; echo main"
    local colony_output="$output"

    [ "$colony_output" = "$bash_output" ]
}

@test "compare: trap list format with bash" {
    skip "trap builtin not yet implemented"
    run bash -c "trap 'echo test' EXIT; trap"
    local bash_output="$output"

    run "$COLONY" -c "trap 'echo test' EXIT; trap"
    local colony_output="$output"

    # Both should show EXIT trap
    [[ "$bash_output" == *"EXIT"* ]]
    [[ "$colony_output" == *"EXIT"* ]]
}

@test "compare: trap reset with bash" {
    skip "trap builtin not yet implemented"
    run bash -c "trap 'echo x' EXIT; trap - EXIT; echo done"
    local bash_output="$output"

    run "$COLONY" -c "trap 'echo x' EXIT; trap - EXIT; echo done"
    local colony_output="$output"

    [ "$colony_output" = "$bash_output" ]
}

@test "compare: multiple signal trap with bash" {
    skip "trap builtin not yet implemented"
    run bash -c "trap 'echo multi' INT TERM; echo set"
    local bash_output="$output"

    run "$COLONY" -c "trap 'echo multi' INT TERM; echo set"
    local colony_output="$output"

    [ "$colony_output" = "$bash_output" ]
}

@test "compare: subshell trap inheritance with bash" {
    skip "trap builtin not yet implemented"
    run bash -c "trap 'echo parent' EXIT; (echo child)"
    local bash_output="$output"

    run "$COLONY" -c "trap 'echo parent' EXIT; (echo child)"
    local colony_output="$output"

    [ "$colony_output" = "$bash_output" ]
}

@test "compare: function trap with bash" {
    skip "trap builtin not yet implemented"
    run bash -c "setup() { trap 'echo done' EXIT; }; setup; echo main"
    local bash_output="$output"

    run "$COLONY" -c "setup() { trap 'echo done' EXIT; }; setup; echo main"
    local colony_output="$output"

    [ "$colony_output" = "$bash_output" ]
}

# ============================================================================
# Special Variables in Traps
# ============================================================================

@test "trap: \$? available in EXIT trap" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap 'echo status=\$?' EXIT; exit 42"
    [ "$status" -eq 42 ]
    [ "$output" = "status=42" ]
}

@test "trap: \$LINENO in DEBUG trap" {
    skip "trap DEBUG not yet implemented"
    run "$COLONY" -c "trap 'echo line=\$LINENO' DEBUG; echo test"
    [ "$status" -eq 0 ]
    [[ "$output" == *"line="* ]]
}

@test "trap: \$BASH_COMMAND in DEBUG trap" {
    skip "trap DEBUG not yet implemented"
    run "$COLONY" -c 'trap '"'"'echo "cmd=$BASH_COMMAND"'"'"' DEBUG; echo hello'
    [ "$status" -eq 0 ]
    [[ "$output" == *"cmd="* ]]
}

# ============================================================================
# Trap and Job Control
# ============================================================================

@test "trap: trap with background job" {
    skip "trap builtin not yet implemented"
    run "$COLONY" -c "trap 'echo cleanup' EXIT; sleep 0.1 &; wait; echo done"
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "done" ]
    [ "${lines[1]}" = "cleanup" ]
}

@test "trap: SIGCHLD trap" {
    skip "trap SIGCHLD not yet implemented"
    run "$COLONY" -c "trap 'echo child-done' CHLD; sleep 0.1 &; wait"
    [ "$status" -eq 0 ]
    [[ "$output" == *"child-done"* ]]
}
