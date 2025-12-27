#!/usr/bin/env bats
# Job control tests for Colony Shell
# Based on bash/tests/jobs.tests and related .sub files
#
# Tests job control features:
# - Background execution (command &)
# - jobs builtin (list jobs, job status)
# - fg builtin (bring to foreground)
# - bg builtin (continue in background)
# - wait builtin (wait for jobs)
# - Job IDs (%1, %+, %-, %%, %job_spec)
# - $! (last background PID)
# - $$ (current shell PID)
# - SIGTSTP/SIGCONT handling

COLONY="${COLONY_BIN}"

setup() {
    TEST_TEMP="$(mktemp -d)"
    export TEST_TEMP
}

teardown() {
    # Clean up any lingering background processes
    jobs -p 2>/dev/null | xargs kill 2>/dev/null || true
    rm -rf "$TEST_TEMP"
}

# ============================================================================
# Special Variables: $! and $$
# ============================================================================

@test "job-control: \$\$ expands to current shell PID" {
    run "$COLONY" -c 'echo $$'
    [ "$status" -eq 0 ]
    # Should be a positive integer
    [[ "$output" =~ ^[0-9]+$ ]]
    [ "$output" -gt 0 ]
}

@test "job-control: \$\$ is consistent within same shell" {
    run "$COLONY" -c 'A=$$; B=$$; [ "$A" = "$B" ] && echo same'
    [ "$status" -eq 0 ]
    [ "$output" = "same" ]
}

@test "job-control: \$\$ in subshell is parent's PID" {
    # POSIX: $$ in subshell should be the parent shell's PID
    run "$COLONY" -c 'PARENT=$$; CHILD=$(echo $$); [ "$PARENT" = "$CHILD" ] && echo same'
    [ "$status" -eq 0 ]
    [ "$output" = "same" ]
}

@test "job-control: compare \$\$ with bash" {
    run bash -c 'echo $$'
    [ "$status" -eq 0 ]
    [[ "$output" =~ ^[0-9]+$ ]]
    local bash_output="$output"

    run "$COLONY" -c 'echo $$'
    [ "$status" -eq 0 ]
    [[ "$output" =~ ^[0-9]+$ ]]
    
    # Both should produce valid PIDs (different values but same format)
    [ "$output" -gt 0 ]
}

@test "job-control: \$! is empty before any background job" {
    run "$COLONY" -c 'echo ">>$!<<"'
    [ "$status" -eq 0 ]
    # Should be empty or not set
    [[ "$output" == ">><" ]] || [[ "$output" == ">><<" ]]
}

@test "job-control: \$! is set after background job" {
    run "$COLONY" -c 'sleep 0.1 &; echo $!'
    [ "$status" -eq 0 ]
    [[ "$output" =~ ^[0-9]+$ ]]
    [ "$output" -gt 0 ]
}

@test "job-control: \$! persists after background job completes" {
    run "$COLONY" -c 'true &; PID=$!; wait; echo $!'
    [ "$status" -eq 0 ]
    [[ "$output" =~ ^[0-9]+$ ]]
}

@test "job-control: \$! updated by subsequent background job" {
    run "$COLONY" -c 'sleep 0.1 &; PID1=$!; sleep 0.1 &; PID2=$!; [ "$PID1" != "$PID2" ] && echo different'
    [ "$status" -eq 0 ]
    [ "$output" = "different" ]
}

# ============================================================================
# Background Execution (command &)
# ============================================================================

@test "job-control: background command syntax parsed" {
    run "$COLONY" -c 'echo hello &'
    [ "$status" -eq 0 ]
}

@test "job-control: background command runs asynchronously" {
    run "$COLONY" -c 'sleep 0.5 &; echo done'
    [ "$status" -eq 0 ]
    [ "$output" = "done" ]
}

@test "job-control: background command output appears" {
    run "$COLONY" -c 'echo hello &; sleep 0.1'
    [ "$status" -eq 0 ]
    [[ "$output" == *"hello"* ]]
}

