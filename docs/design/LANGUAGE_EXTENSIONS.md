# Colony Shell Language Extensions

Colony Shell is **POSIX-compliant first**, but extends the language with modern features that make shell scripting productive, safe, and AI-native. This document describes our language extensions and the philosophy behind them.

## Design Philosophy

### Core Principles

1. **Valid Bash = Valid Colony**: Any script that works in bash should work identically in Colony
2. **Extensions are Opt-In**: New syntax never breaks existing scripts
3. **Graceful Degradation**: Colony scripts can include fallbacks for bash execution
4. **Progressive Enhancement**: Start with bash, adopt Colony features incrementally
5. **Explicit Over Magic**: New features are clearly distinguished from POSIX syntax

### Compatibility Modes

```bash
#!/usr/bin/env colony          # Full Colony features
#!/usr/bin/env colony --posix  # Strict POSIX mode (disable extensions)
#!/usr/bin/env colony-sh       # POSIX-only binary (no extensions compiled in)
```

---

## Extension Categories

| Category | Syntax | Purpose |
|----------|--------|---------|
| **Typed Variables** | `var`, `const` | Type safety, immutability |
| **Structured Data** | `{}`, `[]` | Native dicts and arrays |
| **Expression Syntax** | `$[expr]` | Safe expression evaluation |
| **Extended Channels** | `>&ctx`, `>&llm` | Semantic I/O beyond stdio |
| **Agent Integration** | `@agent`, `#` | Natural language interface |
| **Pipeline Extensions** | `|>`, `?>`, `!>` | Richer pipeline control |
| **Error Handling** | `try/catch`, `?` | Structured error handling |
| **Namespaced Builtins** | `git:status` | Command namespacing |
| **Proc Definitions** | `proc name {}` | First-class procedures |
| **Pattern Matching** | `match` | Modern control flow |

---

## 1. Typed Variables

### Problem with Bash

```bash
# Bash: Everything is a string, implicit conversions, no immutability
count=5
count="hello"  # No error, silently works
readonly PI=3.14159  # readonly is clunky
```

### Colony Extension

```colony
# Colony: Optional typing, const for immutability
var count = 5              # Inferred as integer
var name: str = "colony"   # Explicit type annotation
const PI = 3.14159         # Immutable (cleaner than readonly)
const CONFIG: dict = {}    # Typed constant

# Type checking (optional, can be enforced via config)
var port: int = "hello"    # Warning or error depending on config

# Still works like bash when you want it to
count=5                    # Traditional assignment still works
```

### Type System

```colony
# Primitive types
var s: str = "hello"
var i: int = 42
var f: float = 3.14
var b: bool = true

# Compound types
var arr: list = [1, 2, 3]
var dict: dict = {name: "colony", version: 1}

# Type inference (default)
var x = 42        # Inferred as int
var y = [1, 2]    # Inferred as list

# Any type (explicit opt-out of type checking)
var anything: any = "could be anything"
```

### Compatibility

```bash
# This bash script works identically in Colony:
name="world"
echo "Hello, $name"  # Works exactly like bash

# Colony additions are purely additive:
var name = "world"   # New syntax, same result
echo "Hello, $name"  # Same output
```

---

## 2. Structured Data

### Problem with Bash

```bash
# Bash: Arrays are awkward, no native dicts (associative arrays are bash 4+)
declare -a arr=(1 2 3)
declare -A dict=([name]="value")  # Clunky syntax
echo ${arr[0]}                     # Easy to forget braces
echo ${dict[name]}                 # Inconsistent with other languages
```

### Colony Extension

```colony
# Colony: Native list and dict literals
var arr = [1, 2, 3, "mixed", true]
var dict = {
    name: "colony",
    version: 1,
    features: ["channels", "agents"]
}

# Access with familiar syntax
echo $arr[0]           # First element
echo $dict.name        # Dot notation for dicts
echo $dict["name"]     # Bracket notation also works

# Nested structures
var config = {
    server: {
        host: "localhost",
        port: 8080
    },
    features: ["auth", "logging"]
}
echo $config.server.port    # 8080
echo $config.features[0]    # "auth"

# Spread operator
var defaults = {timeout: 30, retries: 3}
var custom = {...$defaults, timeout: 60}  # Override timeout
```

### JSON Interoperability

