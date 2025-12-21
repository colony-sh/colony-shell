#!/usr/bin/env bats
# Expansion tests from bash exp.tests
# Tests parameter expansion, variable expansion, quoting behavior

COLONY="${COLONY_BIN:-../../target/release/colony}"

# Test null strings without variable expansion
@test "expansion: empty double quotes in middle of word" {
    run $COLONY -c 'recho abcd""efgh'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <abcdefgh>' ]
}

@test "expansion: empty single quotes in middle of word" {
    run $COLONY -c "recho abcd''efgh"
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <abcdefgh>' ]
}

@test "expansion: empty double quotes at start of word" {
    run $COLONY -c 'recho ""abcdefgh'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <abcdefgh>' ]
}

@test "expansion: empty single quotes at start of word" {
    run $COLONY -c "recho ''abcdefgh"
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <abcdefgh>' ]
}

@test "expansion: empty double quotes at end of word" {
    run $COLONY -c 'recho abcd""'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <abcd>' ]
}

@test "expansion: empty single quotes at end of word" {
    run $COLONY -c "recho abcd''"
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <abcd>' ]
}

# Test null strings with variable expansion that fails (unset variable)
@test "expansion: unset var with trailing empty quotes" {
    run $COLONY -c 'recho $xxx""'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <>' ]
}

@test "expansion: unset var with leading empty quotes" {
    run $COLONY -c 'recho ""$xxx'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <>' ]
}

@test "expansion: unset var with trailing empty single quotes" {
    run $COLONY -c "recho \$xxx''"
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <>' ]
}

@test "expansion: unset var with leading empty single quotes" {
    run $COLONY -c "recho ''\$xxx"
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <>' ]
}

@test "expansion: two unset vars with empty quotes between" {
    run $COLONY -c 'recho $xxx""$yyy'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <>' ]
}

# Test null strings with variable expansion that succeeds
@test "expansion: set var with trailing empty quotes" {
    run $COLONY -c 'xxx=abc; recho $xxx""'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <abc>' ]
}

@test "expansion: set var with leading empty quotes" {
    run $COLONY -c 'xxx=abc; recho ""$xxx'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <abc>' ]
}

@test "expansion: set var with trailing empty single quotes" {
    run $COLONY -c "xxx=abc; recho \$xxx''"
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <abc>' ]
}

@test "expansion: set var with leading empty single quotes" {
    run $COLONY -c "xxx=abc; recho ''\$xxx"
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <abc>' ]
}

@test "expansion: two set vars with empty quotes between" {
    run $COLONY -c 'xxx=abc; yyy=def; recho $xxx""$yyy'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <abcdef>' ]
}

# Test expansion of unset variable
@test "expansion: unquoted unset variable produces nothing" {
    run $COLONY -c 'recho $xxx'
    [ "$status" -eq 0 ]
    [ "${#lines[@]}" -eq 0 ]
}

@test "expansion: quoted unset variable produces empty string" {
    run $COLONY -c 'recho "$xxx"'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <>' ]
}

# Test empty string expansion
@test "expansion: empty double quoted string" {
    run $COLONY -c 'recho ""'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <>' ]
}

@test "expansion: empty single quoted string" {
    run $COLONY -c "recho ''"
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <>' ]
}

