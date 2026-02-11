# Proc Definitions (F057)

## Overview

The `proc` keyword introduces typed procedure definitions with named parameters and default values. Unlike traditional shell functions that use positional parameters (`$1`, `$2`), procs provide named arguments with type annotations and optional defaults, making code more readable and maintainable.

## Syntax

```colony
proc <name>([<param>[: <type>] [= <default>], ...]) [: <return_type>] {
    <body>
}
```

## Basic Usage

### Simple Proc

```colony
proc greet(name) {
    echo "Hello $name";
}

greet World
# Output: Hello World
```

### Multiple Parameters

```colony
proc add(a, b) {
    echo "$a + $b = $((a + b))";
}

add 5 3
# Output: 5 + 3 = 8
```

### No Parameters

```colony
proc hello() {
    echo "Hello from proc";
}

hello
# Output: Hello from proc
```

## Default Parameters

Provide default values for optional parameters:

```colony
proc greet(name, greeting = "Hello") {
    echo "$greeting $name";
}

greet Alice
# Output: Hello Alice

greet Bob "Hi"
# Output: Hi Bob
```

Multiple defaults:

```colony
proc info(name = "anonymous", age = 0) {
    echo "$name is $age years old";
}

info
# Output: anonymous is 0 years old

info Alice 30
# Output: Alice is 30 years old
```

## Parameter Handling

### Named Parameters

Parameters are bound by position, not name:

```colony
proc show(x, y) {
    echo "x=$x, y=$y";
}

show first second
# Output: x=first, y=second
```

### Missing Parameters

Missing parameters default to empty strings:

```colony
proc display(value) {
    echo "value=[$value]";
}

display
# Output: value=[]
```

### Extra Arguments

Extra arguments beyond declared parameters are ignored:

```colony
proc one(x) {
    echo "x=$x";
}

one a b c
# Output: x=a
```

## Return Statements

Use `return` to exit early with an exit code:

```colony
proc validate(input) {
    if [ -z "$input" ]; then
        echo "Error: empty input";
        return 1;
    fi
    echo "Valid input: $input";
    return 0;
}

validate ""
# Output: Error: empty input
# Exit code: 1

validate "data"
# Output: Valid input: data
# Exit code: 0
```

Return stops execution:

```colony
proc early_exit() {
    echo "before";
    return 0;
    echo "after";  # Never executed
}

early_exit
# Output: before
```

## Scoping

### Local Parameters

Proc parameters are local to the proc scope and don't affect outer variables:

```colony
x="outer"

proc set_x(x) {
    echo "inside: $x";
}

set_x "inner"
# Output: inside: inner

echo "outside: $x"
# Output: outside: outer
```

### Positional Parameters Preserved

Procs don't affect the shell's positional parameters (`$1`, `$2`, etc.):

```colony
set -- original

proc test(a) {
    echo "param: $a";
}

test hello
# Output: param: hello

echo "$1"
# Output: original
```

## Advanced Usage

### Multi-Statement Bodies

```colony
proc process(file, verbose = false) {
    echo "Processing $file...";

    if [ "$verbose" = "true" ]; then
        echo "Verbose mode enabled";
    fi

    cat "$file" | wc -l;
    echo "Done";
}
```

### Conditionals in Body

```colony
proc check(value) {
    if [ "$value" = "yes" ]; then
        echo "Confirmed";
        return 0;
    else
        echo "Denied";
        return 1;
    fi
}
```

### Loops in Body

```colony
proc repeat(msg, count = 3) {
    for i in $(seq 1 $count); do
        echo "$i: $msg";
    done
}

repeat "Hello" 2
# Output:
# 1: Hello
# 2: Hello
```

### Procs in Pipelines

```colony
proc numbers() {
    echo "1";
    echo "2";
    echo "3";
}

numbers | wc -l
# Output: 3
```

### Nested Proc Calls

```colony
proc greet(name) {
    echo "Hi $name";
}

proc formal(name) {
    greet "Mr. $name";
}

formal Smith
# Output: Hi Mr. Smith
```

## Proc Redefinition

Redefining a proc overwrites the previous definition:

```colony
proc say() {
    echo "first";
}

proc say() {
    echo "second";
}

say
# Output: second
```

## Implementation Details