```colony
# Parse JSON directly
var data = $(curl -s api.example.com/data | json:parse)
echo $data.users[0].name

# Output as JSON
echo $config | json:encode

# Inline JSON (automatically parsed)
var response = '{"status": "ok", "code": 200}':json
echo $response.status  # "ok"
```

### Compatibility

```bash
# Traditional bash arrays still work exactly as expected:
arr=(1 2 3)
echo ${arr[0]}  # Works identically

# Bash associative arrays work too:
declare -A dict
dict[name]="value"
echo ${dict[name]}  # Works identically

# Colony syntax is additive, not replacement
```

---

## 3. Expression Syntax: `$[expr]`

### Problem with Bash

```bash
# Bash: $(()) for arithmetic, $() for commands, ${} for parameters
# Easy to confuse, limited expressions
result=$((1 + 2 * 3))           # Arithmetic only
length=${#string}                # String length
upper=$(echo "$s" | tr a-z A-Z)  # Need subprocess for string ops
```

### Colony Extension

```colony
# Colony: $[expr] for rich expressions
var result = $[1 + 2 * 3]              # Arithmetic
var length = $[len(string)]            # Function call
var upper = $[string.upper()]          # Method call
var filtered = $[arr.filter(x => x > 0)]  # Lambda
var found = $[dict.get("key", "default")] # Default values

# String interpolation in expressions
var greeting = $["Hello, " + name + "!"]
var path = $[home + "/config/" + filename]

# Conditional expressions
var status = $[code == 200 ? "ok" : "error"]
var display = $[name ?? "anonymous"]  # Null coalescing

# Complex expressions
var total = $[
    items
        .filter(i => i.active)
        .map(i => i.price * i.quantity)
        .sum()
]
```

### Built-in Functions

```colony
# String functions
$[len(s)]              # Length
$[s.upper()]           # Uppercase
$[s.lower()]           # Lowercase
$[s.trim()]            # Trim whitespace
$[s.split(",")]        # Split to array
$[s.replace("a", "b")] # Replace
$[s.starts_with("pre")]# Prefix check
$[s.contains("sub")]   # Substring check

# Array functions
$[len(arr)]            # Length
$[arr.first()]         # First element
$[arr.last()]          # Last element
$[arr.join(",")]       # Join to string
$[arr.reverse()]       # Reverse
$[arr.sort()]          # Sort
$[arr.filter(fn)]      # Filter
$[arr.map(fn)]         # Map
$[arr.reduce(fn, init)]# Reduce

# Dict functions
$[dict.keys()]         # Get keys
$[dict.values()]       # Get values
$[dict.get(key, def)]  # Get with default
$[dict.has(key)]       # Check key exists
$[dict.merge(other)]   # Merge dicts

# Math functions
$[abs(n)]              # Absolute value
$[min(a, b)]           # Minimum
$[max(a, b)]           # Maximum
$[floor(n)]            # Floor
$[ceil(n)]             # Ceiling
$[round(n, places)]    # Round
```

### Compatibility

```bash
# Traditional bash arithmetic still works:
result=$((1 + 2))
echo $result  # 3

# Traditional command substitution still works:
files=$(ls)
echo $files

# $[expr] is NEW syntax, doesn't conflict with anything
```

---

## 4. Extended Channels

### Problem with Bash

```bash
# Bash: Only stdin(0), stdout(1), stderr(2)
# No structured way to pass context, metadata, or semantic information
# Custom file descriptors are possible but awkward:
exec 3>/tmp/logfile
echo "log" >&3
```

### Colony Extension

```colony
# Colony: Semantic channels with meaning
# fd 0-2: Traditional (stdin, stdout, stderr)
# fd 3: stdcontext - Session context, observations, decisions
# fd 4: stdllm - LLM conversation history
# fd 5: stdtool - Tool invocations and results
# fd 6: stdtrace - Execution trace (debug)

# Write to context channel
echo "Starting deployment to production" >&ctx
echo "Decision: Using blue-green strategy" >&context

# Write structured data
context "observation" "API latency increased by 20%"
context --type decision "Chose canary deployment"

# Agent can read accumulated context
agent:ask "What have we done so far?"  # Sees context channel

# LLM conversation channel
llm:user "Explain this error"
llm:show  # View conversation history

# Channels are captured and associated with session
# Can be exported, searched, and analyzed
context:export > session-notes.json
```

### Channel Aliases

