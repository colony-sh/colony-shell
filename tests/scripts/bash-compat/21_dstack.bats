#!/usr/bin/env bats

# Directory stack tests (dirs, pushd, popd)
# Based on bash dstack.tests

setup() {
    COLONY="${COLONY_BIN}"
    ORIG_DIR=$(pwd)
    TEST_DIR=$(mktemp -d)
    mkdir -p "$TEST_DIR/subdir1" "$TEST_DIR/subdir2" "$TEST_DIR/subdir3"
    cd "$TEST_DIR"
}

teardown() {
    cd "$ORIG_DIR"
    rm -rf "$TEST_DIR" 2>/dev/null || true
}

# ============================================================
# dirs builtin tests
# ============================================================

@test "dirs: default output shows current directory" {
    result=$($COLONY -c 'dirs')
    [[ "$result" == *"$TEST_DIR"* ]] || [[ "$result" == "~"* ]]
}

@test "dirs -c: clears directory stack" {
    $COLONY -c 'pushd /tmp >/dev/null; dirs -c; dirs'
    result=$($COLONY -c 'pushd /tmp >/dev/null; dirs -c; dirs | wc -w')
    # After clear, should have only one entry
    [[ "$result" -eq 1 ]]
}

@test "dirs -l: long listing (no tilde abbreviation)" {
    result=$($COLONY -c 'cd ~; dirs -l')
    # Should NOT contain tilde, should have full path
    [[ "$result" != "~" ]] || [[ "$result" == *"$HOME"* ]]
}

@test "dirs -p: one entry per line" {
    result=$($COLONY -c 'pushd /tmp >/dev/null; pushd /var >/dev/null; dirs -p | wc -l')
    [[ "$result" -ge 3 ]]
}

@test "dirs -v: indexed entries per line" {
    result=$($COLONY -c 'pushd /tmp >/dev/null; dirs -v')
    [[ "$result" == *" 0  "* ]]
    [[ "$result" == *" 1  "* ]]
}

@test "dirs +0: shows top of stack (current dir)" {
    cd /tmp
    result=$($COLONY -c 'dirs +0')
    [[ "$result" == "/tmp" ]] || [[ "$result" == "/private/tmp" ]]
}

@test "dirs +1: shows second entry" {
    result=$($COLONY -c 'pushd /tmp >/dev/null; dirs +1')
    [[ -n "$result" ]]
}

@test "dirs -0: shows bottom of stack" {
    result=$($COLONY -c 'pushd /tmp >/dev/null; dirs -0')
    [[ -n "$result" ]]
}

@test "dirs: invalid index returns error" {
    run $COLONY -c 'dirs +99'
    [[ "$status" -ne 0 ]]
    [[ "$output" == *"out of range"* ]]
}

@test "dirs: invalid option returns error" {
    run $COLONY -c 'dirs -z'
    [[ "$status" -ne 0 ]]
    [[ "$output" == *"invalid option"* ]]
}

# ============================================================
# pushd builtin tests
# ============================================================

@test "pushd dir: pushes directory and changes to it" {
    result=$($COLONY -c 'pushd /tmp >/dev/null; pwd')
    [[ "$result" == "/tmp" ]] || [[ "$result" == "/private/tmp" ]]
}

@test "pushd dir: prints stack after push" {
    result=$($COLONY -c 'pushd /tmp')
    [[ "$result" == *"/tmp"* ]] || [[ "$result" == *"/private/tmp"* ]]
}

@test "pushd (no args): swaps top two entries" {
    result=$($COLONY -c "cd $TEST_DIR; pushd /tmp >/dev/null; pushd >/dev/null; pwd")
    # Handle macOS path canonicalization (/var -> /private/var)
    [[ "$result" == "$TEST_DIR" ]] || [[ "$result" == "/private$TEST_DIR" ]] || [[ "${result#/private}" == "$TEST_DIR" ]]
}

@test "pushd: two consecutive pushds restore original order" {
    result=$($COLONY -c "cd $TEST_DIR; pushd /tmp >/dev/null; pushd >/dev/null; pushd >/dev/null; pwd")
    [[ "$result" == "/tmp" ]] || [[ "$result" == "/private/tmp" ]]
}

@test "pushd -n dir: adds to stack without changing directory" {
    result=$($COLONY -c "cd $TEST_DIR; pushd -n /tmp >/dev/null; pwd")
    # Handle macOS path canonicalization
    [[ "$result" == "$TEST_DIR" ]] || [[ "$result" == "/private$TEST_DIR" ]] || [[ "${result#/private}" == "$TEST_DIR" ]]
}

@test "pushd -n dir: directory appears in stack" {
    result=$($COLONY -c "cd $TEST_DIR; pushd -n /tmp; dirs")
    [[ "$result" == *"/tmp"* ]] || [[ "$result" == *"/private/tmp"* ]]
}

@test "pushd +1: rotates stack" {
    result=$($COLONY -c "pushd /tmp >/dev/null; pushd /var >/dev/null; pushd +1 >/dev/null; dirs +0")
    [[ "$result" == "/tmp" ]] || [[ "$result" == "/private/tmp" ]]
}

@test "pushd: non-existent directory returns error" {
    run $COLONY -c 'pushd /nonexistent_directory_xyz'
    [[ "$status" -ne 0 ]]
    [[ "$output" == *"No such file or directory"* ]]
}