# Test shell special characters
@test "expansion: escaped backtick" {
    run $COLONY -c 'recho "\`"'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <`>' ]
}

@test "expansion: escaped double quote" {
    run $COLONY -c 'recho "\""'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <">' ]
}

@test "expansion: escaped dollar becomes literal backslash-dollar" {
    run $COLONY -c 'recho "\\$"'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <\$>' ]
}

@test "expansion: double escaped backslash" {
    run $COLONY -c 'recho "\\\\"'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <\\>' ]
}

# Test quoted and unquoted globbing characters
@test "expansion: quoted star followed by unquoted star" {
    run $COLONY -c 'recho "*"*'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <**>' ]
}

# Test POSIX parameter expansions
@test "expansion: default value with command substitution - quoted" {
    run $COLONY -c 'recho "${x:-$(echo "foo bar")}"'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <foo bar>' ]
}

@test "expansion: default value with command substitution - unquoted" {
    run $COLONY -c 'recho ${x:-$(echo "foo bar")}'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <foo>' ]
    [ "${lines[1]}" = 'argv[2] = <bar>' ]
}

@test "expansion: assign default value" {
    run $COLONY -c 'unset X; recho ${X:=abc}; recho $X'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <abc>' ]
    [ "${lines[1]}" = 'argv[1] = <abc>' ]
}

@test "expansion: use alternative value" {
    run $COLONY -c 'set a b c; recho ${3:+posix}'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <posix>' ]
}

@test "expansion: string length" {
    run $COLONY -c 'POSIX=/usr/posix; recho ${#POSIX}'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <10>' ]
}

# Pattern removal
@test "expansion: remove shortest trailing match" {
    run $COLONY -c 'x=file.c; recho ${x%.c}.o'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <file.o>' ]
}

@test "expansion: remove longest trailing match" {
    run $COLONY -c 'x=posix/src/std; recho ${x%%/*}'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <posix>' ]
}

@test "expansion: remove shortest leading match" {
    run $COLONY -c 'HOME=/home/user; x=$HOME/src/cmd; recho ${x#$HOME}'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = </src/cmd>' ]
}

@test "expansion: remove longest leading match" {
    run $COLONY -c 'x=/one/two/three; recho ${x##*/}'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <three>' ]
}

# Pattern removal with non-matching patterns
@test "expansion: non-matching prefix removal - short" {
    run $COLONY -c 'z=abcdef; recho ${z#xyz}'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <abcdef>' ]
}

@test "expansion: non-matching prefix removal - long" {
    run $COLONY -c 'z=abcdef; recho ${z##xyz}'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <abcdef>' ]
}

@test "expansion: non-matching suffix removal - short" {
    run $COLONY -c 'z=abcdef; recho ${z%xyz}'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <abcdef>' ]
}

@test "expansion: non-matching suffix removal - long" {
    run $COLONY -c 'z=abcdef; recho ${z%%xyz}'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <abcdef>' ]
}

# Command substitution differences
@test "expansion: backslash in single quotes with echo" {
    run $COLONY -c "recho '\$x'"
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <$x>' ]
}

@test "expansion: dollar paren preserves backslash-dollar" {
    run $COLONY -c "recho \$(echo '\\\$x')"
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <\$x>' ]
}

# Test $* "$*" and "$@"
@test "expansion: unquoted star splits on whitespace" {
    run $COLONY -c 'set "abc" "def ghi" "jkl"; recho $*'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <abc>' ]
    [ "${lines[1]}" = 'argv[2] = <def>' ]
    [ "${lines[2]}" = 'argv[3] = <ghi>' ]
    [ "${lines[3]}" = 'argv[4] = <jkl>' ]
}

@test "expansion: quoted star joins with space" {
    run $COLONY -c 'set "abc" "def ghi" "jkl"; recho "$*"'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <abc def ghi jkl>' ]
}

@test "expansion: quoted at preserves arguments" {
    run $COLONY -c 'set "abc" "def ghi" "jkl"; recho "$@"'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <abc>' ]
    [ "${lines[1]}" = 'argv[2] = <def ghi>' ]
    [ "${lines[2]}" = 'argv[3] = <jkl>' ]
}

@test "expansion: prefix and suffix around quoted at" {
    run $COLONY -c 'set "abc" "def ghi" "jkl"; recho "xx$@yy"'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <xxabc>' ]
    [ "${lines[1]}" = 'argv[2] = <def ghi>' ]
    [ "${lines[2]}" = 'argv[3] = <jklyy>' ]
}

@test "expansion: double quoted at concatenation" {
    run $COLONY -c 'set "abc" "def ghi" "jkl"; recho "$@$@"'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <abc>' ]
    [ "${lines[1]}" = 'argv[2] = <def ghi>' ]
    [ "${lines[2]}" = 'argv[3] = <jklabc>' ]
    [ "${lines[3]}" = 'argv[4] = <def ghi>' ]
    [ "${lines[4]}" = 'argv[5] = <jkl>' ]
}

@test "expansion: two quoted vars concatenated" {
    run $COLONY -c 'foo=abc; bar=def; recho "$foo""$bar"'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <abcdef>' ]
}

# Positional parameters
@test "expansion: positional params 1 3 5 with unset 8 9" {
    run $COLONY -c 'set one two three four five; recho $1 $3 ${5} $8 ${9}'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <one>' ]
    [ "${lines[1]}" = 'argv[2] = <three>' ]
    [ "${lines[2]}" = 'argv[3] = <five>' ]
}

# Length of positional parameters
@test "expansion: length of positional params count" {
    run $COLONY -c 'set one two three four five; recho $# ${#}'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <5>' ]
    [ "${lines[1]}" = 'argv[2] = <5>' ]
}

@test "expansion: length of first positional param" {
    run $COLONY -c 'set one two three four five; recho ${#1}'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <3>' ]
}

@test "expansion: length of param count var" {
    run $COLONY -c 'set one two three four five; recho ${##}'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <1>' ]
}

@test "expansion: length of at" {
    run $COLONY -c 'set one two three four five; recho ${#@}'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <5>' ]
}

@test "expansion: length of star" {
    run $COLONY -c 'set one two three four five; recho ${#*}'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <5>' ]
}

# Arithmetic expansion
@test "expansion: arithmetic addition" {
    run $COLONY -c 'recho $((28 + 14))'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <42>' ]
}

# Tilde expansion
@test "expansion: quoted tilde is literal" {
    run $COLONY -c "recho '~'"
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <~>' ]
}

# Empty $! when no background jobs
@test "expansion: bang with no background jobs is empty" {
    run $COLONY -c 'recho $!'
    [ "$status" -eq 0 ]
    [ "${#lines[@]}" -eq 0 ]
}

@test "expansion: braced bang with no background jobs is empty" {
    run $COLONY -c 'recho ${!}'
    [ "$status" -eq 0 ]
    [ "${#lines[@]}" -eq 0 ]
}

# Case statement with newline in variable
@test "expansion: case with newline in variable" {
    run $COLONY -c 'NL="
"; x="ab
cd"; case "$x" in *$NL*) recho "newline expected" ;; esac'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <newline expected>' ]
}

# Quoting in case patterns
@test "expansion: case with quoted question mark" {
    run $COLONY -c 'case \? in *"?"*) recho "got it" ;; esac'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <got it>' ]
}

@test "expansion: case with escaped question mark" {
    run $COLONY -c 'case \? in *\?*) recho "got it" ;; esac'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <got it>' ]
}

# Deferred evaluation
@test "expansion: deferred dollar variable" {
    run $COLONY -c 'foo=10; x=foo; y='\''$'\''$x; recho $y'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <$foo>' ]
}

@test "expansion: eval deferred variable" {
    run $COLONY -c 'foo=10; x=foo; eval y='\''$'\''$x; recho $y'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <10>' ]
}

# IFS tests with $*
@test "expansion: quoted star with custom IFS" {
    run $COLONY -c 'set "abc" "def ghi" "jkl"; OIFS="$IFS"; IFS=":$IFS"; recho "$*"; IFS="$OIFS"'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <abc:def ghi:jkl>' ]
}

# Set with empty and null values
@test "expansion: set with empty string and unset var" {
    run $COLONY -c 'unset foo; set $foo bar "" xyz "$foo" abc; recho "$@"'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'argv[1] = <bar>' ]
    [ "${lines[1]}" = 'argv[2] = <>' ]
    [ "${lines[2]}" = 'argv[3] = <xyz>' ]
    [ "${lines[3]}" = 'argv[4] = <>' ]
    [ "${lines[4]}" = 'argv[5] = <abc>' ]
}
