#!/usr/bin/env bats

# Programmable Completion Tests (complete, compgen)
# Tests the bash-compatible programmable completion builtins

load '/usr/local/lib/bats-support/load'
load '/usr/local/lib/bats-assert/load'

COLONY="${COLONY:-./target/debug/colony-shell}"

# =============================================================================
# complete builtin - basic functionality
# =============================================================================

@test "complete: no args lists all completions" {
    run $COLONY -c 'complete -W "foo bar" cmd1; complete -f cmd2; complete'
    assert_success
    assert_output --partial "cmd1"
    assert_output --partial "cmd2"
}

@test "complete -W: word list completion" {
    run $COLONY -c 'complete -W "alpha beta gamma" testcmd; complete -p testcmd'
    assert_success
    assert_output --partial "-W"
    assert_output --partial "alpha beta gamma"
}

@test "complete -F: function completion" {
    run $COLONY -c 'complete -F _myfunc mycmd; complete -p mycmd'
    assert_success
    assert_output --partial "-F"
    assert_output --partial "_myfunc"
}

@test "complete -f: file completion" {
    run $COLONY -c 'complete -f filecmd; complete -p filecmd'
    assert_success
    assert_output --partial "-f"
}

@test "complete -d: directory completion" {
    run $COLONY -c 'complete -d dircmd; complete -p dircmd'
    assert_success
    assert_output --partial "-d"
}

@test "complete -c: command completion" {
    run $COLONY -c 'complete -c cmdcmd; complete -p cmdcmd'
    assert_success
    assert_output --partial "-c"
}

@test "complete -a: alias completion" {
    run $COLONY -c 'complete -a aliascmd; complete -p aliascmd'
    assert_success
    assert_output --partial "-a"
}

@test "complete -b: builtin completion" {
    run $COLONY -c 'complete -b builtincmd; complete -p builtincmd'
    assert_success
    assert_output --partial "-b"
}

@test "complete -v: variable completion" {
    run $COLONY -c 'complete -v varcmd; complete -p varcmd'
    assert_success
    assert_output --partial "-v"
}

@test "complete -e: export completion" {
    run $COLONY -c 'complete -e exportcmd; complete -p exportcmd'
    assert_success
    assert_output --partial "-e"
}

@test "complete -u: user completion" {
    run $COLONY -c 'complete -u usercmd; complete -p usercmd'
    assert_success
    assert_output --partial "-u"
}

@test "complete -g: group completion" {
    run $COLONY -c 'complete -g groupcmd; complete -p groupcmd'
    assert_success
    assert_output --partial "-g"
}

@test "complete -j: job completion" {
    run $COLONY -c 'complete -j jobcmd; complete -p jobcmd'
    assert_success
    assert_output --partial "-j"
}

@test "complete -k: keyword completion" {
    run $COLONY -c 'complete -k kwcmd; complete -p kwcmd'
    assert_success
    assert_output --partial "-k"
}

@test "complete -s: service completion" {
    run $COLONY -c 'complete -s svccmd; complete -p svccmd'
    assert_success
    assert_output --partial "-s"
}

# =============================================================================
# complete builtin - extended options
# =============================================================================

@test "complete -P: prefix" {
    run $COLONY -c 'complete -P "prefix_" -W "word" cmd; complete -p cmd'
    assert_success
    assert_output --partial "-P"
    assert_output --partial "prefix_"
}

@test "complete -S: suffix" {
    run $COLONY -c 'complete -S "_suffix" -W "word" cmd; complete -p cmd'
    assert_success
    assert_output --partial "-S"
    assert_output --partial "_suffix"
}

@test "complete -X: filter pattern" {
    run $COLONY -c 'complete -X "*.bak" -f cmd; complete -p cmd'
    assert_success
    assert_output --partial "-X"
}

@test "complete -G: glob pattern" {
    run $COLONY -c 'complete -G "*.txt" cmd; complete -p cmd'
    assert_success
    assert_output --partial "-G"
}

