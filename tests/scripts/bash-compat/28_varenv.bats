#!/usr/bin/env bats
# Variable Environment and Scoping Tests
# Tests shell variable assignment, environment variables, export, unset,
# readonly, declare, scoping in subshells, functions, and loops.
# Reference: bash tests/varenv.tests

COLONY="${COLONY_BIN}"

setup() {
    TEST_TEMP="$(mktemp -d)"
    export TEST_TEMP
}

teardown() {
    rm -rf "$TEST_TEMP"
}

# ============================================================================
# Basic Variable Assignment
# ============================================================================

@test "varenv: simple variable assignment" {
    run "$COLONY" -c 'var=hello; echo $var'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "varenv: variable assignment with spaces in value" {
    run "$COLONY" -c 'var="hello world"; echo $var'
    [ "$status" -eq 0 ]
    [ "$output" = "hello world" ]
}

@test "varenv: multiple variable assignments" {
    run "$COLONY" -c 'a=1; b=2; c=3; echo $a $b $c'
    [ "$status" -eq 0 ]
    [ "$output" = "1 2 3" ]
}

@test "varenv: variable assignment on same line" {
    run "$COLONY" -c 'a=1 b=2 c=3; echo $a $b $c'
    [ "$status" -eq 0 ]
    [ "$output" = "1 2 3" ]
}

@test "varenv: empty variable assignment" {
    run "$COLONY" -c 'var=; echo ">$var<"'
    [ "$status" -eq 0 ]
    [ "$output" = "><" ]
}

@test "varenv: variable reassignment" {
    run "$COLONY" -c 'var=first; var=second; echo $var'
    [ "$status" -eq 0 ]
    [ "$output" = "second" ]
}

@test "varenv: variable with numbers in name" {
    run "$COLONY" -c 'var123=value; echo $var123'
    [ "$status" -eq 0 ]
    [ "$output" = "value" ]
}

@test "varenv: variable with underscore in name" {
    run "$COLONY" -c 'my_var=value; echo $my_var'
    [ "$status" -eq 0 ]
    [ "$output" = "value" ]
}

@test "varenv: variable starting with underscore" {
    run "$COLONY" -c '_var=value; echo $_var'
    [ "$status" -eq 0 ]
    [ "$output" = "value" ]
}

@test "varenv: unset variable expands to empty" {
    run "$COLONY" -c 'echo ">$unset_variable<"'
    [ "$status" -eq 0 ]
    [ "$output" = "><" ]
}

@test "varenv: quoted unset variable expands to empty string" {
    run "$COLONY" -c 'echo ">\"$unset_variable\"<"'
    [ "$status" -eq 0 ]
    [ "$output" = ">\"\"<" ]
}

# ============================================================================
# Export and Environment Variables
# ============================================================================

@test "varenv: export variable" {
    run "$COLONY" -c 'export VAR=hello; echo $VAR'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "varenv: export existing variable" {
    run "$COLONY" -c 'VAR=hello; export VAR; echo $VAR'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "varenv: export multiple variables" {
    run "$COLONY" -c 'export A=1 B=2 C=3; echo $A $B $C'
    [ "$status" -eq 0 ]
    [ "$output" = "1 2 3" ]
}

@test "varenv: exported var visible to child" {
    run "$COLONY" -c 'export MYVAR=test; /bin/sh -c "echo \$MYVAR"'
    [ "$status" -eq 0 ]
    [ "$output" = "test" ]
}

@test "varenv: non-exported var not visible to child" {
    run "$COLONY" -c 'MYVAR=test; /bin/sh -c "echo \$MYVAR"'
    [ "$status" -eq 0 ]
    [ "$output" = "" ]
}

@test "varenv: export without value" {
    skip "export VAR (without value) to mark as exported not yet implemented"
    run "$COLONY" -c 'VAR=hello; export VAR; /bin/sh -c "echo \$VAR"'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "varenv: export -p lists exported variables" {
    run "$COLONY" -c 'export MYTEST=abc; export -p | grep MYTEST'
    [ "$status" -eq 0 ]
    [[ "$output" =~ "MYTEST" ]]
}

@test "varenv: export -n removes export attribute" {
    run "$COLONY" -c 'export VAR=test; export -n VAR; /bin/sh -c "echo \$VAR"'
    [ "$status" -eq 0 ]
    [ "$output" = "" ]
}

# ============================================================================
# Inline Environment Variables (VAR=value command)
# ============================================================================

@test "varenv: inline env for command" {
    run "$COLONY" -c 'VAR=inline /bin/sh -c "echo \$VAR"'
    [ "$status" -eq 0 ]
    [ "$output" = "inline" ]
}

@test "varenv: inline env multiple vars" {
    run "$COLONY" -c 'A=1 B=2 /bin/sh -c "echo \$A \$B"'
    [ "$status" -eq 0 ]
    [ "$output" = "1 2" ]
}

@test "varenv: inline env does not persist" {
    skip "Inline env vars currently persist in shell - POSIX requires they only affect the command"
    run "$COLONY" -c 'VAR=inline /usr/bin/true; echo ">$VAR<"'
    [ "$status" -eq 0 ]
    [ "$output" = "><" ]
}

@test "varenv: inline env overrides existing" {
    run "$COLONY" -c 'export VAR=original; VAR=override /bin/sh -c "echo \$VAR"'
    [ "$status" -eq 0 ]
    [ "$output" = "override" ]
}

@test "varenv: inline env with builtin" {
    run "$COLONY" -c 'VAR=test echo $VAR'
    [ "$status" -eq 0 ]
    # For builtins, inline env may or may not apply - bash doesn't export to builtin
    # This tests our behavior
}

# ============================================================================
# Unset Variable
# ============================================================================

@test "varenv: unset removes variable" {
    run "$COLONY" -c 'VAR=hello; unset VAR; echo ">$VAR<"'
    [ "$status" -eq 0 ]
    [ "$output" = "><" ]
}

@test "varenv: unset multiple variables" {
    run "$COLONY" -c 'A=1; B=2; C=3; unset A B; echo ">$A< >$B< >$C<"'
    [ "$status" -eq 0 ]
    [ "$output" = ">< >< >3<" ]
}

@test "varenv: unset exported variable" {
    run "$COLONY" -c 'export VAR=hello; unset VAR; /bin/sh -c "echo \"\$VAR\""'
    [ "$status" -eq 0 ]
    [ "$output" = "" ]
}

@test "varenv: unset non-existent variable succeeds" {
    run "$COLONY" -c 'unset NONEXISTENT; echo $?'
    [ "$status" -eq 0 ]
    [ "$output" = "0" ]
}

@test "varenv: unset -v for variable" {
    skip "unset -v not yet implemented"
    run "$COLONY" -c 'VAR=hello; unset -v VAR; echo ">$VAR<"'
    [ "$status" -eq 0 ]
    [ "$output" = "><" ]
}

# ============================================================================
# Readonly Variables
# ============================================================================

@test "varenv: readonly variable" {
    run "$COLONY" -c 'readonly VAR=hello; echo $VAR'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "varenv: readonly prevents assignment" {
    run "$COLONY" -c 'readonly VAR=hello; VAR=world 2>&1'
    [ "$status" -ne 0 ]
    [[ "$output" =~ "readonly" ]] || [[ "$output" =~ "read-only" ]] || [[ "$output" =~ "cannot" ]]
}

@test "varenv: readonly prevents unset" {
    run "$COLONY" -c 'readonly VAR=hello; unset VAR 2>&1'
    [ "$status" -ne 0 ]
    [[ "$output" =~ "readonly" ]] || [[ "$output" =~ "read-only" ]] || [[ "$output" =~ "cannot" ]]
}

@test "varenv: readonly existing variable" {
    run "$COLONY" -c 'VAR=hello; readonly VAR; VAR=world 2>&1'
    [ "$status" -ne 0 ]
}

@test "varenv: readonly -p lists readonly variables" {
    skip "readonly -p not yet implemented"
    run "$COLONY" -c 'readonly MYTEST=abc; readonly -p | grep MYTEST'
    [ "$status" -eq 0 ]
    [[ "$output" =~ "MYTEST" ]]
}

@test "varenv: readonly without value" {
    run "$COLONY" -c 'VAR=hello; readonly VAR; echo $VAR'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

# ============================================================================
# Declare Builtin
# ============================================================================

@test "varenv: declare variable" {
    run "$COLONY" -c 'declare VAR=hello; echo $VAR'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "varenv: declare -x exports variable" {
    skip "declare -x export not yet implemented"
    run "$COLONY" -c 'declare -x VAR=hello; /bin/sh -c "echo \$VAR"'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "varenv: declare -r makes readonly" {
    skip "declare -r readonly not yet implemented"
    run "$COLONY" -c 'declare -r VAR=hello; VAR=world 2>&1'
    [ "$status" -ne 0 ]
}

@test "varenv: declare -i integer variable" {
    skip "declare -i not yet implemented"
    run "$COLONY" -c 'declare -i NUM=5+5; echo $NUM'
    [ "$status" -eq 0 ]
    [ "$output" = "10" ]
}

@test "varenv: declare -a array variable" {
    skip "declare -a arrays not yet implemented"
    run "$COLONY" -c 'declare -a ARR=(a b c); echo ${ARR[1]}'
    [ "$status" -eq 0 ]
    [ "$output" = "b" ]
}

@test "varenv: declare -A associative array" {
    skip "declare -A associative arrays not yet implemented"
    run "$COLONY" -c 'declare -A MAP; MAP[key]=value; echo ${MAP[key]}'
    [ "$status" -eq 0 ]
    [ "$output" = "value" ]
}

@test "varenv: declare -p prints variable" {
    skip "declare -p not yet implemented"
    run "$COLONY" -c 'VAR=hello; declare -p VAR'
    [ "$status" -eq 0 ]
    [[ "$output" =~ "VAR" ]]
}

@test "varenv: declare +x removes export" {
    skip "declare +x not yet implemented"
    run "$COLONY" -c 'export VAR=test; declare +x VAR; /bin/sh -c "echo \$VAR"'
    [ "$status" -eq 0 ]
    [ "$output" = "" ]
}

# ============================================================================
# Subshell Variable Isolation
# ============================================================================

@test "varenv: subshell inherits variables" {
    run "$COLONY" -c 'VAR=parent; (echo $VAR)'
    [ "$status" -eq 0 ]
    [ "$output" = "parent" ]
}

@test "varenv: subshell changes don't affect parent" {
    skip "Subshell variable isolation not yet implemented - subshells share context"
    run "$COLONY" -c 'VAR=parent; (VAR=child); echo $VAR'
    [ "$status" -eq 0 ]
    [ "$output" = "parent" ]
}

@test "varenv: subshell new var doesn't affect parent" {
    skip "Subshell variable isolation not yet implemented - subshells share context"
    run "$COLONY" -c '(NEWVAR=child); echo ">$NEWVAR<"'
    [ "$status" -eq 0 ]
    [ "$output" = "><" ]
}

@test "varenv: nested subshells inherit" {
    skip "Nested subshells ((cmd)) syntax not yet implemented"
    run "$COLONY" -c 'VAR=outer; ((echo $VAR))'
    [ "$status" -eq 0 ]
    [ "$output" = "outer" ]
}

@test "varenv: subshell export doesn't affect parent" {
    skip "Subshell variable isolation not yet implemented - subshells share context"
    run "$COLONY" -c 'VAR=parent; (export VAR=child); /bin/sh -c "echo \$VAR"'
    [ "$status" -eq 0 ]
    [ "$output" = "" ]
}

@test "varenv: command substitution is subshell" {
    run "$COLONY" -c 'VAR=parent; result=$(VAR=child; echo inner); echo $VAR $result'
    [ "$status" -eq 0 ]
    [ "$output" = "parent inner" ]
}

@test "varenv: pipeline runs in subshell" {
    run "$COLONY" -c 'VAR=parent; echo test | { VAR=child; cat; }; echo $VAR'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "test" ]
    [ "${lines[1]}" = "parent" ]
}

# ============================================================================
# Function Variable Scoping
# ============================================================================

@test "varenv: function can access global variables" {
    run "$COLONY" -c 'GLOBAL=hello; func() { echo $GLOBAL; }; func'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "varenv: function can modify global variables" {
    run "$COLONY" -c 'VAR=before; func() { VAR=after; }; func; echo $VAR'
    [ "$status" -eq 0 ]
    [ "$output" = "after" ]
}

@test "varenv: local variable in function" {
    run "$COLONY" -c 'VAR=global; func() { local VAR=local; echo $VAR; }; func; echo $VAR'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "local" ]
    [ "${lines[1]}" = "global" ]
}

@test "varenv: local hides global" {
    run "$COLONY" -c 'X=outer; f() { local X=inner; echo $X; }; f'
    [ "$status" -eq 0 ]
    [ "$output" = "inner" ]
}

@test "varenv: local without value" {
    run "$COLONY" -c 'X=outer; f() { local X; echo ">$X<"; }; f'
    [ "$status" -eq 0 ]
    [ "$output" = "><" ]
}

@test "varenv: multiple local declarations" {
    run "$COLONY" -c 'f() { local A=1 B=2 C=3; echo $A $B $C; }; f'
    [ "$status" -eq 0 ]
    [ "$output" = "1 2 3" ]
}

@test "varenv: local in nested function" {
    run "$COLONY" -c 'VAR=global; outer() { local VAR=outer; inner() { local VAR=inner; echo $VAR; }; inner; echo $VAR; }; outer; echo $VAR'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "inner" ]
    [ "${lines[1]}" = "outer" ]
    [ "${lines[2]}" = "global" ]
}

@test "varenv: function positional parameters are local" {
    run "$COLONY" -c 'func() { echo $1 $2; }; func arg1 arg2; echo ">$1<"'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "arg1 arg2" ]
    [ "${lines[1]}" = "><" ]
}

@test "varenv: function can export variable" {
    run "$COLONY" -c 'func() { export FUNCVAR=hello; }; func; /bin/sh -c "echo \$FUNCVAR"'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

# ============================================================================
# Variable Scoping in Loops
# ============================================================================

@test "varenv: for loop variable persists" {
    run "$COLONY" -c 'for i in a b c; do :; done; echo $i'
    [ "$status" -eq 0 ]
    [ "$output" = "c" ]
}

@test "varenv: while loop variable persists" {
    run "$COLONY" -c 'i=0; while [ $i -lt 3 ]; do i=$((i+1)); done; echo $i'
    [ "$status" -eq 0 ]
    [ "$output" = "3" ]
}

@test "varenv: variable in loop body persists" {
    run "$COLONY" -c 'for i in a b c; do LAST=$i; done; echo $LAST'
    [ "$status" -eq 0 ]
    [ "$output" = "c" ]
}

@test "varenv: loop in subshell doesn't persist" {
    skip "Subshell variable isolation not yet implemented - subshells share context"
    run "$COLONY" -c '(for i in a b c; do :; done); echo ">$i<"'
    [ "$status" -eq 0 ]
    [ "$output" = "><" ]
}

@test "varenv: loop with local in function" {
    run "$COLONY" -c 'func() { local i; for i in a b c; do :; done; echo $i; }; func; echo ">$i<"'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "c" ]
    [ "${lines[1]}" = "><" ]
}

# ============================================================================
# Special Variables
# ============================================================================

@test "varenv: HOME variable" {
    run "$COLONY" -c 'echo $HOME'
    [ "$status" -eq 0 ]
    [ -n "$output" ]
    [[ "$output" == /* ]]
}

@test "varenv: PATH variable" {
    run "$COLONY" -c 'echo $PATH'
    [ "$status" -eq 0 ]
    [ -n "$output" ]
}

@test "varenv: PWD variable" {
    run "$COLONY" -c 'echo $PWD'
    [ "$status" -eq 0 ]
    [[ "$output" == /* ]]
}

@test "varenv: OLDPWD set by cd" {
    run "$COLONY" -c 'cd /tmp; cd /; echo $OLDPWD'
    [ "$status" -eq 0 ]
    [[ "$output" =~ "/tmp" ]] || [[ "$output" =~ "private/tmp" ]]
}

@test "varenv: OLDPWD used by cd -" {
    run "$COLONY" -c 'cd /tmp; cd /; cd -; pwd'
    [ "$status" -eq 0 ]
    [[ "$output" =~ "/tmp" ]] || [[ "$output" =~ "private/tmp" ]]
}

@test "varenv: SHLVL increments in subshell" {
    skip "SHLVL not yet implemented"
    run "$COLONY" -c 'echo $SHLVL; $COLONY -c "echo \$SHLVL"'
    [ "$status" -eq 0 ]
    local outer="${lines[0]}"
    local inner="${lines[1]}"
    [ "$inner" -eq $((outer + 1)) ]
}

@test "varenv: RANDOM is different each time" {
    skip "RANDOM not yet implemented"
    run "$COLONY" -c 'echo $RANDOM $RANDOM'
    [ "$status" -eq 0 ]
    local parts=($output)
    [ "${parts[0]}" != "${parts[1]}" ]
}

@test "varenv: SECONDS increases" {
    skip "SECONDS not yet implemented"
    run "$COLONY" -c 'SECONDS=0; sleep 1; echo $SECONDS'
    [ "$status" -eq 0 ]
    [ "$output" -ge 1 ]
}

@test "varenv: LINENO in script" {
    skip "LINENO not yet implemented"
    echo 'echo $LINENO' > "$TEST_TEMP/lineno.sh"
    run "$COLONY" "$TEST_TEMP/lineno.sh"
    [ "$status" -eq 0 ]
    [ "$output" = "1" ]
}

# ============================================================================
# ENV and BASH_ENV Sourcing
# ============================================================================

@test "varenv: BASH_ENV sourced for non-interactive" {
    echo 'SOURCED_VAR=from_env' > "$TEST_TEMP/env_file"
    run env BASH_ENV="$TEST_TEMP/env_file" "$COLONY" -c 'echo $SOURCED_VAR'
    [ "$status" -eq 0 ]
    [ "$output" = "from_env" ]
}

@test "varenv: ENV sourced for sh compatibility" {
    echo 'SOURCED_VAR=from_env' > "$TEST_TEMP/env_file"
    run env ENV="$TEST_TEMP/env_file" "$COLONY" -c 'echo $SOURCED_VAR'
    [ "$status" -eq 0 ]
    [ "$output" = "from_env" ]
}

@test "varenv: BASH_ENV function available" {
    echo 'myfunc() { echo from_env; }' > "$TEST_TEMP/env_file"
    run env BASH_ENV="$TEST_TEMP/env_file" "$COLONY" -c 'myfunc'
    [ "$status" -eq 0 ]
    [ "$output" = "from_env" ]
}

# ============================================================================
# Variable in Different Contexts
# ============================================================================

@test "varenv: variable in double quotes" {
    run "$COLONY" -c 'VAR="hello world"; echo "$VAR"'
    [ "$status" -eq 0 ]
    [ "$output" = "hello world" ]
}

@test "varenv: variable in single quotes is literal" {
    run "$COLONY" -c "VAR=hello; echo '\$VAR'"
    [ "$status" -eq 0 ]
    [ "$output" = '$VAR' ]
}

@test "varenv: variable in here document" {
    run "$COLONY" -c 'VAR=hello; cat <<EOF
$VAR world
EOF'
    [ "$status" -eq 0 ]
    [ "$output" = "hello world" ]
}

@test "varenv: variable in quoted here document delimiter" {
    run "$COLONY" -c "VAR=hello; cat <<'EOF'
\$VAR world
EOF"
    [ "$status" -eq 0 ]
    [ "$output" = '$VAR world' ]
}

@test "varenv: variable in arithmetic expansion" {
    run "$COLONY" -c 'X=5; Y=3; echo $((X + Y))'
    [ "$status" -eq 0 ]
    [ "$output" = "8" ]
}

@test "varenv: variable in case pattern" {
    run "$COLONY" -c 'PAT="foo"; case foo in $PAT) echo matched;; esac'
    [ "$status" -eq 0 ]
    [ "$output" = "matched" ]
}

@test "varenv: variable in array index" {
    skip "Arrays not yet implemented"
    run "$COLONY" -c 'ARR=(a b c); I=1; echo ${ARR[$I]}'
    [ "$status" -eq 0 ]
    [ "$output" = "b" ]
}

# ============================================================================
# Env Command Interaction
# ============================================================================

@test "varenv: env shows exported variables" {
    run "$COLONY" -c 'export MYTEST=hello; env | grep MYTEST'
    [ "$status" -eq 0 ]
    [ "$output" = "MYTEST=hello" ]
}

@test "varenv: env -i clears environment" {
    run "$COLONY" -c 'export MYTEST=hello; env -i /bin/sh -c "echo \$MYTEST"'
    [ "$status" -eq 0 ]
    [ "$output" = "" ]
}

@test "varenv: env sets variable for command" {
    run "$COLONY" -c 'env ENVVAR=test /bin/sh -c "echo \$ENVVAR"'
    [ "$status" -eq 0 ]
    [ "$output" = "test" ]
}

@test "varenv: env variable doesn't persist" {
    run "$COLONY" -c 'env ENVVAR=test /usr/bin/true; echo ">$ENVVAR<"'
    [ "$status" -eq 0 ]
    [ "$output" = "><" ]
}

# ============================================================================
# Variable Attributes and Edge Cases
# ============================================================================

@test "varenv: variable with newline in value" {
    run "$COLONY" -c 'VAR="line1
line2"; echo "$VAR"'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "line1" ]
    [ "${lines[1]}" = "line2" ]
}

@test "varenv: variable with special chars" {
    run "$COLONY" -c 'VAR="hello@#%^*()"; echo "$VAR"'
    [ "$status" -eq 0 ]
    [ "$output" = "hello@#%^*()" ]
}

@test "varenv: variable with equals sign in value" {
    run "$COLONY" -c 'VAR="key=value"; echo "$VAR"'
    [ "$status" -eq 0 ]
    [ "$output" = "key=value" ]
}

@test "varenv: variable name cannot start with digit" {
    run "$COLONY" -c '1VAR=test 2>&1'
    [ "$status" -ne 0 ]
}

@test "varenv: empty variable name is error" {
    run "$COLONY" -c '=test 2>&1'
    [ "$status" -ne 0 ]
}

@test "varenv: typeset is alias for declare" {
    skip "typeset not yet implemented as declare alias"
    run "$COLONY" -c 'typeset VAR=hello; echo $VAR'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

# ============================================================================
# Process Environment Inheritance
# ============================================================================

@test "varenv: child inherits exported vars only" {
    run "$COLONY" -c 'export EXP=exported; LOCAL=local; /bin/sh -c "echo \$EXP"'
    [ "$status" -eq 0 ]
    [ "$output" = "exported" ]
}

@test "varenv: grandchild inherits exported vars" {
    run "$COLONY" -c 'export VAR=hello; /bin/sh -c "/bin/sh -c \"echo \\\$VAR\""'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "varenv: modified export visible to child" {
    skip "Modifying already-exported variable should update child env - not yet implemented"
    run "$COLONY" -c 'export VAR=first; VAR=second; /bin/sh -c "echo \$VAR"'
    [ "$status" -eq 0 ]
    [ "$output" = "second" ]
}

@test "varenv: export after assignment makes visible" {
    skip "export VAR (without value) to mark as exported not yet implemented"
    run "$COLONY" -c 'VAR=hello; /bin/sh -c "echo \"\$VAR\""; export VAR; /bin/sh -c "echo \"\$VAR\""'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "" ]
    [ "${lines[1]}" = "hello" ]
}

# ============================================================================
# Bash Compatibility Comparisons
# ============================================================================

@test "compare: basic assignment with bash" {
    run bash -c 'VAR=hello; echo $VAR'
    bash_output="$output"
    run "$COLONY" -c 'VAR=hello; echo $VAR'
    [ "$output" = "$bash_output" ]
}

@test "compare: export with bash" {
    run bash -c 'export VAR=hello; /bin/sh -c "echo \$VAR"'
    bash_output="$output"
    run "$COLONY" -c 'export VAR=hello; /bin/sh -c "echo \$VAR"'
    [ "$output" = "$bash_output" ]
}

@test "compare: unset with bash" {
    run bash -c 'VAR=hello; unset VAR; echo ">$VAR<"'
    bash_output="$output"
    run "$COLONY" -c 'VAR=hello; unset VAR; echo ">$VAR<"'
    [ "$output" = "$bash_output" ]
}

@test "compare: inline env with bash" {
    run bash -c 'VAR=inline /bin/sh -c "echo \$VAR"'
    bash_output="$output"
    run "$COLONY" -c 'VAR=inline /bin/sh -c "echo \$VAR"'
    [ "$output" = "$bash_output" ]
}

@test "compare: local in function with bash" {
    run bash -c 'VAR=global; f() { local VAR=local; echo $VAR; }; f; echo $VAR'
    bash_output="$output"
    run "$COLONY" -c 'VAR=global; f() { local VAR=local; echo $VAR; }; f; echo $VAR'
    [ "$output" = "$bash_output" ]
}

@test "compare: subshell isolation with bash" {
    skip "Subshell variable isolation not yet implemented"
    run bash -c 'VAR=parent; (VAR=child); echo $VAR'
    bash_output="$output"
    run "$COLONY" -c 'VAR=parent; (VAR=child); echo $VAR'
    [ "$output" = "$bash_output" ]
}

@test "compare: for loop var persistence with bash" {
    run bash -c 'for i in a b c; do :; done; echo $i'
    bash_output="$output"
    run "$COLONY" -c 'for i in a b c; do :; done; echo $i'
    [ "$output" = "$bash_output" ]
}

@test "compare: readonly with bash" {
    run bash -c 'readonly VAR=hello; echo $VAR'
    bash_output="$output"
    run "$COLONY" -c 'readonly VAR=hello; echo $VAR'
    [ "$output" = "$bash_output" ]
}

@test "compare: HOME variable with bash" {
    run bash -c 'echo $HOME'
    bash_output="$output"
    run "$COLONY" -c 'echo $HOME'
    [ "$output" = "$bash_output" ]
}

@test "compare: PWD variable with bash" {
    run bash -c 'pwd'
    bash_output="$output"
    run "$COLONY" -c 'pwd'
    [ "$output" = "$bash_output" ]
}

# ============================================================================
# Complex Scenarios
# ============================================================================

@test "varenv: variable in command substitution result" {
    run "$COLONY" -c 'VAR=$(echo hello); echo $VAR world'
    [ "$status" -eq 0 ]
    [ "$output" = "hello world" ]
}

@test "varenv: nested variable expansion" {
    skip "eval builtin not yet implemented"
    run "$COLONY" -c 'A=B; B=hello; eval echo \$$A'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "varenv: variable in prompt string PS1" {
    skip "PS1 prompt handling not tested here"
}

@test "varenv: IFS affects word splitting" {
    run "$COLONY" -c 'VAR="a:b:c"; IFS=:; echo $VAR'
    [ "$status" -eq 0 ]
    [ "$output" = "a b c" ]
}

@test "varenv: default IFS behavior" {
    run "$COLONY" -c 'VAR="a b  c"; echo $VAR'
    [ "$status" -eq 0 ]
    [ "$output" = "a b c" ]
}

@test "varenv: quoted prevents IFS splitting" {
    run "$COLONY" -c 'VAR="a b  c"; echo "$VAR"'
    [ "$status" -eq 0 ]
    [ "$output" = "a b  c" ]
}

@test "varenv: unset IFS restores default" {
    run "$COLONY" -c 'IFS=:; unset IFS; VAR="a b c"; echo $VAR'
    [ "$status" -eq 0 ]
    [ "$output" = "a b c" ]
}
