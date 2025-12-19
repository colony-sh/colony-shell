#!/usr/bin/env bats
# Test Colony shell heredoc support for POSIX compliance
# Based on bash heredoc.tests

COLONY="${COLONY_BIN}"

# ============================================================================
# Basic Heredocs
# ============================================================================

@test "heredoc: basic cat" {
    run $COLONY -c 'cat <<EOF
a
b
c
EOF'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "a" ]
    [ "${lines[1]}" = "b" ]
    [ "${lines[2]}" = "c" ]
}

@test "heredoc: read with heredoc" {
    run $COLONY -c 'read x <<EOF
a
b
c
EOF
echo "$x"'
    [ "$status" -eq 0 ]
    [ "$output" = "a" ]
}

@test "heredoc: quoted delimiter no expansion" {
    run $COLONY -c 'a=foo; cat <<'\''EOF'\''
$a
EOF'
    [ "$status" -eq 0 ]
    [ "$output" = '$a' ]
}

@test "heredoc: unquoted delimiter with expansion" {
    run $COLONY -c 'a=foo; cat <<EOF
$a
EOF'
    [ "$status" -eq 0 ]
    [ "$output" = "foo" ]
}

# ============================================================================
# Empty Heredocs
# ============================================================================

@test "heredoc: empty heredoc" {
    run $COLONY -c 'read x <<EOF
EOF
echo "[$x]"'
    [ "$status" -eq 0 ]
    [ "$output" = "[]" ]
}

@test "heredoc: empty heredoc quoted delimiter" {
    run $COLONY -c 'read x <<'\''EOF'\''
EOF
echo "[$x]"'
    [ "$status" -eq 0 ]
    [ "$output" = "[]" ]
}

@test "heredoc: empty variable expansion in heredoc" {
    run $COLONY -c 'empty=; read x <<EOF
$empty
EOF
echo "[$x]"'
    [ "$status" -eq 0 ]
    [ "$output" = "[]" ]
}

# ============================================================================
# Tab Stripping (<<-)
# ============================================================================

@test "heredoc: <<- strips leading tabs" {
    run $COLONY -c 'cat <<-EOF
	tab 1
	tab 2
	tab 3
	EOF'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "tab 1" ]
    [ "${lines[1]}" = "tab 2" ]
    [ "${lines[2]}" = "tab 3" ]
}

@test "heredoc: <<- does not strip spaces" {
    run $COLONY -c 'cat <<-EOF
    space 1
    space 2
	EOF'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "    space 1" ]
    [ "${lines[1]}" = "    space 2" ]
}

# ============================================================================
# Multiple Heredocs
# ============================================================================

@test "heredoc: multiple heredocs on same line" {
    run $COLONY -c 'cat <<EOF1 <<EOF2
hi
EOF1
there
EOF2'
    [ "$status" -eq 0 ]
    [ "$output" = "there" ]
}

@test "heredoc: multiple heredocs with different fds" {
    run $COLONY -c 'while read line1; do
    read line2 <&3
    echo "$line1 - $line2"
done <<EOF1 3<<EOF2
one
two
EOF1
alpha
beta
EOF2'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "one - alpha" ]
    [ "${lines[1]}" = "two - beta" ]
}

# ============================================================================
# Quoted Delimiters
# ============================================================================

@test "heredoc: single quoted delimiter protects content" {
    run $COLONY -c 'a=foo; cat <<'\''EOF'\''
hi\
there$a
stuff
EOF'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'hi\' ]
    [ "${lines[1]}" = 'there$a' ]
    [ "${lines[2]}" = "stuff" ]
}