@test "complete -C: command" {
    run $COLONY -c 'complete -C "echo foo" cmd; complete -p cmd'
    assert_success
    assert_output --partial "-C"
}

@test "complete -o: completion option" {
    run $COLONY -c 'complete -o nospace -f cmd; complete -p cmd'
    assert_success
    assert_output --partial "-o"
    assert_output --partial "nospace"
}

@test "complete -A: action name" {
    run $COLONY -c 'complete -A function cmd; complete -p cmd'
    assert_success
}

# =============================================================================
# complete builtin - print and remove modes
# =============================================================================

@test "complete -p: print specific command" {
    run $COLONY -c 'complete -W "a b c" mytest; complete -p mytest'
    assert_success
    assert_output --partial "mytest"
}

@test "complete -p: no output for unknown command" {
    run $COLONY -c 'complete -p nonexistent'
    assert_success
    refute_output --partial "nonexistent"
}

@test "complete -r: remove completion" {
    run $COLONY -c 'complete -W "x" rmtest; complete -r rmtest; complete -p rmtest'
    assert_success
    refute_output --partial "rmtest"
}

@test "complete -r: multiple commands" {
    run $COLONY -c 'complete -f c1; complete -d c2; complete -r c1 c2; complete'
    assert_success
    refute_output --partial "c1"
    refute_output --partial "c2"
}

# =============================================================================
# complete builtin - special completions (-D, -E)
# =============================================================================

@test "complete -D: default completion" {
    run $COLONY -c 'complete -D -W "default"; complete -p -D'
    assert_success
    assert_output --partial "default"
}

@test "complete -E: empty line completion" {
    run $COLONY -c 'complete -E -W "empty"; complete -p -E'
    assert_success
    assert_output --partial "empty"
}

# =============================================================================
# complete builtin - multiple actions
# =============================================================================

@test "complete: multiple action flags" {
    run $COLONY -c 'complete -f -d multiact; complete -p multiact'
    assert_success
    assert_output --partial "-f"
    assert_output --partial "-d"
}

@test "complete: multiple commands" {
    run $COLONY -c 'complete -W "shared" c1 c2 c3; complete -p c1; complete -p c2; complete -p c3'
    assert_success
    assert_output --partial "c1"
    assert_output --partial "c2"
    assert_output --partial "c3"
}

# =============================================================================
# compgen builtin - word list
# =============================================================================

@test "compgen -W: generate from word list" {
    run $COLONY -c 'compgen -W "apple banana cherry"'
    assert_success
    assert_line "apple"
    assert_line "banana"
    assert_line "cherry"
}

@test "compgen -W: filter by prefix" {
    run $COLONY -c 'compgen -W "apple apricot banana cherry" ap'
    assert_success
    assert_line "apple"
    assert_line "apricot"
    refute_line "banana"
    refute_line "cherry"
}

@test "compgen -W: no matches returns failure" {
    run $COLONY -c 'compgen -W "apple banana" xyz'
    assert_failure
    assert_output ""
}

@test "compgen -W: empty prefix matches all" {
    run $COLONY -c 'compgen -W "one two three" ""'
    assert_success
    assert_line "one"
    assert_line "two"
    assert_line "three"
}

# =============================================================================
# compgen builtin - actions
# =============================================================================

@test "compgen -a: list aliases" {
    run $COLONY -c 'alias foo=bar; alias fizz=buzz; compgen -a f'
    assert_success
    assert_line "foo"
    assert_line "fizz"
}

@test "compgen -b: list builtins" {
    run $COLONY -c 'compgen -b ec'
    assert_success
    assert_line "echo"
}

@test "compgen -c: list commands" {
    run $COLONY -c 'compgen -b ec'
    assert_success
    assert_output --partial "echo"
}

@test "compgen -v: list variables" {
    run $COLONY -c 'FOO=1; BAR=2; compgen -v FO'
    assert_success
    assert_output --partial "FOO"
}

@test "compgen -e: list exports" {
    run $COLONY -c 'export TESTEXPORT=1; compgen -e TESTEXPORT'
    assert_success
    assert_output --partial "TESTEXPORT"
}

