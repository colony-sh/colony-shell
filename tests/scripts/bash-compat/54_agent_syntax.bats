#!/usr/bin/env bats
# Test Colony shell agent invocation syntax (F059)
# Colony-specific feature - @agent_name "prompt" syntax

COLONY="${COLONY_BIN}"

# ============================================================================
# Basic Agent Invocation
# ============================================================================

@test "agent: basic invocation with quoted prompt" {
    run $COLONY -c '@myagent "hello world"'
    [ "$status" -eq 0 ]
    [ "$output" = "@myagent hello world" ]
}

@test "agent: invocation with unquoted word" {
    run $COLONY -c '@myagent hello'
    [ "$status" -eq 0 ]
    [ "$output" = "@myagent hello" ]
}

@test "agent: invocation sets AGENT_NAME variable" {
    run $COLONY -c '@myagent "test"; echo "name=$AGENT_NAME"'
    [ "$status" -eq 0 ]
    [ "${lines[1]}" = "name=myagent" ]
}

@test "agent: invocation sets AGENT_PROMPT variable" {
    run $COLONY -c '@myagent "hello world"; echo "prompt=$AGENT_PROMPT"'
    [ "$status" -eq 0 ]
    [ "${lines[1]}" = "prompt=hello world" ]
}

# ============================================================================
# Agent:Tool Invocation
# ============================================================================

@test "agent: tool invocation" {
    run $COLONY -c '@myagent:search "find stuff"'
    [ "$status" -eq 0 ]
    [ "$output" = "@myagent:search find stuff" ]
}

@test "agent: tool invocation sets AGENT_TOOL variable" {
    run $COLONY -c '@myagent:search "test"; echo "tool=$AGENT_TOOL"'
    [ "$status" -eq 0 ]
    [ "${lines[1]}" = "tool=search" ]
}

@test "agent: different tool names" {
    run $COLONY -c '@agent:analyze "data"'
    [ "$status" -eq 0 ]
    [ "$output" = "@agent:analyze data" ]
}

# ============================================================================
# Agent in Pipelines
# ============================================================================

@test "agent: pipe output to agent" {
    run $COLONY -c 'echo "some data" | @analyzer "process this"'
    [ "$status" -eq 0 ]
    [ "$output" = "@analyzer process this" ]
}

@test "agent: agent output piped to command" {
    run $COLONY -c '@myagent "hello" | cat'
    [ "$status" -eq 0 ]
    [ "$output" = "@myagent hello" ]
}

@test "agent: agent in middle of pipeline" {
    run $COLONY -c 'echo test | @agent "analyze" | cat'
    [ "$status" -eq 0 ]
    [ "$output" = "@agent analyze" ]
}

# ============================================================================
# Variable Expansion in Prompt
# ============================================================================

@test "agent: variable expansion in prompt" {
    run $COLONY -c 'msg="greetings"; @myagent "$msg"'
    [ "$status" -eq 0 ]
    [ "$output" = "@myagent greetings" ]
}

@test "agent: command substitution in prompt" {
    run $COLONY -c '@myagent "$(echo expanded)"'
    [ "$status" -eq 0 ]
    [ "$output" = "@myagent expanded" ]
}

# ============================================================================
# Agent Name Variants
# ============================================================================

@test "agent: hyphenated agent name" {
    run $COLONY -c '@my-agent "hello"'
    [ "$status" -eq 0 ]
    [ "$output" = "@my-agent hello" ]
}

@test "agent: underscored agent name" {
    run $COLONY -c '@my_agent "hello"'
    [ "$status" -eq 0 ]
    [ "$output" = "@my_agent hello" ]
}

@test "agent: numeric in agent name" {
    run $COLONY -c '@agent2 "hello"'
    [ "$status" -eq 0 ]
    [ "$output" = "@agent2 hello" ]
}

# ============================================================================
# Agent with Command List Operators
# ============================================================================

@test "agent: followed by && on success" {
    run $COLONY -c '@myagent "test" && echo "success"'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "@myagent test" ]
    [ "${lines[1]}" = "success" ]
}

@test "agent: followed by semicolon" {
    run $COLONY -c '@myagent "first"; echo "second"'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "@myagent first" ]
    [ "${lines[1]}" = "second" ]
}

@test "agent: preceded by command" {
    run $COLONY -c 'echo "before"; @myagent "after"'
    [ "$status" -eq 0 ]
    [ "${lines[0]}" = "before" ]
    [ "${lines[1]}" = "@myagent after" ]
}

# ============================================================================
# Exit Status
# ============================================================================

@test "agent: returns success by default" {
    run $COLONY -c '@myagent "hello"'
    [ "$status" -eq 0 ]
}

@test "agent: exit status usable in && chain" {
    run $COLONY -c '@myagent "test" && echo ok'
    [ "$status" -eq 0 ]
    [ "${lines[1]}" = "ok" ]
}
