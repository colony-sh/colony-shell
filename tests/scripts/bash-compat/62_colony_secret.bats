#!/usr/bin/env bats
# `colony secret`: argument and input checks. None of these write to the OS keychain.

COLONY="${COLONY_BIN}"

@test "colony secret says how to use it" {
    run "$COLONY" -c 'colony secret'
    [ "$status" -eq 2 ]
    [[ "$output" == *"usage: colony secret set|has|rm <NAME>"* ]]
    run "$COLONY" -c 'colony secret get ANTHROPIC_API_KEY'
    [ "$status" -eq 2 ]
}

@test "colony secret refuses a bad name before touching the keychain" {
    for name in lower 1ABC A-B; do
        run "$COLONY" -c "colony secret set $name"
        [ "$status" -eq 2 ]
        [[ "$output" == *"bad name '$name'"* ]]
    done
}

@test "colony secret set stores nothing for an empty value" {
    run bash -c "printf '\n' | '$COLONY' -c 'colony secret set COLONY_TEST_NEVER_STORED'"
    [ "$status" -eq 1 ]
    [[ "$output" == *"empty value, nothing stored"* ]]
}

@test "colony secret has reports a name that was never stored" {
    run "$COLONY" -c 'colony secret has COLONY_TEST_NEVER_STORED'
    [ "$status" -eq 1 ]
    [ "$output" = "COLONY_TEST_NEVER_STORED: not stored" ]
}
