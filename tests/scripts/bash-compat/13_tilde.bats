#!/usr/bin/env bats
# Test suite for tilde expansion
# Based on bash tilde.tests and tilde2.tests from github.com/bminor/bash

SCRIPT_DIR="$(cd "$(dirname "$BATS_TEST_FILENAME")" && pwd)"
COLONY="${COLONY_BIN:-$SCRIPT_DIR/../../../target/debug/colony}"

run_colony() {
    run "$COLONY" -c "$1"
}

# =============================================================================
# Basic tilde expansion
# =============================================================================

@test "tilde: ~ expands to HOME" {
    run_colony 'HOME=/usr/xyz; echo ~'
    [ "$status" -eq 0 ]
    [ "$output" = "/usr/xyz" ]
}

@test "tilde: ~/foo expands to HOME/foo" {
    run_colony 'HOME=/usr/xyz; echo ~/foo'
    [ "$status" -eq 0 ]
    [ "$output" = "/usr/xyz/foo" ]
}

@test "tilde: ~/path with quotes around path part" {
    run_colony 'HOME=/usr/xyz; echo ~/"foo"'
    [ "$status" -eq 0 ]
    [ "$output" = "/usr/xyz/foo" ]
}

# =============================================================================
# Tilde in assignments
# =============================================================================

@test "tilde: expands in unquoted assignment" {
    run_colony 'HOME=/usr/xyz; SHELL=~/bash; echo $SHELL'
    [ "$status" -eq 0 ]
    [ "$output" = "/usr/xyz/bash" ]
}

@test "tilde: expands in path-style assignment" {
    run_colony 'HOME=/usr/xyz; path=/usr/ucb:/bin:~/bin:~/tmp/bin:/usr/bin; echo $path'
    [ "$status" -eq 0 ]
    [ "$output" = "/usr/ucb:/bin:/usr/xyz/bin:/usr/xyz/tmp/bin:/usr/bin" ]
}

@test "tilde: expands after colon in assignment" {
    run_colony 'HOME=/usr/xyz; XPATH=/bin:/usr/bin:.; PPATH=$XPATH:~/bin; echo $PPATH'
    [ "$status" -eq 0 ]
    [ "$output" = "/bin:/usr/bin:.::/usr/xyz/bin" ] || [ "$output" = "/bin:/usr/bin:.:/usr/xyz/bin" ]
}

# =============================================================================
# No expansion when quoted
# =============================================================================

@test "tilde: no expansion in double quotes" {
    run_colony 'HOME=/usr/xyz; echo "~"'
    [ "$status" -eq 0 ]
    [ "$output" = "~" ]
}

@test "tilde: no expansion in quoted assignment value" {
    run_colony 'HOME=/usr/xyz; XPATH=/bin:/usr/bin:.; PPATH="$XPATH:~/bin"; echo $PPATH'
    [ "$status" -eq 0 ]
    [ "$output" = "/bin:/usr/bin:.:~/bin" ]
}

@test "tilde: no expansion when escaped" {
    run_colony 'HOME=/usr/xyz; echo \~'
    [ "$status" -eq 0 ]
    [ "$output" = "~" ]
}

@test "tilde: no expansion with escaped char in path" {
    run_colony 'HOME=/usr/xyz; echo \~chet/bar'
    [ "$status" -eq 0 ]
    [ "$output" = "~chet/bar" ]
}

# =============================================================================
# Special tilde forms
# =============================================================================

@test "tilde: ~+ expands to PWD" {
    run_colony 'cd /tmp; echo ~+'
    [ "$status" -eq 0 ]
    [[ "$output" == *"/tmp"* ]] || [[ "$output" == *"private/tmp"* ]]
}

@test "tilde: ~- expands to OLDPWD" {
    run_colony 'cd /usr; cd /tmp; echo ~-'
    [ "$status" -eq 0 ]
    [ "$output" = "/usr" ]
}

# =============================================================================
# Tilde with username (~user)
# =============================================================================

@test "tilde: ~root expands to root's home" {
    skip "~user expansion not yet implemented"
    run_colony 'echo ~root'
    [ "$status" -eq 0 ]
    [[ "$output" == "/"* ]]
}

@test "tilde: ~\$USER expands variable then user" {
    skip "~\$USER expansion not yet implemented"
    run_colony 'USER=root; echo ~$USER'
    [ "$status" -eq 0 ]
    [[ "$output" == "/"* ]]
}

# =============================================================================
# Tilde in parameter expansion
# =============================================================================