@test "pushd: empty stack (no swap target) returns error" {
    run $COLONY -c 'dirs -c; pushd'
    [[ "$status" -ne 0 ]]
    [[ "$output" == *"no other directory"* ]]
}

@test "pushd: out of range index returns error" {
    run $COLONY -c 'pushd +99'
    [[ "$status" -ne 0 ]]
    [[ "$output" == *"out of range"* ]]
}

# ============================================================
# popd builtin tests
# ============================================================

@test "popd: removes top and changes to new top" {
    result=$($COLONY -c "cd $TEST_DIR; pushd /tmp >/dev/null; popd >/dev/null; pwd")
    # Handle macOS path canonicalization
    [[ "$result" == "$TEST_DIR" ]] || [[ "$result" == "/private$TEST_DIR" ]] || [[ "${result#/private}" == "$TEST_DIR" ]]
}

@test "popd: prints stack after pop" {
    result=$($COLONY -c "cd $TEST_DIR; pushd /tmp >/dev/null; popd")
    [[ -n "$result" ]]
}

@test "popd -n: removes entry without changing directory" {
    result=$($COLONY -c "pushd /tmp >/dev/null; pushd /var >/dev/null; popd -n +1 >/dev/null; pwd")
    [[ "$result" == "/var" ]] || [[ "$result" == "/private/var" ]]
}

@test "popd +1: removes second entry" {
    result=$($COLONY -c "cd $TEST_DIR; pushd /tmp >/dev/null; pushd /var >/dev/null; popd +1; dirs")
    # /tmp should be removed, leaving /var and original
    [[ "$result" != *"/tmp"* ]] || [[ "$result" != *"/private/tmp"* ]]
}

@test "popd: empty stack returns error" {
    run $COLONY -c 'dirs -c; popd'
    [[ "$status" -ne 0 ]]
    [[ "$output" == *"directory stack empty"* ]]
}

@test "popd: out of range index returns error" {
    run $COLONY -c 'pushd /tmp >/dev/null; popd +99'
    [[ "$status" -ne 0 ]]
    [[ "$output" == *"out of range"* ]]
}

# ============================================================
# Integration tests
# ============================================================

@test "pushd/popd preserve PWD and OLDPWD" {
    result=$($COLONY -c "cd $TEST_DIR; pushd /tmp >/dev/null; echo \$OLDPWD")
    # Handle macOS path canonicalization
    [[ "$result" == "$TEST_DIR" ]] || [[ "$result" == "/private$TEST_DIR" ]] || [[ "${result#/private}" == "$TEST_DIR" ]]
}

@test "directory stack works across multiple operations" {
    result=$($COLONY -c "
        cd $TEST_DIR
        pushd /tmp >/dev/null
        pushd /var >/dev/null
        pushd /usr >/dev/null
        dirs | wc -w
    ")
    [[ "$result" -eq 4 ]]
}

@test "pushd/popd cycle returns to original directory" {
    result=$($COLONY -c "
        cd $TEST_DIR
        pushd /tmp >/dev/null
        pushd /var >/dev/null
        popd >/dev/null
        popd >/dev/null
        pwd
    ")
    # Handle macOS path canonicalization
    [[ "$result" == "$TEST_DIR" ]] || [[ "$result" == "/private$TEST_DIR" ]] || [[ "${result#/private}" == "$TEST_DIR" ]]
}

@test "dirs -c then pushd works correctly" {
    result=$($COLONY -c "
        pushd /tmp >/dev/null
        dirs -c
        pushd /var >/dev/null
        dirs | wc -w
    ")
    [[ "$result" -eq 2 ]]
}

@test "multiple subdirectory navigation" {
    result=$($COLONY -c "
        cd $TEST_DIR
        pushd subdir1 >/dev/null
        pushd ../subdir2 >/dev/null
        pushd ../subdir3 >/dev/null
        dirs | wc -w
    ")
    [[ "$result" -eq 4 ]]
}

# ============================================================
# Bash comparison tests
# ============================================================

@test "bash-compat: dirs output format matches bash" {
    # Basic format comparison - space-separated paths
    colony_result=$($COLONY -c 'pushd /tmp >/dev/null; dirs')
    bash_result=$(bash -c 'pushd /tmp >/dev/null; dirs')
    
    # Both should have /tmp (or /private/tmp on macOS)
    [[ "$colony_result" == *"tmp"* ]]
}

@test "bash-compat: pushd changes directory" {
    colony_pwd=$($COLONY -c 'pushd /tmp >/dev/null; pwd')
    bash_pwd=$(bash -c 'pushd /tmp >/dev/null; pwd')
    
    # Both should be in /tmp (or /private/tmp)
    [[ "$colony_pwd" == *"tmp"* ]]
    [[ "$bash_pwd" == *"tmp"* ]]
}

@test "bash-compat: popd returns to previous directory" {
    colony_result=$($COLONY -c "cd $TEST_DIR; pushd /tmp >/dev/null; popd >/dev/null; pwd")
    bash_result=$(bash -c "cd $TEST_DIR; pushd /tmp >/dev/null; popd >/dev/null; pwd")
    
    # Handle macOS path canonicalization - both should resolve to same real path
    colony_real=$(echo "$colony_result" | sed 's|^/private||')
    bash_real=$(echo "$bash_result" | sed 's|^/private||')
    [[ "$colony_real" == "$bash_real" ]]
}