```colony
# By number (POSIX-compatible)
echo "note" >&3    # stdcontext
echo "msg" >&4     # stdllm

# By name (Colony extension)
echo "note" >&ctx
echo "note" >&context
echo "msg" >&llm
echo "trace" >&trace
```

### Compatibility

```bash
# Traditional redirects work exactly as expected:
echo "error" >&2           # stderr
command > file.txt         # stdout to file
command 2>&1               # stderr to stdout

# Custom file descriptors still work:
exec 3>/tmp/mylog
echo "log" >&3

# Colony named channels are purely additive
```

---

## 5. Agent Integration

### Problem with Bash

```bash
# Bash: No AI integration
# Must shell out to external tools, parse responses manually
response=$(curl -s https://api.openai.com/v1/... | jq -r '.choices[0].message.content')
```

### Colony Extension

```colony
# Natural language prefix (# or ?)
# list all large files in this directory
→ Agent interprets, generates: find . -size +100M -type f

# Question syntax
? how do I find files modified today
→ Agent responds with explanation and command

# Explicit agent invocation
agent:ask "Explain what this script does"
agent:run "Refactor this function to be more readable"

# Agent with context
context "I'm debugging a memory leak"
agent:ask "What should I check next?"
→ Agent sees context, gives relevant advice

# Agent in pipeline (agent processes data)
cat error.log | agent:analyze "Summarize these errors"
git diff | agent:review "Review this code change"

# Tool use (agent can run commands with approval)
agent:do "Set up a Python virtual environment here"
→ Agent proposes: python -m venv .venv && source .venv/bin/activate
→ User approves or modifies

# Multi-turn conversation
llm:chat
> What's wrong with this Dockerfile?
< [Agent analyzes and responds]
> How do I fix the layer caching issue?
< [Agent continues conversation]
```

### Agent Configuration

```colony
# In .colonyrc
[agent]
personality = "You are a senior DevOps engineer..."
temperature = 0.7
max_tokens = 4096
tools = ["file_read", "file_write", "command_run"]
require_approval = ["file_write", "command_run"]

# Per-session override
agent:config temperature=0.3  # More deterministic
agent:config require_approval=all  # Always ask
```

### Compatibility

```bash
# The # character for comments still works normally:
# This is still a comment in Colony
echo "hello"  # This is also a comment

# Agent mode is triggered by # at START of input line in interactive mode
# Or by explicit agent: prefix in scripts
# Scripts remain fully compatible
```

---

## 6. Pipeline Extensions

### Problem with Bash

```bash
# Bash: Pipelines only pass stdout, error handling is awkward
command1 | command2 | command3
# If command2 fails, command3 still runs
# No easy way to handle errors mid-pipeline
# No way to pass structured data
```

### Colony Extension

```colony
# Standard pipe (unchanged, POSIX-compatible)
cat file | grep pattern | wc -l

# Error-aware pipeline: ?> (stop on error)
command1 ?> command2 ?> command3
# If command1 fails, pipeline stops immediately

# Tap pipeline: |> (inspect without consuming)
data | transform |> log:debug | output
# |> sends copy to log:debug, original continues to output

# Split pipeline: &> (parallel execution)
data &> [process1, process2, process3]
# Runs all three in parallel, collects results

# Structured pipeline (pass data, not just text)
fetch:json "https://api.example.com/users" \
    | filter '.active == true' \
    | map '.name' \
    | join ", "

# Pipeline with error handler
command1 | command2 | command3 !> handle_error
# !> catches any error in pipeline, runs handler

# Conditional pipeline
test -f config.json && cat config.json | process
test -f config.json ?> cat config.json | process  # Same but clearer

# Pipeline status
result=$(command1 | command2)
echo $[PIPESTATUS]  # Array of exit codes: [0, 0]
```

### Compatibility

```bash
# All standard bash pipeline syntax works:
cat file | grep foo | head -10
command1 && command2
command1 || command2
command1 ; command2

# PIPESTATUS works the same:
false | true | false
echo ${PIPESTATUS[@]}  # 1 0 1

# New operators are NEW characters, no conflicts
```

---

## 7. Error Handling

### Problem with Bash

```bash
# Bash: Error handling is primitive
set -e  # Exit on error (too aggressive)
set -o pipefail  # Pipeline fails if any command fails
trap 'echo "Error on line $LINENO"' ERR  # Global trap

# No structured try/catch
# No error context
# Hard to write robust scripts
```

### Colony Extension