@test "tilde: expands in unquoted parameter default" {
    run_colony 'HOME=/usr/xyz; echo ${TPATH:-~}'
    [ "$status" -eq 0 ]
    [ "$output" = "/usr/xyz" ]
}

@test "tilde: no expansion in quoted parameter default" {
    run_colony 'HOME=/usr/xyz; echo "${TPATH:-~}"'
    [ "$status" -eq 0 ]
    [ "$output" = "~" ]
}

@test "tilde: expands in parameter alternate value" {
    skip "Parameter expansion alternate not fully implemented"
    run_colony 'HOME=/usr/xyz; XPATH=foo; echo "${XPATH+~}"'
    [ "$status" -eq 0 ]
    [ "$output" = "/usr/xyz" ]
}

# =============================================================================
# Tilde in case statements
# =============================================================================

@test "tilde: expands in case pattern" {
    run_colony 'HOME=/usr/xyz; case ~ in /usr/xyz) echo ok;; *) echo bad;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "ok" ]
}

@test "tilde: expands in case word and pattern" {
    run_colony 'HOME=/usr/xyz; case ~ in ~) echo ok;; *) echo bad;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "ok" ]
}

# =============================================================================
# Tilde in heredocs
# =============================================================================

@test "tilde: no expansion in heredoc" {
    run_colony 'cat << EOF
~/bin
EOF'
    [ "$status" -eq 0 ]
    [ "$output" = "~/bin" ]
}

# =============================================================================
# Edge cases
# =============================================================================

@test "tilde: not expanded in middle of word" {
    run_colony 'HOME=/usr/xyz; echo abcd~chet'
    [ "$status" -eq 0 ]
    [ "$output" = "abcd~chet" ]
}

@test "tilde: not expanded after colon in middle of word" {
    run_colony 'HOME=/usr/xyz; echo ":~chet/"'
    [ "$status" -eq 0 ]
    [ "$output" = ":~chet/" ]
}

@test "tilde: NOT expanded after colon in regular command argument" {
    # Tilde after colon only expands in ASSIGNMENT contexts, not echo arguments
    run_colony 'HOME=/usr/xyz; echo abcd:~'
    [ "$status" -eq 0 ]
    [ "$output" = "abcd:~" ]
}

@test "tilde: empty username treated as HOME" {
    run_colony 'HOME=/usr/xyz; echo ~""'
    [ "$status" -eq 0 ]
    [ "$output" = "/usr/xyz" ]
}

@test "tilde: with trailing slash" {
    run_colony 'HOME=/usr/xyz; echo ~/'
    [ "$status" -eq 0 ]
    [ "$output" = "/usr/xyz/" ]
}

# =============================================================================
# Export and declare contexts
# =============================================================================

@test "tilde: expands in export assignment" {
    run_colony 'HOME=/usr/xyz; XPATH=/bin; export PPATH=$XPATH:~/bin; echo $PPATH'
    [ "$status" -eq 0 ]
    [[ "$output" == *"/usr/xyz/bin"* ]]
}

@test "tilde: no expansion in quoted export" {
    run_colony 'HOME=/usr/xyz; XPATH=/bin; export PPATH="$XPATH:~/bin"; echo $PPATH'
    [ "$status" -eq 0 ]
    [ "$output" = "/bin:~/bin" ]
}

@test "tilde: expands in declare assignment" {
    skip "declare not fully implemented"
    run_colony 'HOME=/usr/xyz; XPATH=/bin; declare PPATH=$XPATH:~/bin; echo $PPATH'
    [ "$status" -eq 0 ]
    [[ "$output" == *"/usr/xyz/bin"* ]]
}

# =============================================================================
# Multiple tildes
# =============================================================================

@test "tilde: multiple tildes in colon-separated value" {
    run_colony 'HOME=/usr/xyz; foo=~:~; echo $foo'
    [ "$status" -eq 0 ]
    [ "$output" = "/usr/xyz:/usr/xyz" ]
}

@test "tilde: tilde in default value assignment" {
    run_colony 'HOME=/usr/xyz; unset ADDPATH; : ${ADDPATH:=~/bin}; echo $ADDPATH'
    [ "$status" -eq 0 ]
    [ "$output" = "/usr/xyz/bin" ]
}

# =============================================================================
# Interaction with other expansions  
# =============================================================================

@test "tilde: case with tilde in variable comparison" {
    run_colony 'HOME=/usr/xyz; SHELL=~/bash; case $SHELL in ~/bash) echo ok;; *) echo bad;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "ok" ]
}

@test "tilde: literal tilde preserved when needed" {
    run_colony "echo '~'"
    [ "$status" -eq 0 ]
    [ "$output" = "~" ]
}
