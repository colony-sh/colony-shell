#!/usr/bin/env bats
# Test system variables (UID, EUID, PPID, HOSTNAME, HOSTTYPE, OSTYPE, MACHTYPE, USER, SHELL)

COLONY="${COLONY_BIN}"

@test "UID - real user ID" {
    run "$COLONY" -c 'echo $UID'
    [ "$status" -eq 0 ]
    [[ "$output" =~ ^[0-9]+$ ]]
    
    bash_uid=$(bash -c 'echo $UID')
    [ "$output" = "$bash_uid" ]
}

@test "EUID - effective user ID" {
    run "$COLONY" -c 'echo $EUID'
    [ "$status" -eq 0 ]
    [[ "$output" =~ ^[0-9]+$ ]]
    
    bash_euid=$(bash -c 'echo $EUID')
    [ "$output" = "$bash_euid" ]
}

@test "PPID - parent process ID" {
    run "$COLONY" -c 'echo $PPID'
    [ "$status" -eq 0 ]
    [[ "$output" =~ ^[0-9]+$ ]]
}

@test "HOSTNAME - machine hostname" {
    run "$COLONY" -c 'echo $HOSTNAME'
    [ "$status" -eq 0 ]
    [ -n "$output" ]
    
    bash_hostname=$(bash -c 'echo $HOSTNAME')
    [ "$output" = "$bash_hostname" ]
}

@test "HOSTTYPE - CPU architecture" {
    run "$COLONY" -c 'echo $HOSTTYPE'
    [ "$status" -eq 0 ]
    [ -n "$output" ]
    
    bash_hosttype=$(bash -c 'echo $HOSTTYPE')
    [ "$output" = "$bash_hosttype" ]
}

@test "OSTYPE - OS type with version" {
    run "$COLONY" -c 'echo $OSTYPE'
    [ "$status" -eq 0 ]
    [[ "$output" =~ ^(darwin|linux) ]]
}

@test "MACHTYPE - full machine type" {
    run "$COLONY" -c 'echo $MACHTYPE'
    [ "$status" -eq 0 ]
    [[ "$output" =~ ^[a-z0-9_]+-[a-z]+-[a-z] ]]
}

@test "USER - current username" {
    run "$COLONY" -c 'echo $USER'
    [ "$status" -eq 0 ]
    [ -n "$output" ]
    
    bash_user=$(bash -c 'echo $USER')
    [ "$output" = "$bash_user" ]
}

@test "SHELL - shell path from environment" {
    run "$COLONY" -c 'echo $SHELL'
    [ "$status" -eq 0 ]
    [ -n "$output" ]
    
    bash_shell=$(bash -c 'echo $SHELL')
    [ "$output" = "$bash_shell" ]
}

@test "UID is readonly - cannot be assigned" {
    run "$COLONY" -c 'UID=999; echo $UID'
    [ "$status" -eq 0 ]
    [ "$output" != "999" ]
}

@test "EUID is readonly - cannot be assigned" {
    run "$COLONY" -c 'EUID=999; echo $EUID'
    [ "$status" -eq 0 ]
    [ "$output" != "999" ]
}

@test "PPID is readonly - cannot be assigned" {
    run "$COLONY" -c 'PPID=999; echo $PPID'
    [ "$status" -eq 0 ]
    [ "$output" != "999" ]
}

@test "HOSTNAME is readonly - cannot be assigned" {
    run "$COLONY" -c 'HOSTNAME=fake; echo $HOSTNAME'
    [ "$status" -eq 0 ]
    [ "$output" != "fake" ]
}

@test "all system vars in single command" {
    run "$COLONY" -c 'echo "UID=$UID EUID=$EUID PPID=$PPID HOSTNAME=$HOSTNAME HOSTTYPE=$HOSTTYPE OSTYPE=$OSTYPE MACHTYPE=$MACHTYPE USER=$USER SHELL=$SHELL"'
    [ "$status" -eq 0 ]
    [[ "$output" =~ "UID=" ]]
    [[ "$output" =~ "EUID=" ]]
    [[ "$output" =~ "PPID=" ]]
    [[ "$output" =~ "HOSTNAME=" ]]
    [[ "$output" =~ "HOSTTYPE=" ]]
    [[ "$output" =~ "OSTYPE=" ]]
    [[ "$output" =~ "MACHTYPE=" ]]
    [[ "$output" =~ "USER=" ]]
    [[ "$output" =~ "SHELL=" ]]
}
