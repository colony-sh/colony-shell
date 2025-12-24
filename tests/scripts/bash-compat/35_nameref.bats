#!/usr/bin/env bats
# Test suite for bash nameref (declare -n) functionality
# Based on bash nameref.tests from bminor/bash repository

COLONY="${COLONY_BIN}"

setup() {
    TEST_TEMP="$(mktemp -d)"
    export TEST_TEMP
}

teardown() {
    rm -rf "$TEST_TEMP"
}

@test "nameref: basic declare -n creates reference" {
    run "$COLONY" -c 'bar=one; declare -n foo=bar; echo $foo'
    [ "$status" -eq 0 ]
    [ "$output" = "one" ]
}

@test "nameref: reading through nameref gets target value" {
    run "$COLONY" -c 'target=hello; declare -n ref=target; echo $ref'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "nameref: writing through nameref sets target" {
    run "$COLONY" -c 'target=initial; declare -n ref=target; ref=changed; echo $target'
    [ "$status" -eq 0 ]
    [ "$output" = "changed" ]
}

@test "nameref: multiple namerefs to same target" {
    run "$COLONY" -c 'target=value; declare -n ref1=target; declare -n ref2=target; echo "$ref1 $ref2"'
    [ "$status" -eq 0 ]
    [ "$output" = "value value" ]
}

@test "nameref: changing target reflects in nameref" {
    run "$COLONY" -c 'bar=one; declare -n foo=bar; bar=two; echo $foo'
    [ "$status" -eq 0 ]
    [ "$output" = "two" ]
}

@test "nameref: reassigning nameref target" {
    run "$COLONY" -c 'flow=two; flip=three; declare -n fee=flow; echo $fee; declare -n fee=flip; echo $fee'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "two" ]
    [ "${lines[1]}" = "three" ]
}

@test "nameref: unset through nameref unsets target" {
    run "$COLONY" -c 'target=value; declare -n ref=target; unset ref; echo "target=${target:-unset}"'
    [ "$status" -eq 0 ]
    [ "$output" = "target=unset" ]
}

@test "nameref: unset -n unsets nameref itself, not target" {
    run "$COLONY" -c 'target=value; declare -n ref=target; unset -n ref; echo "target=$target ref=${ref:-unset}"'
    [ "$status" -eq 0 ]
    [ "$output" = "target=value ref=unset" ]
}

@test "nameref: nameref chain (ref to ref)" {
    run "$COLONY" -c 'value=hello; declare -n ref1=value; declare -n ref2=ref1; echo $ref2'
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}

@test "nameref: circular reference detection" {
    run timeout 2 "$COLONY" -c 'declare -n a=b; declare -n b=a; echo ${a:-empty}'
    [ "$status" -eq 0 ]
    [ "$output" = "empty" ]
}

@test "nameref: self-reference detection" {
    run timeout 2 "$COLONY" -c 'declare -n self=self; echo ${self:-empty}'
    [ "$status" -eq 0 ]
    [ "$output" = "empty" ]
}

@test "nameref: declare -n without value" {
    run "$COLONY" -c 'declare -n ref; ref=hello; echo ${ref:-empty}'
    [ "$status" -eq 0 ]
}

@test "nameref: nameref to undefined variable" {
    run "$COLONY" -c 'declare -n ref=undefined; echo "${ref:-default}"'
    [ "$status" -eq 0 ]
    [ "$output" = "default" ]
}

@test "nameref: setting undefined target through nameref" {
    run "$COLONY" -c 'declare -n ref=newvar; ref=created; echo $newvar'
    [ "$status" -eq 0 ]
    [ "$output" = "created" ]
}

@test "nameref: nameref with combined flags declare -n" {
    run "$COLONY" -c 'target=value; declare -n myref=target; echo $myref'
    [ "$status" -eq 0 ]
    [ "$output" = "value" ]
}

@test "nameref: empty nameref assignment" {
    run "$COLONY" -c 'target=; declare -n ref=target; echo "value=[$ref]"'
    [ "$status" -eq 0 ]
    [ "$output" = "value=[]" ]
}

@test "nameref: nameref preserves target type for strings" {
    run "$COLONY" -c 'str="hello world"; declare -n ref=str; echo "$ref"'
    [ "$status" -eq 0 ]
    [ "$output" = "hello world" ]
}

@test "nameref: matches bash behavior for basic read" {
    colony_result=$("$COLONY" -c 'x=10; declare -n ref=x; echo $ref' 2>&1)
    bash_result=$(bash -c 'x=10; declare -n ref=x; echo $ref' 2>&1)
    [ "$colony_result" = "$bash_result" ]
}

@test "nameref: matches bash behavior for basic write" {
    colony_result=$("$COLONY" -c 'x=10; declare -n ref=x; ref=20; echo $x' 2>&1)
    bash_result=$(bash -c 'x=10; declare -n ref=x; ref=20; echo $x' 2>&1)
    [ "$colony_result" = "$bash_result" ]
}