```colony
# Try/catch blocks
try {
    risky_command
    another_command
} catch {
    echo "Something failed: $ERROR"
    echo "Command: $ERROR_COMMAND"
    echo "Line: $ERROR_LINE"
    echo "Exit code: $ERROR_CODE"
}

# Try with specific error handling
try {
    connect_to_database
} catch ConnectionError {
    echo "Database unreachable, using cache"
    use_cache
} catch AuthError {
    echo "Authentication failed"
    exit 1
}

# Finally block
try {
    acquire_lock
    do_critical_work
} finally {
    release_lock  # Always runs
}

# Error propagation with ?
fn deploy() {
    build?          # If build fails, deploy fails immediately
    test?           # Same for test
    push_to_prod?   # And push
}

# Result type (optional)
fn divide(a, b): result {
    if $b == 0 {
        return err "Division by zero"
    }
    return ok $[$a / $b]
}

var result = divide 10 2
match $result {
    ok(value) => echo "Result: $value"
    err(msg) => echo "Error: $msg"
}

# Error context
try {
    deploy_to_production
} catch {
    # Rich error information
    echo "Error: $ERROR"
    echo "Stack trace:"
    for frame in $ERROR_STACK {
        echo "  $frame.file:$frame.line in $frame.function"
    }
}
```

### Compatibility

```bash
# Traditional bash error handling still works:
set -e
set -o pipefail
trap 'echo "Error"' ERR

command || handle_error
command && success || failure

if ! command; then
    echo "Failed"
fi

# try/catch is NEW syntax, doesn't conflict
```

---

## 8. Namespaced Builtins

### Problem with Bash

```bash
# Bash: All commands in flat namespace
# Collisions between builtins, functions, external commands
# No organization or discoverability
type echo  # Is it builtin? External? Alias?
```

### Colony Extension

```colony
# Colony: Namespace organization for discoverability
# Traditional commands still work (flat namespace)
echo "hello"
cd /tmp

# Namespaced builtins for colony-specific features
context:add "observation" "Starting deployment"
context:show
context:export > log.json

llm:user "Explain this"
llm:show
llm:branch "experiment"
llm:checkpoint "before-refactor"

agent:ask "How do I...?"
agent:config temperature=0.5

ts:out ("lock", "deploy", $SESSION)  # Tuple space
ts:in ("task", _, _)

mcp:list                   # List MCP servers
mcp:call server tool args  # Call MCP tool

buffer:create "scratch"
buffer:append "scratch" "data"
buffer:show "scratch"

# Git helpers (optional, not builtins)
git:status    # Pretty status
git:branch    # Branch management
git:stash     # Stash helpers

# File helpers
file:read "config.json" | json:parse
json:encode $data > output.json

# Process helpers
proc:list     # Like ps but structured
proc:kill 1234
proc:wait 1234
```

### Compatibility

```bash
# All existing commands work:
echo "hello"
cd /tmp
ls -la

# Namespaced commands are purely additive
# No existing bash syntax uses : in command names like this
# (Technically :: exists in some contexts, but cmd:sub is new)
```

---

## 9. Proc Definitions

### Problem with Bash

```bash
# Bash: Functions are primitive
# No named parameters
# No default values
# No type hints
# Awkward return values (exit codes or echo)

my_function() {
    local arg1=$1
    local arg2=${2:-default}
    echo "result"  # "Return" via stdout
}
result=$(my_function "value")
```

### Colony Extension

```colony
# Colony: Modern procedure definitions
proc greet(name: str, greeting: str = "Hello") {
    echo "$greeting, $name!"
}

greet "World"              # "Hello, World!"
greet "World" "Hi"         # "Hi, World!"
greet name="Colony" greeting="Welcome"  # Named arguments

# Return values (not just exit codes)
proc add(a: int, b: int): int {
    return $[$a + $b]
}
var sum = add 2 3  # sum = 5

# Multiple return values
proc divmod(a: int, b: int): (int, int) {
    return ($[$a / $b], $[$a % $b])
}
var (quotient, remainder) = divmod 10 3

# Optional parameters
proc fetch(url: str, timeout: int? = null) {
    if $timeout {
        curl --max-time $timeout $url
    } else {
        curl $url
    }
}

# Rest parameters
proc log(level: str, ...messages) {
    for msg in $messages {
        echo "[$level] $msg"
    }
}
log "INFO" "Starting" "Processing" "Done"

# Docstrings
proc deploy(env: str, version: str) {
    ## Deploy application to specified environment
    ## 
    ## Args:
    ##   env: Target environment (dev, staging, prod)
    ##   version: Version tag to deploy
    ##
    ## Example:
    ##   deploy prod v1.2.3
    
    echo "Deploying $version to $env"
}

help deploy  # Shows docstring
```

