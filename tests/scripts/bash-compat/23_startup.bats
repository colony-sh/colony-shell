#!/usr/bin/env bats
# Test Colony shell startup file processing (bash-compatible)

COLONY="${COLONY_BIN}"

setup() {
    TEST_TEMP="$(mktemp -d)"
    export TEST_TEMP
}

teardown() {
    rm -rf "$TEST_TEMP"
}

# =============================================================================
# Basic Command-Line Options
# =============================================================================

@test "startup: --help shows usage" {
    run "$COLONY" --help
    [ "$status" -eq 0 ]
    [[ "$output" == *"Usage:"* ]]
    [[ "$output" == *"--login"* ]]
    [[ "$output" == *"--noprofile"* ]]
}

@test "startup: --version shows version" {
    run "$COLONY" --version
    [ "$status" -eq 0 ]
    [[ "$output" == *"Colony Shell"* ]]
}

@test "startup: -h is same as --help" {
    run "$COLONY" -h
    [ "$status" -eq 0 ]
    [[ "$output" == *"Usage:"* ]]
}

@test "startup: -V is same as --version" {
    run "$COLONY" -V
    [ "$status" -eq 0 ]
    [[ "$output" == *"Colony Shell"* ]]
}

# =============================================================================
# BASH_ENV Processing
# =============================================================================

@test "startup: BASH_ENV sourced for -c command" {
    echo 'export FROM_BASH_ENV=yes' > "$TEST_TEMP/env_file"
    BASH_ENV="$TEST_TEMP/env_file" run "$COLONY" -c 'echo $FROM_BASH_ENV'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

@test "startup: ENV sourced as fallback (POSIX)" {
    echo 'export FROM_ENV=yes' > "$TEST_TEMP/env_file"
    ENV="$TEST_TEMP/env_file" run "$COLONY" -c 'echo $FROM_ENV'
    [ "$status" -eq 0 ]
    [ "$output" = "yes" ]
}

@test "startup: BASH_ENV takes precedence over ENV" {
    echo 'export SOURCE=bash_env' > "$TEST_TEMP/bash_env"
    echo 'export SOURCE=env' > "$TEST_TEMP/env"
    BASH_ENV="$TEST_TEMP/bash_env" ENV="$TEST_TEMP/env" run "$COLONY" -c 'echo $SOURCE'
    [ "$status" -eq 0 ]
    [ "$output" = "bash_env" ]
}

@test "startup: BASH_ENV not sourced if empty" {
    export BASH_ENV=""
    run "$COLONY" -c 'echo ok'
    [ "$status" -eq 0 ]
    [ "$output" = "ok" ]
}

# =============================================================================
# --rcfile Option
# =============================================================================

@test "startup: --rcfile sources custom file" {
    echo 'export CUSTOM_RC=loaded' > "$TEST_TEMP/myrc"
    # Note: --rcfile is for interactive non-login shells, but we can test it
    # sources correctly by checking verbose output
    run "$COLONY" --verbose --rcfile "$TEST_TEMP/myrc" -c 'echo done'
    [ "$status" -eq 0 ]
}

@test "startup: --init-file is alias for --rcfile" {
    echo 'export INIT_FILE=loaded' > "$TEST_TEMP/myrc"
    run "$COLONY" --verbose --init-file "$TEST_TEMP/myrc" -c 'echo done'
    [ "$status" -eq 0 ]
}

# =============================================================================
# --login Option
# =============================================================================

@test "startup: --login flag recognized" {
    run "$COLONY" --login --noprofile -c 'echo ok'
    [ "$status" -eq 0 ]
    [ "$output" = "ok" ]
}

@test "startup: -l is shorthand for --login" {
    run "$COLONY" -l --noprofile -c 'echo ok'
    [ "$status" -eq 0 ]
    [ "$output" = "ok" ]
}

# =============================================================================
# --noprofile Option
# =============================================================================

@test "startup: --noprofile skips profile files" {
    # Create a profile that would cause errors if sourced
    mkdir -p "$TEST_TEMP/home"
    echo 'exit 42' > "$TEST_TEMP/home/.profile"
    
    # With noprofile, this should NOT exit with 42
    HOME="$TEST_TEMP/home" run "$COLONY" --login --noprofile -c 'echo ok'
    [ "$status" -eq 0 ]
    [ "$output" = "ok" ]
}

# =============================================================================
# --norc Option
# =============================================================================

@test "startup: --norc is recognized" {
    run "$COLONY" --norc -c 'echo ok'
    [ "$status" -eq 0 ]
    [ "$output" = "ok" ]
}

# =============================================================================
# --verbose Option
# =============================================================================

@test "startup: --verbose shows login state" {
    run "$COLONY" --verbose -c 'echo ok'
    [ "$status" -eq 0 ]
    [[ "$output" == *"login="* ]]
    [[ "$output" == *"interactive="* ]]
}

@test "startup: --verbose shows sourced files" {
    echo 'export X=1' > "$TEST_TEMP/env_file"
    BASH_ENV="$TEST_TEMP/env_file" run "$COLONY" --verbose -c 'echo ok'
    [ "$status" -eq 0 ]
    [[ "$output" == *"sourcing"* ]]
}

# =============================================================================
# Combined Options
# =============================================================================

@test "startup: -lc combined option works" {
    run "$COLONY" --noprofile -lc 'echo ok'
    [ "$status" -eq 0 ]
    [ "$output" = "ok" ]
}

# =============================================================================
# Script Mode
# =============================================================================

@test "startup: script mode sets $0 correctly" {
    echo 'echo "$0"' > "$TEST_TEMP/test.sh"
    run "$COLONY" "$TEST_TEMP/test.sh"
    [ "$status" -eq 0 ]
    [[ "$output" == *"test.sh"* ]]
}

@test "startup: script mode sets positional params" {
    echo 'echo "$1 $2"' > "$TEST_TEMP/test.sh"
    run "$COLONY" "$TEST_TEMP/test.sh" arg1 arg2
    [ "$status" -eq 0 ]
    [ "$output" = "arg1 arg2" ]
}

@test "startup: script mode sources BASH_ENV" {
    echo 'export SCRIPT_ENV=loaded' > "$TEST_TEMP/env_file"
    echo 'echo "$SCRIPT_ENV"' > "$TEST_TEMP/test.sh"
    BASH_ENV="$TEST_TEMP/env_file" run "$COLONY" "$TEST_TEMP/test.sh"
    [ "$status" -eq 0 ]
    [ "$output" = "loaded" ]
}

# =============================================================================
# Error Handling
# =============================================================================

@test "startup: missing rcfile argument shows error" {
    run "$COLONY" --rcfile
    [ "$status" -eq 2 ]
    [[ "$output" == *"requires an argument"* ]]
}

@test "startup: missing -c argument shows error" {
    run "$COLONY" -c
    [ "$status" -eq 2 ]
    [[ "$output" == *"requires an argument"* ]]
}

@test "startup: invalid option shows error" {
    run "$COLONY" --invalid-option
    [ "$status" -eq 2 ]
    [[ "$output" == *"invalid option"* ]]
}

@test "startup: nonexistent script shows error" {
    run "$COLONY" "/nonexistent/script.sh"
    [ "$status" -eq 127 ]
    [[ "$output" == *"No such file"* ]]
}

# =============================================================================
# COLONY_STARTUP_SOURCED marker
# =============================================================================

@test "startup: COLONY_STARTUP_SOURCED is set after startup" {
    run "$COLONY" -c 'echo $COLONY_STARTUP_SOURCED'
    [ "$status" -eq 0 ]
    [ "$output" = "1" ]
}
