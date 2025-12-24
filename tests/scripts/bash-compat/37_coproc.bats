#!/usr/bin/env bats

COLONY="${COLONY_BIN}"

@test "coproc: anonymous coprocess sets COPROC array" {
    run $COLONY -c 'coproc { echo hello; }; echo ${COPROC[@]}'
    [ "$status" -eq 0 ]
    [[ "$output" =~ ^[0-9]+\ [0-9]+$ ]]
}

@test "coproc: anonymous coprocess sets COPROC_PID" {
    run $COLONY -c 'coproc { sleep 0.1; }; echo $COPROC_PID'
    [ "$status" -eq 0 ]
    [[ "$output" =~ ^[0-9]+$ ]]
}

@test "coproc: read from coprocess stdout" {
    run $COLONY -c 'coproc { echo "hello world"; }; sleep 0.1; read line <&${COPROC[0]}; echo "$line"'
    [ "$status" -eq 0 ]
    [ "$output" = "hello world" ]
}

@test "coproc: write to coprocess stdin" {
    run $COLONY -c 'coproc { read x; echo "got: $x"; }; echo "test" >&${COPROC[1]}; sleep 0.1; read line <&${COPROC[0]}; echo "$line"'
    [ "$status" -eq 0 ]
    [ "$output" = "got: test" ]
}

@test "coproc: named coprocess sets NAME array" {
    run $COLONY -c 'coproc MYPROC { echo done; }; echo ${MYPROC[@]}'
    [ "$status" -eq 0 ]
    [[ "$output" =~ ^[0-9]+\ [0-9]+$ ]]
}

@test "coproc: named coprocess sets NAME_PID" {
    run $COLONY -c 'coproc MYPROC { sleep 0.1; }; echo $MYPROC_PID'
    [ "$status" -eq 0 ]
    [[ "$output" =~ ^[0-9]+$ ]]
}

@test "coproc: read from named coprocess" {
    run $COLONY -c 'coproc READER { echo "from reader"; }; sleep 0.1; read line <&${READER[0]}; echo "$line"'
    [ "$status" -eq 0 ]
    [ "$output" = "from reader" ]
}

@test "coproc: bidirectional communication" {
    run $COLONY -c 'coproc ECHO { while read line; do echo "echo: $line"; done; }; echo "first" >&${ECHO[1]}; read resp <&${ECHO[0]}; echo "$resp"; exec {ECHO[1]}>&-'
    [ "$status" -eq 0 ]
    [ "$output" = "echo: first" ]
}

@test "coproc: simple command form uses COPROC name" {
    run $COLONY -c 'coproc cat; echo test >&${COPROC[1]}; exec {COPROC[1]}>&-; read line <&${COPROC[0]}; echo "$line"'
    [ "$status" -eq 0 ]
    [ "$output" = "test" ]
}

@test "coproc: wait for coprocess" {
    run $COLONY -c 'coproc { exit 42; }; wait $COPROC_PID; echo $?'
    [ "$status" -eq 0 ]
    [ "$output" = "42" ]
}

@test "coproc: multiple reads from coprocess" {
    run $COLONY -c 'coproc { echo line1; echo line2; echo line3; }; sleep 0.1; read l1 <&${COPROC[0]}; read l2 <&${COPROC[0]}; read l3 <&${COPROC[0]}; echo "$l1 $l2 $l3"'
    [ "$status" -eq 0 ]
    [ "$output" = "line1 line2 line3" ]
}

@test "coproc: subshell as coprocess" {
    run $COLONY -c 'coproc SUBSH ( echo from subshell ); sleep 0.1; read line <&${SUBSH[0]}; echo "$line"'
    [ "$status" -eq 0 ]
    [ "$output" = "from subshell" ]
}

@test "coproc: array element 0 is read fd" {
    run $COLONY -c 'coproc { echo test; }; [[ ${COPROC[0]} =~ ^[0-9]+$ ]] && echo "valid fd"'
    [ "$status" -eq 0 ]
    [ "$output" = "valid fd" ]
}

@test "coproc: array element 1 is write fd" {
    run $COLONY -c 'coproc { cat; }; [[ ${COPROC[1]} =~ ^[0-9]+$ ]] && echo "valid fd"; exec {COPROC[1]}>&-'
    [ "$status" -eq 0 ]
    [ "$output" = "valid fd" ]
}

@test "coproc: coprocess in pipeline reads" {
    run $COLONY -c 'coproc { echo apple; echo banana; echo cherry; }; sleep 0.1; cat <&${COPROC[0]} | sort'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "apple" ]
    [ "${lines[1]}" = "banana" ]
    [ "${lines[2]}" = "cherry" ]
}
