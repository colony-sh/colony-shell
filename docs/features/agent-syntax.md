# Agent Invocation Syntax (F059)

## Overview

Colony Shell introduces native syntax for invoking AI agents and their tools directly from the command line. The `@agent` syntax provides a concise, pipeline-friendly way to interact with LLM-powered agents without explicit function calls or configuration.

## Syntax

### Basic Agent Invocation

```colony
@<agent_name> "<prompt>"
@<agent_name> <unquoted_word>
```

### Agent with Tool Invocation

```colony
@<agent_name>:<tool_name> "<prompt>"
@<agent_name>:<tool_name> <unquoted_word>
```

## Basic Usage

### Simple Agent Call

```colony
@coding "explain this function"
@reviewer "check for security issues"
@assistant help
```

### Agent with Specific Tool

```colony
@search:web "latest Rust async news"
@analyzer:sql "optimize this query"
@devops:k8s "list all failing pods"
```

## Agent Context Variables

Agent invocations automatically set environment variables for introspection:

- `$AGENT_NAME` - The agent name (e.g., "coding", "search")
- `$AGENT_PROMPT` - The prompt text provided
- `$AGENT_TOOL` - The tool name if specified (e.g., "web", "sql")

### Example: Custom Agent Implementation

```colony
# Define a simple agent handler
proc handle_agent() {
    echo "Agent: $AGENT_NAME";
    echo "Tool: ${AGENT_TOOL:-none}";
    echo "Prompt: $AGENT_PROMPT";

    # Route to appropriate handler based on agent name
    match $AGENT_NAME {
        "search" => { perform_search "$AGENT_PROMPT"; }
        "analyzer" => { run_analysis "$AGENT_PROMPT"; }
        _ => { echo "Unknown agent: $AGENT_NAME"; }
    }
}
```

## Pipeline Integration

Agents work seamlessly in pipelines:

### Agent as Data Source

```colony
@generator "create sample data" | jq . | save_to_db
```

### Agent as Processor

```colony
cat error.log | @analyzer "find root cause" | tee analysis.txt
```

### Agent as Sink

```colony
git diff | @reviewer "check for issues"
```

### Chained Agents

```colony
@researcher "latest AI trends" | @summarizer "key points" | @formatter "markdown"
```

## Prompt Construction

### Variable Expansion

```colony
file="src/main.rs"
@reviewer "check $file for bugs"
# Expands to: @reviewer "check src/main.rs for bugs"
```

### Command Substitution

```colony
@analyzer "process $(cat data.txt)"
# Prompt includes content of data.txt
```

### Quoted vs Unquoted

```colony
@agent hello           # Prompt: "hello"
@agent "hello world"   # Prompt: "hello world"
@agent "$var"          # Prompt: expanded variable
@agent '$var'          # Prompt: literal "$var"
```

## Agent Name Conventions

Agent names support:
- Alphanumeric characters: `@agent123`
- Hyphens: `@my-agent`
- Underscores: `@my_agent`
- Mixed: `@ai-assistant_v2`

Tool names follow the same rules:
- `@agent:tool`
- `@my-agent:my-tool`
- `@assistant:search_web`

## Control Flow Integration

### With Conditionals

```colony
if @validator "check config"; then
    echo "Config valid";
else
    echo "Config invalid";
fi
```

### With Command Lists

```colony
@builder "compile project" && echo "Build succeeded"
@tester "run tests" || echo "Tests failed"
```

### With Match Statements

```colony
match $action {
    "analyze" => { @analyzer "review code"; }
    "test" => { @tester "run all tests"; }
    _ => { echo "Unknown action"; }
}
```

## Implementation Details

### Files Involved

- `crates/colony-parser/colony.pest` - Grammar for @agent syntax
- `crates/colony-parser/src/ast.rs` - AgentInvocation AST node
- `crates/colony-parser/src/builder.rs` - Parse tree to AST conversion
- `crates/colony-shell/src/shell/execution/llm.rs` - Agent execution logic
- `crates/colony-agent/src/router.rs` - Agent routing and dispatch
- `tests/scripts/bash-compat/54_agent_syntax.bats` - 20 comprehensive tests

### Data Flow

1. Lexer recognizes `@` prefix and agent name
2. Parser extracts agent name, optional tool, and prompt
3. AST builder creates AgentInvocation node
4. Executor sets AGENT_NAME, AGENT_TOOL, AGENT_PROMPT variables
5. Agent router dispatches to appropriate agent implementation
6. Agent processes prompt and returns output
7. Output flows to stdout or next pipeline stage

### Exit Status

Agent invocations return exit status like any command:
- `0` - Success
- Non-zero - Failure