@test "heredoc: quoted delimiter preserves backslash-newline" {
    run $COLONY -c 'cat <<'\''EOF'\''
hi\
there
EO\
F
EOF'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = 'hi\' ]
    [ "${lines[1]}" = "there" ]
    [ "${lines[2]}" = 'EO\' ]
    [ "${lines[3]}" = "F" ]
}

# ============================================================================
# Backslash-Newline Processing
# ============================================================================

@test "heredoc: unquoted removes backslash-newline" {
    run $COLONY -c 'cat <<EOF
line 1\
line 2
EOF'
    [ "$status" -eq 0 ]
    [ "$output" = "line 1line 2" ]
}

@test "heredoc: backslash-newline at start of heredoc" {
    run $COLONY -c 'cat <<EO\
F
hi
EOF'
    [ "$status" -eq 0 ]
    [ "$output" = "hi" ]
}

@test "heredoc: backslash-newline in delimiter" {
    run $COLONY -c 'cat <<EOF
hi
EO\
F'
    [ "$status" -eq 0 ]
    [ "$output" = "hi" ]
}

@test "heredoc: backslash-newline before delimiter" {
    run $COLONY -c 'cat <<EOF
next\
EOF
EOF'
    [ "$status" -eq 0 ]
    [ "$output" = "nextEOF" ]
}

# ============================================================================
# File Operations
# ============================================================================

@test "heredoc: write to file" {
    run $COLONY -c 'cat > /tmp/heredoc-test-$$ <<EOF
abc
EOF
cat /tmp/heredoc-test-$$
rm -f /tmp/heredoc-test-$$'
    [ "$status" -eq 0 ]
    [ "$output" = "abc" ]
}

@test "heredoc: append to file" {
    run $COLONY -c 'cat > /tmp/heredoc-test-$$ <<EOF
abc
EOF
cat >> /tmp/heredoc-test-$$ <<EOF
def
EOF
cat /tmp/heredoc-test-$$
rm -f /tmp/heredoc-test-$$'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "abc" ]
    [ "${lines[1]}" = "def" ]
}

# ============================================================================
# Special Characters
# ============================================================================

@test "heredoc: double quote in heredoc" {
    run $COLONY -c 'cat <<EOF
echo "
EOF'
    [ "$status" -eq 0 ]
    [ "$output" = 'echo "' ]
}

@test "heredoc: escaped double quote in heredoc" {
    run $COLONY -c 'cat <<EOF
echo \"
EOF'
    [ "$status" -eq 0 ]
    [ "$output" = 'echo \"' ]
}

# ============================================================================
# Heredocs in Pipelines
# ============================================================================

@test "heredoc: in pipeline" {
    run $COLONY -c 'cat <<EOF | grep b
a
b
c
EOF'
    [ "$status" -eq 0 ]
    [ "$output" = "b" ]
}

@test "heredoc: in command substitution" {
    run $COLONY -c 'result=$(cat <<EOF
hello
world
EOF
)
echo "$result"'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "hello" ]
    [ "${lines[1]}" = "world" ]
}

# ============================================================================
# Edge Cases
# ============================================================================

@test "heredoc: delimiter with numbers" {
    run $COLONY -c 'cat <<EOF123
test
EOF123'
    [ "$status" -eq 0 ]
    [ "$output" = "test" ]
}

@test "heredoc: delimiter with underscores" {
    run $COLONY -c 'cat <<END_OF_INPUT
test
END_OF_INPUT'
    [ "$status" -eq 0 ]
    [ "$output" = "test" ]
}

@test "heredoc: very long delimiter" {
    run $COLONY -c 'cat <<VERY_LONG_DELIMITER_NAME
test
VERY_LONG_DELIMITER_NAME'
    [ "$status" -eq 0 ]
    [ "$output" = "test" ]
}

@test "heredoc: content with delimiter-like text" {
    run $COLONY -c 'cat <<EOF
This line has EOF in it
EOF'
    [ "$status" -eq 0 ]
    [ "$output" = "This line has EOF in it" ]
}

@test "heredoc: multiple blank lines" {
    run $COLONY -c 'cat <<EOF
line1


line4
EOF'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "line1" ]
    [ "${lines[1]}" = "" ]
    [ "${lines[2]}" = "" ]
    [ "${lines[3]}" = "line4" ]
}

# ============================================================================
# Comparison with Bash
# ============================================================================

@test "compare: basic heredoc with bash" {
    colony_out=$($COLONY -c 'cat <<EOF
test
EOF')
    bash_out=$(bash -c 'cat <<EOF
test
EOF')
    [ "$colony_out" = "$bash_out" ]
}

@test "compare: heredoc expansion with bash" {
    colony_out=$($COLONY -c 'x=hello; cat <<EOF
$x world
EOF')
    bash_out=$(bash -c 'x=hello; cat <<EOF
$x world
EOF')
    [ "$colony_out" = "$bash_out" ]
}

@test "compare: quoted heredoc with bash" {
    colony_out=$($COLONY -c 'x=hello; cat <<'\''EOF'\''
$x world
EOF')
    bash_out=$(bash -c 'x=hello; cat <<'\''EOF'\''
$x world
EOF')
    [ "$colony_out" = "$bash_out" ]
}

@test "compare: <<- with bash" {
    colony_out=$($COLONY -c 'cat <<-EOF
	test
	EOF')
    bash_out=$(bash -c 'cat <<-EOF
	test
	EOF')
    [ "$colony_out" = "$bash_out" ]
}