### Traditional Functions Still Work

```bash
# Bash-style functions work exactly the same:
my_function() {
    echo "Hello from $1"
}

function another_function {
    echo "Also works"
}

my_function "World"  # Works identically to bash
```

---

## 10. Pattern Matching

### Problem with Bash

```bash
# Bash: case is limited, no destructuring
case $value in
    pattern1) command1 ;;
    pattern2) command2 ;;
    *) default ;;
esac
# No way to extract parts of the match
# No guards
# Limited pattern syntax
```

### Colony Extension

```colony
# Colony: Modern pattern matching
match $response {
    {status: 200, body: b} => {
        echo "Success: $b"
    }
    {status: 404} => {
        echo "Not found"
    }
    {status: s} if $s >= 500 => {
        echo "Server error: $s"
        retry
    }
    _ => {
        echo "Unknown response"
    }
}

# Match on type
match $value {
    str s => echo "String: $s"
    int n => echo "Number: $n"
    list l => echo "List with $[len(l)] items"
    _ => echo "Unknown type"
}

# Match with arrays
match $args {
    [] => echo "No arguments"
    [single] => echo "One arg: $single"
    [first, ...rest] => {
        echo "First: $first"
        echo "Rest: $rest"
    }
}

# Match in pipelines
cat data.json | json:parse | match {
    {users: u} => process_users $u
    {error: e} => handle_error $e
}

# Guard clauses
match $user {
    {role: "admin", active: true} => grant_all_access
    {role: "admin", active: false} => echo "Inactive admin"
    {role: r} if $r in ["editor", "author"] => grant_write_access
    _ => grant_read_access
}
```

### Compatibility

```bash
# Traditional case still works exactly the same:
case $value in
    start) echo "Starting" ;;
    stop) echo "Stopping" ;;
    *) echo "Unknown" ;;
esac

# match is a NEW keyword, no conflict with bash
```

---

## Extension Detection and Graceful Degradation

### Detecting Colony Features

```colony
# Check if running in Colony
if [[ -n "$COLONY_VERSION" ]]; then
    # Use Colony features
    var data = {name: "colony"}
else
    # Fall back to bash
    declare -A data
    data[name]="colony"
fi

# Feature detection function
colony:has_feature "typed_vars" && var x = 5 || x=5
```

### Writing Polyglot Scripts

```bash
#!/usr/bin/env bash
# This script works in both bash and colony

# Colony-enhanced section (ignored by bash due to false condition)
if command -v colony &>/dev/null && [[ "${0##*/}" == "colony" ]]; then
    # Colony-specific features
    var config = {debug: true}
    context "Running in Colony mode"
else
    # Bash fallback
    declare -A config
    config[debug]=true
fi

# Common code works in both
echo "Debug: ${config[debug]:-true}"
```

### Shebang Variations

```bash
#!/usr/bin/env colony          # Full Colony features
#!/usr/bin/env colony --posix  # POSIX-only mode
#!/usr/bin/env colony-sh       # Separate POSIX-only binary
#!/bin/bash                    # Still works, runs as subshell
```

---

## Summary: What's New vs What's Compatible

### 100% Bash Compatible (Unchanged Behavior)

| Feature | Status |
|---------|--------|
| Variable assignment `var=value` | ✅ Identical |
| Command substitution `$(cmd)` | ✅ Identical |
| Arithmetic `$((expr))` | ✅ Identical |
| Parameter expansion `${var}` | ✅ Identical |
| Arrays `arr=(1 2 3)` | ✅ Identical |
| Associative arrays `declare -A` | ✅ Identical |
| Functions `fn() {}` | ✅ Identical |
| Conditionals `if/then/else/fi` | ✅ Identical |
| Loops `for/while/until` | ✅ Identical |
| Case statements | ✅ Identical |
| Pipes `\|` | ✅ Identical |
| Redirects `>` `>>` `<` `2>&1` | ✅ Identical |
| Globs `*` `?` `[...]` | ✅ Identical |
| Quoting `"..."` `'...'` | ✅ Identical |
| Here documents `<<EOF` | ✅ Identical |
| Process substitution `<()` | ✅ Identical |
| Job control `&` `fg` `bg` | ✅ Identical |
| Signals and traps | ✅ Identical |
| All builtins | ✅ Identical |