This enables proper control flow:

```colony
@agent "task" && echo "Success" || echo "Failed"
```

## Examples

### Code Review Workflow

```colony
proc review_pr(branch) {
    git diff main..$branch \
        |> tee pr.diff \
        | @reviewer:code "find issues" \
        |> tee review.txt \
        | @formatter:markdown \
        > review.md

    echo "Review saved to review.md";
}
```

### Multi-Agent Data Processing

```colony
proc analyze_logs(log_file) {
    cat "$log_file" \
        | @extractor "find error patterns" \
        | @classifier "categorize by severity" \
        | @summarizer "create executive summary" \
        > report.txt
}
```

### Interactive Agent Session

```colony
proc chat_with_agent(agent_name) {
    while read -p "> " prompt; do
        if [ -z "$prompt" ]; then
            break;
        fi
        @$agent_name "$prompt";
    done
}

chat_with_agent assistant
```

### Conditional Agent Routing

```colony
proc smart_handler(input) {
    match $input {
        q if [[ $q == *"code"* ]] => { @coder "$q"; }
        q if [[ $q == *"search"* ]] => { @search:web "$q"; }
        q if [[ $q == *"analyze"* ]] => { @analyzer "$q"; }
        q => { @general "$q"; }
    }
}
```

### Agent-Driven Build System

```colony
proc ai_build() {
    @planner "analyze project and create build plan" \
        | @executor:build "execute plan" \
        | @validator "verify build artifacts" \
        | @reporter "generate build report";

    if [ $? -eq 0 ]; then
        @notifier "build succeeded";
    else
        @debugger "analyze build failure" | tee debug.log;
    fi
}
```

## Best Practices

### Prompt Clarity

```colony
# ❌ Vague
@agent "fix it"

# ✅ Specific
@reviewer "check for SQL injection vulnerabilities in auth.rs"
```

### Error Handling

```colony
# ✅ Always handle agent failures
@agent "task" || {
    echo "Agent failed, using fallback";
    manual_handler;
}
```

### Context Passing

```colony
# ✅ Provide context via stdin
git log --oneline -10 | @summarizer "describe recent changes"

# ✅ Or via prompt
@analyzer "review file: $(cat config.yaml)"
```

### Agent Naming

```colony
# ✅ Clear, descriptive names
@security-scanner "check dependencies"
@doc-generator "create API docs"

# ❌ Ambiguous names
@agent1 "do stuff"
@x "task"
```

## Integration with Recipe System

Agents are typically defined in recipe files:

```toml
# recipes/code-reviewer.toml
[agent]
name = "reviewer"
provider = "anthropic"
model = "claude-opus-4"

[agent.tools.code]
description = "Review code for issues"
prompt = "You are an expert code reviewer..."

[agent.tools.security]
description = "Security-focused review"
prompt = "Focus on security vulnerabilities..."
```

Invoke with:

```colony
@reviewer:code "check main.rs"
@reviewer:security "audit auth system"
```

## Testing

The agent syntax has 20 comprehensive BATS tests covering:

- Basic agent invocation with quoted and unquoted prompts
- AGENT_NAME, AGENT_PROMPT, AGENT_TOOL variable setting
- Agent:tool invocation syntax
- Various tool and agent name formats
- Pipeline integration (pipe to/from agents)
- Variable expansion in prompts
- Command substitution in prompts
- Agent name variants (hyphens, underscores, numbers)
- Command list operators (&&, ||, ;)
- Exit status handling

Run tests with:

```bash
bats tests/scripts/bash-compat/54_agent_syntax.bats
```

## Comparison with Other Approaches

### Traditional CLI Tools

```bash
# Traditional: explicit command
llm-client --agent reviewer --prompt "check code"

# Colony: native syntax
@reviewer "check code"
```

### API Calls

```bash
# API call: verbose
curl -X POST api.example.com/agents/reviewer \
    -H "Content-Type: application/json" \
    -d '{"prompt": "check code"}'

# Colony: concise
@reviewer "check code"
```

### Script Functions

```bash
# Function wrapper: manual
function call_agent() {
    agent=$1
    shift
    prompt="$*"
    # ... complex invocation logic ...
}
call_agent reviewer check code

# Colony: built-in
@reviewer "check code"
```

## Future Enhancements

Potential future additions:

- Streaming responses: `@agent "prompt" | stream`
- Async agents: `@agent "task" &` (background)
- Agent composition: `@agent1 + @agent2 "collaborate"`
- Agent state: `@agent.save_context` / `@agent.load_context`
- Agent configuration: `@agent[model=gpt4] "prompt"`
- Multi-agent coordination: `@team[reviewer,tester] "workflow"`