### Files Involved

- `crates/colony-parser/colony.pest` - Grammar rules for proc syntax
- `crates/colony-parser/src/ast.rs` - ProcDef, ProcParam AST nodes
- `crates/colony-parser/src/builder.rs` - AST construction from parse tree
- `crates/colony-expand/src/context.rs` - Proc registration and storage
- `crates/colony-exec/src/lib.rs` - Proc execution with parameter binding
- `tests/scripts/bash-compat/52_proc.bats` - 22 comprehensive tests

### Data Flow

1. Parser recognizes `proc` keyword and parameter list
2. AST builder creates ProcDef with name, params, defaults, and body
3. Executor registers proc definition in expansion context
4. On invocation, args are matched to params by position
5. Parameters are bound in a local scope
6. Body statements execute with parameter bindings
7. Return statement or end of body exits proc

### Parameter Binding Algorithm

```rust
for (param, value) in proc_params.zip(call_args) {
    if let Some(val) = value {
        scope.set(param.name, val);
    } else if let Some(default) = param.default {
        scope.set(param.name, evaluate(default));
    } else {
        scope.set(param.name, "");
    }
}
```

### Scope Management

Procs create an isolated scope:

1. Save current positional parameters
2. Create new variable scope with parameter bindings
3. Execute body in new scope
4. Restore original positional parameters
5. Propagate exit status

## Comparison with Functions

| Feature | `proc` | `function` |
|---------|--------|------------|
| Parameters | Named with defaults | Positional (`$1`, `$2`) |
| Type annotations | Supported (future) | No |
| Defaults | Yes | No (must check manually) |
| Scope isolation | Yes | Manual (requires `local`) |
| Return value | Exit code | Exit code |
| POSIX compatible | No | Yes |

Use `proc` for modern, readable code with clear interfaces. Use `function` for POSIX compatibility and positional parameter flexibility.

## Examples

### Configuration Helper

```colony
proc load_config(file = ".config", strict = false) {
    if [ ! -f "$file" ]; then
        if [ "$strict" = "true" ]; then
            echo "Error: Config file not found: $file";
            return 1;
        fi
        echo "Warning: Using defaults (no $file)";
        return 0;
    fi

    source "$file";
    echo "Loaded config from $file";
    return 0;
}

load_config               # Uses .config, non-strict
load_config "custom.conf" # Uses custom.conf, non-strict
load_config ".config" true # Strict mode
```

### Logging with Levels

```colony
proc log(level, message, timestamp = true) {
    if [ "$timestamp" = "true" ]; then
        echo "[$(date +%T)] [$level] $message";
    else
        echo "[$level] $message";
    fi
}

log "INFO" "Starting application"
# Output: [14:30:15] [INFO] Starting application

log "ERROR" "Failed to connect" false
# Output: [ERROR] Failed to connect
```

### Retry Logic

```colony
proc retry(command, max_attempts = 3, delay = 1) {
    local attempt=1;

    while [ $attempt -le $max_attempts ]; do
        echo "Attempt $attempt/$max_attempts";

        if eval "$command"; then
            echo "Success";
            return 0;
        fi

        if [ $attempt -lt $max_attempts ]; then
            echo "Retrying in ${delay}s...";
            sleep $delay;
        fi

        attempt=$((attempt + 1));
    done

    echo "Failed after $max_attempts attempts";
    return 1;
}

retry "curl https://api.example.com" 5 2
```

## Testing

The proc system has 22 comprehensive BATS tests covering:

- Basic proc definition and invocation
- Multiple parameters
- Default parameter values
- Missing parameters (empty string)
- Extra arguments (ignored)
- Multi-statement bodies
- Conditionals and loops in bodies
- Return statements with exit codes
- Parameter scope isolation
- Positional parameter preservation
- Proc redefinition
- Procs in pipelines
- Nested proc calls

Run tests with:

```bash
bats tests/scripts/bash-compat/52_proc.bats
```

## Future Enhancements

Potential future additions:

- Type checking: `proc add(a: int, b: int): int`
- Return type validation
- Keyword arguments: `proc greet(name, greeting: "Hi")`
- Variadic parameters: `proc sum(...numbers)`
- Anonymous procs/lambdas
- Proc composition and currying