### Colony Extensions (New Syntax)

| Feature | Syntax | Conflicts? |
|---------|--------|------------|
| Typed variables | `var x = 5` | No (new keyword) |
| Constants | `const X = 5` | No (new keyword) |
| Dict literals | `{key: value}` | No (new context) |
| List literals | `[1, 2, 3]` | No (new context) |
| Expression syntax | `$[expr]` | No (new syntax) |
| Named channels | `>&ctx` | No (extends existing) |
| Agent prefix | `# natural language` | Context-dependent |
| Error-aware pipe | `?>` | No (new operator) |
| Tap pipe | `\|>` | No (new operator) |
| Try/catch | `try {} catch {}` | No (new keywords) |
| Proc definitions | `proc name() {}` | No (new keyword) |
| Pattern matching | `match {} ...` | No (new keyword) |
| Namespaced commands | `cmd:sub` | No (new syntax) |

---

## Future Extensions (Planned)

### Under Consideration

```colony
# Async/await for background jobs
var result = await { long_running_command }
var (a, b) = await all [task1, task2]

# Reactive variables (update when dependencies change)
reactive var status = $[service:health]
watch $status { echo "Status changed: $status" }

# Schema validation
schema UserConfig {
    name: str
    port: int = 8080
    debug: bool = false
}
var config: UserConfig = load_config

# Pipe to lambda
data | { |line| echo "Processing: $line" }

# String templates
var html = ```html
    <html>
        <body>Hello, ${name}!</body>
    </html>
```

# Units (for clarity)
var timeout = 30s
var size = 100MB
sleep $timeout
```

### Not Planned (Explicitly Avoided)

- **Breaking POSIX behavior**: We will never change how existing bash syntax works
- **Implicit type coercion errors**: Types are advisory/optional, not enforced by default
- **Mandatory new syntax**: You can write pure bash forever
- **Removing features**: Even deprecated bash features will work

---

## Testing Compatibility

### Compatibility Test Suite

We maintain a comprehensive test suite that runs against both bash and Colony:

```bash
# tests/bash-compat/
01_builtins.bats        # All builtins behave identically
02_arithmetic.bats      # Arithmetic expansion
03_arrays.bats          # Array behavior
04_glob.bats            # Globbing patterns
05_functions.bats       # Function definitions
06_case.bats            # Case statements
07_loops.bats           # Loop constructs
08_conditionals.bats    # Test and conditionals
09_redirects.bats       # Redirection
10_pipes.bats           # Pipelines
11_quoting.bats         # Quote handling
12_expansion.bats       # Parameter expansion
13_heredoc.bats         # Here documents
14_subshell.bats        # Subshells and process substitution
```

### Running Compatibility Tests

```bash
# Run against both bash and colony, compare outputs
./scripts/run-tests.sh --compare-with-bash

# Run only POSIX compliance tests
./scripts/run-tests.sh --posix-only

# Run extension tests (colony-only features)
./scripts/run-tests.sh --extensions
```

---

## Migration Guide

### From Bash to Colony

1. **Start with shebang change**: `#!/usr/bin/env colony`
2. **Scripts work immediately**: No changes required
3. **Adopt features incrementally**: Add `var`, structured data, etc. as desired
4. **Use compatibility mode for shared scripts**: `#!/usr/bin/env colony --posix`

### Example Migration

```bash
# Before (bash)
#!/bin/bash
declare -A config
config[host]="localhost"
config[port]=8080
result=$((${config[port]} + 1))
echo "Connecting to ${config[host]}:$result"
```

```colony
# After (colony) - gradual migration
#!/usr/bin/env colony
var config = {host: "localhost", port: 8080}
var result = $[config.port + 1]
echo "Connecting to $config.host:$result"
```

```colony
# Original bash version still works perfectly in Colony:
#!/usr/bin/env colony
declare -A config
config[host]="localhost"
config[port]=8080
result=$((${config[port]} + 1))
echo "Connecting to ${config[host]}:$result"
# ^^^ Runs identically to bash
```

---

*Colony Shell: The shell that grows with you — from bash compatibility to modern productivity.*