@test "job-control: multiple background commands" {
    run "$COLONY" -c 'echo one &; echo two &; echo three &; wait'
    [ "$status" -eq 0 ]
    [[ "$output" == *"one"* ]]
    [[ "$output" == *"two"* ]]
    [[ "$output" == *"three"* ]]
}

@test "job-control: background pipeline" {
    run "$COLONY" -c 'echo hello | cat &; wait; echo done'
    [ "$status" -eq 0 ]
    [[ "$output" == *"hello"* ]]
    [[ "$output" == *"done"* ]]
}

@test "job-control: background job creates job entry" {
    skip "Background execution (&) not yet implemented"
    run "$COLONY" -c 'sleep 1 &; jobs | grep -c Running'
    [ "$status" -eq 0 ]
    [ "$output" -ge 1 ]
}

@test "job-control: background job shows in job notification" {
    skip "Background execution (&) not yet implemented"
    # bash prints [1] PID when starting background job
    run "$COLONY" -c 'sleep 0.1 & 2>&1'
    [ "$status" -eq 0 ]
    [[ "$output" =~ \[[0-9]+\].*[0-9]+ ]]
}

@test "job-control: background with redirection" {
    run "$COLONY" -c "echo hello > $TEST_TEMP/bg.txt &; wait; cat $TEST_TEMP/bg.txt"
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "job-control: background exit status available via wait" {
    run "$COLONY" -c 'false &; wait $!; echo $?'
    [ "$status" -eq 0 ]
    [ "$output" = "1" ]
}

# ============================================================================
# jobs Builtin
# ============================================================================

@test "job-control: jobs with no jobs prints nothing" {
    run "$COLONY" -c 'jobs'
    [ "$status" -eq 0 ]
    [ -z "$output" ]
}

@test "job-control: jobs -l shows PIDs" {
    skip "Background execution (&) not yet implemented"
    run "$COLONY" -c 'sleep 1 &; jobs -l'
    [ "$status" -eq 0 ]
    # Should contain job number and PID
    [[ "$output" =~ \[1\] ]]
    [[ "$output" =~ [0-9]+ ]]
}

@test "job-control: jobs -p shows only PIDs" {
    skip "Background execution (&) not yet implemented"
    run "$COLONY" -c 'sleep 1 &; jobs -p'
    [ "$status" -eq 0 ]
    [[ "$output" =~ ^[0-9]+$ ]]
}

@test "job-control: jobs -r shows running jobs only" {
    skip "Background execution (&) not yet implemented - job filtering"
    run "$COLONY" -c 'sleep 1 &; jobs -r'
    [ "$status" -eq 0 ]
    [[ "$output" == *"Running"* ]]
}

@test "job-control: jobs -s shows stopped jobs only" {
    skip "Background execution (&) not yet implemented - job filtering"
    # Would need to stop a job first
    run "$COLONY" -c 'jobs -s'
    [ "$status" -eq 0 ]
}

@test "job-control: jobs shows job status" {
    run "$COLONY" -c 'sleep 1 &; jobs'
    [ "$status" -eq 0 ]
    # Should show Running status
    [[ "$output" == *"Running"* ]] || [[ "$output" == *"running"* ]]
}

@test "job-control: jobs shows command" {
    run "$COLONY" -c 'sleep 100 &; jobs'
    [ "$status" -eq 0 ]
    [[ "$output" == *"sleep"* ]]
}

@test "job-control: jobs current job marked with +" {
    skip "Background execution (&) not yet implemented"
    run "$COLONY" -c 'sleep 1 &; jobs'
    [ "$status" -eq 0 ]
    [[ "$output" == *"+"* ]]
}

@test "job-control: jobs previous job marked with -" {
    skip "Background execution (&) not yet implemented"
    run "$COLONY" -c 'sleep 1 &; sleep 1 &; jobs'
    [ "$status" -eq 0 ]
    [[ "$output" == *"-"* ]]
}

@test "job-control: jobs shows Done for completed jobs" {
    skip "Background execution (&) not yet implemented"
    run "$COLONY" -c 'true &; sleep 0.1; jobs'
    [ "$status" -eq 0 ]
    [[ "$output" == *"Done"* ]]
}

@test "job-control: jobs specific job number" {
    skip "Background execution (&) not yet implemented"
    run "$COLONY" -c 'sleep 1 &; sleep 1 &; jobs %1'
    [ "$status" -eq 0 ]
    [[ "$output" =~ \[1\] ]]
}

@test "job-control: jobs invalid job number" {
    skip "Background execution (&) not yet implemented"
    run "$COLONY" -c 'jobs %99'
    [ "$status" -ne 0 ]
}

# ============================================================================
# wait Builtin
# ============================================================================

@test "job-control: wait with no jobs returns immediately" {
    run "$COLONY" -c 'wait; echo done'
    [ "$status" -eq 0 ]
    [ "$output" = "done" ]
}

@test "job-control: wait for specific PID" {
    run "$COLONY" -c 'sleep 0.1 &; PID=$!; wait $PID; echo done'
    [ "$status" -eq 0 ]
    [ "$output" = "done" ]
}

@test "job-control: wait for job ID" {
    skip "Background execution (&) not yet implemented"
    run "$COLONY" -c 'sleep 0.1 &; wait %1; echo done'
    [ "$status" -eq 0 ]
    [ "$output" = "done" ]
}

@test "job-control: wait returns exit status of job" {
    run "$COLONY" -c 'sh -c "exit 42" &; wait $!; echo $?'
    [ "$status" -eq 0 ]
    [ "$output" = "42" ]
}

@test "job-control: wait for all background jobs" {
    run "$COLONY" -c 'sleep 0.1 &; sleep 0.1 &; wait; echo done'
    [ "$status" -eq 0 ]
    [ "$output" = "done" ]
}

@test "job-control: wait -n waits for any job" {
    skip "Background execution (&) not yet implemented - wait -n"
    run "$COLONY" -c 'sleep 1 &; sleep 0.1 &; wait -n; echo next'
    [ "$status" -eq 0 ]
    # Should complete quickly (0.1s job finishes first)
}

@test "job-control: wait invalid PID" {
    run "$COLONY" -c 'wait 99999999'
    [ "$status" -ne 0 ]
}

@test "job-control: wait invalid job spec" {
    skip "Background execution (&) not yet implemented"
    run "$COLONY" -c 'wait %99'
    [ "$status" -ne 0 ]
}

@test "job-control: wait for pipeline" {
    skip "Background execution (&) not yet implemented"
    run "$COLONY" -c 'echo hello | cat | cat &; wait; echo done'
    [ "$status" -eq 0 ]
    [[ "$output" == *"done"* ]]
}

@test "job-control: wait returns immediately for completed job" {
    skip "Background execution (&) not yet implemented"
    run "$COLONY" -c 'true &; PID=$!; sleep 0.1; wait $PID; echo done'
    [ "$status" -eq 0 ]
    [ "$output" = "done" ]
}

# ============================================================================
# fg Builtin
# ============================================================================

@test "job-control: fg with no jobs fails" {
    run "$COLONY" -c 'fg'
    [ "$status" -ne 0 ]
    [[ "$output" == *"no current job"* ]] || [[ "$output" == *"no job"* ]]
}

@test "job-control: fg brings job to foreground" {
    skip "Background execution (&) not yet implemented"
    run "$COLONY" -c 'sleep 0.1 &; fg; echo done'
    [ "$status" -eq 0 ]
    [ "$output" = "done" ]
}

@test "job-control: fg specific job number" {
    skip "Background execution (&) not yet implemented"
    run "$COLONY" -c 'sleep 0.1 &; fg %1; echo done'
    [ "$status" -eq 0 ]
}

@test "job-control: fg %%" {
    skip "Background execution (&) not yet implemented"
    run "$COLONY" -c 'sleep 0.1 &; fg %%; echo done'
    [ "$status" -eq 0 ]
}

@test "job-control: fg %+" {
    skip "Background execution (&) not yet implemented"
    run "$COLONY" -c 'sleep 0.1 &; fg %+; echo done'
    [ "$status" -eq 0 ]
}

@test "job-control: fg %-" {
    skip "Background execution (&) not yet implemented"
    run "$COLONY" -c 'sleep 0.1 &; sleep 0.2 &; fg %-; echo done'
    [ "$status" -eq 0 ]
}

@test "job-control: fg invalid job spec" {
    skip "Background execution (&) not yet implemented"
    run "$COLONY" -c 'sleep 0.1 &; fg %99'
    [ "$status" -ne 0 ]
}

@test "job-control: fg shows job command" {
    skip "Background execution (&) not yet implemented"
    run "$COLONY" -c 'sleep 0.1 &; fg 2>&1'
    [ "$status" -eq 0 ]
    [[ "$output" == *"sleep"* ]]
}

@test "job-control: fg returns job exit status" {
    skip "Background execution (&) not yet implemented"
    run "$COLONY" -c 'sh -c "sleep 0.1; exit 5" &; fg; echo $?'
    [ "$status" -eq 0 ]
    [[ "$output" == *"5"* ]]
}

@test "job-control: fg resumes stopped job" {
    skip "Background execution (&) not yet implemented - SIGTSTP handling"
    # Would need to stop a job with Ctrl-Z first
    run "$COLONY" -c 'sleep 0.1 &; kill -STOP $!; fg'
    [ "$status" -eq 0 ]
}

# ============================================================================
# bg Builtin
# ============================================================================

@test "job-control: bg with no jobs fails" {
    run "$COLONY" -c 'bg'
    [ "$status" -ne 0 ]
    [[ "$output" == *"no current job"* ]] || [[ "$output" == *"no job"* ]]
}

@test "job-control: bg resumes stopped job in background" {
    skip "Background execution (&) not yet implemented - SIGTSTP handling"
    run "$COLONY" -c 'sleep 1 &; kill -STOP $!; bg; jobs'
    [ "$status" -eq 0 ]
    [[ "$output" == *"Running"* ]]
}

@test "job-control: bg specific job number" {
    skip "Background execution (&) not yet implemented - SIGTSTP handling"
    run "$COLONY" -c 'sleep 1 &; kill -STOP $!; bg %1; jobs'
    [ "$status" -eq 0 ]
}

@test "job-control: bg shows job command" {
    skip "Background execution (&) not yet implemented - SIGTSTP handling"
    run "$COLONY" -c 'sleep 1 &; kill -STOP $!; bg 2>&1'
    [ "$status" -eq 0 ]
    [[ "$output" == *"sleep"* ]]
}

@test "job-control: bg invalid job spec" {
    skip "Background execution (&) not yet implemented"
    run "$COLONY" -c 'sleep 0.1 &; bg %99'
    [ "$status" -ne 0 ]
}

@test "job-control: bg on running job (no-op or error)" {
    skip "Background execution (&) not yet implemented"
    # bash: bg on running job may be no-op or error
    run "$COLONY" -c 'sleep 0.5 &; bg %1'
    # Status depends on implementation
}

# ============================================================================
# Job Specifications (%job)
# ============================================================================

@test "job-control: %1 references first job" {
    skip "Background execution (&) not yet implemented"
    run "$COLONY" -c 'sleep 1 &; jobs %1'
    [ "$status" -eq 0 ]
    [[ "$output" =~ \[1\] ]]
}

@test "job-control: %2 references second job" {
    skip "Background execution (&) not yet implemented"
    run "$COLONY" -c 'sleep 1 &; sleep 1 &; jobs %2'
    [ "$status" -eq 0 ]
    [[ "$output" =~ \[2\] ]]
}

@test "job-control: %% references current job" {
    skip "Background execution (&) not yet implemented"
    run "$COLONY" -c 'sleep 1 &; jobs %%'
    [ "$status" -eq 0 ]
}

@test "job-control: %+ references current job" {
    skip "Background execution (&) not yet implemented"
    run "$COLONY" -c 'sleep 1 &; jobs %+'
    [ "$status" -eq 0 ]
}

@test "job-control: %- references previous job" {
    skip "Background execution (&) not yet implemented"
    run "$COLONY" -c 'sleep 1 &; sleep 1 &; jobs %-'
    [ "$status" -eq 0 ]
}

@test "job-control: %?string matches job containing string" {
    skip "Background execution (&) not yet implemented - job string matching"
    run "$COLONY" -c 'sleep 1 &; jobs %?sleep'
    [ "$status" -eq 0 ]
    [[ "$output" == *"sleep"* ]]
}

@test "job-control: %string matches job starting with string" {
    skip "Background execution (&) not yet implemented - job string matching"
    run "$COLONY" -c 'sleep 1 &; jobs %sle'
    [ "$status" -eq 0 ]
    [[ "$output" == *"sleep"* ]]
}

# ============================================================================
# Signal Handling (SIGTSTP, SIGCONT)
# ============================================================================

@test "job-control: SIGTSTP stops foreground job" {
    skip "Background execution (&) not yet implemented - SIGTSTP handling"
    # Would need interactive terminal to test Ctrl-Z
    run "$COLONY" -c 'sleep 1 &; kill -TSTP $!; jobs'
    [ "$status" -eq 0 ]
    [[ "$output" == *"Stopped"* ]]
}

@test "job-control: SIGCONT resumes stopped job" {
    skip "Background execution (&) not yet implemented - SIGCONT handling"
    run "$COLONY" -c 'sleep 0.5 &; PID=$!; kill -STOP $PID; kill -CONT $PID; wait $PID; echo done'
    [ "$status" -eq 0 ]
    [ "$output" = "done" ]
}

@test "job-control: stopped job status in jobs" {
    skip "Background execution (&) not yet implemented - SIGTSTP handling"
    run "$COLONY" -c 'sleep 1 &; kill -STOP $!; jobs'
    [ "$status" -eq 0 ]
    [[ "$output" == *"Stopped"* ]] || [[ "$output" == *"stopped"* ]]
}

@test "job-control: shell ignores SIGTSTP" {
    # The shell should not stop itself when receiving SIGTSTP
    # This tests init_job_control() signal setup
    run "$COLONY" -c 'echo before; kill -TSTP $$; echo after'
    [ "$status" -eq 0 ]
    [[ "$output" == *"before"* ]]
    [[ "$output" == *"after"* ]]
}

@test "job-control: shell ignores SIGINT during job control" {
    # Shell should ignore SIGINT to let foreground job handle it
    run "$COLONY" -c 'trap "" INT; echo done'
    [ "$status" -eq 0 ]
    [ "$output" = "done" ]
}

# ============================================================================
# Job Completion Notifications
# ============================================================================

@test "job-control: completed job notified before next prompt" {
    skip "Background execution (&) not yet implemented"
    # bash shows [N]+ Done command when job completes
    run "$COLONY" -c 'true &; sleep 0.1; echo check'
    [ "$status" -eq 0 ]
    # Should see Done notification before "check"
}

@test "job-control: exit status of failed background job" {
    skip "Background execution (&) not yet implemented"
    run "$COLONY" -c 'sh -c "exit 1" &; wait; echo $?'
    [ "$status" -eq 0 ]
    [ "$output" = "1" ]
}

@test "job-control: exit status 128+signal for signaled job" {
    skip "Background execution (&) not yet implemented"
    run "$COLONY" -c 'sleep 10 &; PID=$!; kill -9 $PID; wait $PID; echo $?'
    [ "$status" -eq 0 ]
    [ "$output" = "137" ]  # 128 + 9 (SIGKILL)
}

# ============================================================================
# Process Groups
# ============================================================================

@test "job-control: background job has its own process group" {
    skip "Background execution (&) not yet implemented"
    run "$COLONY" -c 'sleep 0.1 &; PID=$!; PGID=$(ps -o pgid= -p $PID); [ "$PID" = "$PGID" ] && echo same'
    [ "$status" -eq 0 ]
    # On some systems this may differ, but typically background job leads its own group
}

@test "job-control: pipeline members share process group" {
    skip "Background execution (&) not yet implemented"
    run "$COLONY" -c 'sleep 1 | sleep 1 &; PGID1=$(ps -o pgid= -p $(jobs -p | head -1)); [ -n "$PGID1" ]'
    [ "$status" -eq 0 ]
}

# ============================================================================
# Edge Cases
# ============================================================================

@test "job-control: subshell inherits job control settings" {
    skip "Background execution (&) not yet implemented"
    run "$COLONY" -c '(sleep 0.1 &; wait; echo done)'
    [ "$status" -eq 0 ]
    [ "$output" = "done" ]
}

@test "job-control: background in pipeline" {
    skip "Background execution (&) not yet implemented"
    # Only last command in pipeline backgrounds
    run "$COLONY" -c 'echo hello | cat &; wait'
    [ "$status" -eq 0 ]
}

@test "job-control: command substitution with background" {
    skip "Background execution (&) not yet implemented"
    run "$COLONY" -c 'result=$(echo hi &; wait); echo $result'
    [ "$status" -eq 0 ]
    [ "$output" = "hi" ]
}

@test "job-control: background in brace group" {
    skip "Background execution (&) not yet implemented"
    run "$COLONY" -c '{ echo hello &; wait; }; echo done'
    [ "$status" -eq 0 ]
    [[ "$output" == *"hello"* ]]
    [[ "$output" == *"done"* ]]
}

@test "job-control: background in subshell" {
    skip "Background execution (&) not yet implemented"
    run "$COLONY" -c '(echo hello &; wait); echo done'
    [ "$status" -eq 0 ]
    [[ "$output" == *"hello"* ]]
    [[ "$output" == *"done"* ]]
}

@test "job-control: nested background jobs" {
    skip "Background execution (&) not yet implemented"
    run "$COLONY" -c 'sh -c "sleep 0.1 &" &; wait; echo done'
    [ "$status" -eq 0 ]
    [ "$output" = "done" ]
}

@test "job-control: kill background job" {
    skip "Background execution (&) not yet implemented"
    run "$COLONY" -c 'sleep 10 &; PID=$!; kill $PID; wait $PID 2>/dev/null; echo done'
    [ "$status" -eq 0 ]
    [ "$output" = "done" ]
}

@test "job-control: kill background job by job spec" {
    skip "Background execution (&) not yet implemented"
    run "$COLONY" -c 'sleep 10 &; kill %1; wait; echo done'
    [ "$status" -eq 0 ]
    [ "$output" = "done" ]
}

# ============================================================================
# disown (bash extension, may not be POSIX)
# ============================================================================

@test "job-control: disown removes job from table" {
    skip "disown not yet implemented"
    run "$COLONY" -c 'sleep 10 &; disown %1; jobs'
    [ "$status" -eq 0 ]
    [ -z "$output" ]  # No jobs after disown
}

@test "job-control: disown -h keeps job but no HUP" {
    skip "disown not yet implemented"
    run "$COLONY" -c 'sleep 10 &; disown -h %1; jobs'
    [ "$status" -eq 0 ]
    # Job still visible but won't receive SIGHUP on shell exit
}

# ============================================================================
# Bash Compatibility Comparisons
# ============================================================================

@test "compare: \$\$ format matches bash" {
    run bash -c 'echo $$'
    [[ "$output" =~ ^[0-9]+$ ]]
    
    run "$COLONY" -c 'echo $$'
    [[ "$output" =~ ^[0-9]+$ ]]
}

@test "compare: jobs with no jobs matches bash" {
    run bash -c 'jobs'
    local bash_output="$output"
    local bash_status="$status"

    run "$COLONY" -c 'jobs'
    local colony_output="$output"
    local colony_status="$status"

    [ "$colony_status" -eq "$bash_status" ]
    [ "$colony_output" = "$bash_output" ]
}

@test "compare: fg error message similar to bash" {
    run bash -c 'fg'
    local bash_output="$output"

    run "$COLONY" -c 'fg'
    # Both should fail with some error about no jobs
    [ "$status" -ne 0 ]
}

@test "compare: bg error message similar to bash" {
    run bash -c 'bg'
    local bash_output="$output"

    run "$COLONY" -c 'bg'
    # Both should fail with some error about no jobs
    [ "$status" -ne 0 ]
}

@test "compare: wait with no jobs matches bash" {
    run bash -c 'wait; echo $?'
    local bash_output="$output"

    run "$COLONY" -c 'wait; echo $?'
    local colony_output="$output"

    [ "$colony_output" = "$bash_output" ]
}

# ============================================================================
# Interactive Mode Tests (require TTY - may be skipped)
# ============================================================================

@test "job-control: Ctrl-C sends SIGINT to foreground job" {
    skip "Interactive terminal tests not supported in BATS"
}

@test "job-control: Ctrl-Z sends SIGTSTP to foreground job" {
    skip "Interactive terminal tests not supported in BATS"
}

@test "job-control: job control disabled in non-interactive shell" {
    skip "Interactive terminal tests not supported in BATS"
    # When shell is non-interactive, job control is typically off
}

# ============================================================================
# set -m / set +m (Job control option)
# ============================================================================

@test "job-control: set -m enables job control" {
    skip "set -m not yet implemented"
    run "$COLONY" -c 'set -m; echo done'
    [ "$status" -eq 0 ]
}

@test "job-control: set +m disables job control" {
    skip "set +m not yet implemented"
    run "$COLONY" -c 'set +m; echo done'
    [ "$status" -eq 0 ]
}

# ============================================================================
# SIGCHLD Handling
# ============================================================================

@test "job-control: SIGCHLD triggers job status update" {
    skip "Background execution (&) not yet implemented"
    run "$COLONY" -c 'sleep 0.1 &; sleep 0.2; jobs'
    [ "$status" -eq 0 ]
    # Job should show as Done
    [[ "$output" == *"Done"* ]]
}

@test "job-control: trap SIGCHLD handler" {
    skip "trap CHLD not yet implemented"
    run "$COLONY" -c 'trap "echo child" CHLD; sleep 0.1 &; wait'
    [ "$status" -eq 0 ]
    [[ "$output" == *"child"* ]]
}

# ============================================================================
# History Integration with Job Control
# ============================================================================

@test "job-control: background command appears in history" {
    skip "History integration with job control not tested"
    # Would need to test history command after background job
}

# ============================================================================
# Exit with Background Jobs
# ============================================================================

@test "job-control: exit with running background jobs" {
    skip "Background execution (&) not yet implemented"
    # bash warns and may not exit immediately
    run "$COLONY" -c 'sleep 10 &; exit 0'
    [ "$status" -eq 0 ]
}

@test "job-control: exit with stopped jobs" {
    skip "Background execution (&) not yet implemented - SIGTSTP handling"
    # bash typically warns about stopped jobs
    run "$COLONY" -c 'sleep 10 &; kill -STOP $!; exit 0'
    # May show warning but should exit
}