@test "compgen -A function: list functions" {
    run $COLONY -c 'myfunc() { echo hi; }; compgen -A function my'
    assert_success
    assert_line "myfunc"
}

@test "compgen -A signal: list signals" {
    run $COLONY -c 'compgen -A signal INT'
    assert_success
    assert_line "INT"
}

@test "compgen -A setopt: list set options" {
    run $COLONY -c 'compgen -A setopt err'
    assert_success
    assert_line "errexit"
}

@test "compgen -A shopt: list shopt options" {
    run $COLONY -c 'compgen -A shopt ext'
    assert_success
    assert_line "extglob"
}

@test "compgen -k: list keywords" {
    run $COLONY -c 'compgen -k if'
    assert_success
    assert_line "if"
}

# =============================================================================
# compgen builtin - prefix and suffix
# =============================================================================

@test "compgen -P: add prefix" {
    run $COLONY -c 'compgen -P "pre_" -W "foo bar"'
    assert_success
    assert_line "pre_foo"
    assert_line "pre_bar"
}

@test "compgen -S: add suffix" {
    run $COLONY -c 'compgen -S "_suf" -W "foo bar"'
    assert_success
    assert_line "foo_suf"
    assert_line "bar_suf"
}

@test "compgen -P -S: both prefix and suffix" {
    run $COLONY -c 'compgen -P "[" -S "]" -W "a b"'
    assert_success
    assert_line "[a]"
    assert_line "[b]"
}

# =============================================================================
# compgen builtin - filter pattern
# =============================================================================

@test "compgen -X: exclude matches" {
    run $COLONY -c 'compgen -X "*.txt" -W "a.txt b.log c.txt d.log"'
    assert_success
    assert_line "b.log"
    assert_line "d.log"
    refute_line "a.txt"
    refute_line "c.txt"
}

@test "compgen -X !pattern: include only matches" {
    run $COLONY -c 'compgen -X "!*.txt" -W "a.txt b.log c.txt d.log"'
    assert_success
    assert_line "a.txt"
    assert_line "c.txt"
    refute_line "b.log"
    refute_line "d.log"
}

# =============================================================================
# compgen builtin - file and directory
# =============================================================================

@test "compgen -f: file completion" {
    run $COLONY -c 'cd /tmp && touch compgen_test_file.txt && compgen -f compgen_test && rm -f compgen_test_file.txt'
    assert_success
    assert_output --partial "compgen_test_file.txt"
}

@test "compgen -d: directory completion" {
    run $COLONY -c 'cd /tmp && mkdir -p compgen_test_dir && compgen -d compgen_test && rmdir compgen_test_dir'
    assert_success
    assert_output --partial "compgen_test_dir"
}

# =============================================================================
# compgen builtin - users and groups
# =============================================================================

@test "compgen -u: user completion" {
    run $COLONY -c 'compgen -u root'
    assert_success
    assert_line "root"
}

@test "compgen -g: group completion" {
    # Most systems have a 'root' or 'wheel' group
    run $COLONY -c 'compgen -g | head -1'
    assert_success
}

# =============================================================================
# Integration tests
# =============================================================================

@test "complete and compgen work together" {
    run $COLONY -c '
complete -W "start stop restart status" myservice
# Simulating what readline would use
compgen -W "start stop restart status" st
'
    assert_success
    assert_line "start"
    assert_line "stop"
    assert_line "status"
}

@test "completion with function pattern" {
    run $COLONY -c '
_complete_fruit() {
    COMPREPLY=($(compgen -W "apple apricot banana blueberry" -- "${COMP_WORDS[$COMP_CWORD]}"))
}
complete -F _complete_fruit fruit
complete -p fruit
'
    assert_success
    assert_output --partial "_complete_fruit"
}

@test "overwrite existing completion" {
    run $COLONY -c '
complete -W "old" mycmd
complete -W "new" mycmd
complete -p mycmd
'
    assert_success
    assert_output --partial "new"
    refute_output --partial "old"
}
