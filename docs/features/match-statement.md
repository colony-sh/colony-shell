# Match Statement (F056)

## Overview

The `match` statement provides modern pattern matching for structured control flow in Colony Shell. It enables testing values against multiple patterns with optional guards and destructuring, similar to pattern matching in Rust, Swift, or modern functional languages.

## Syntax

```colony
match <expression> {
    <pattern> [if <guard>] => <body>
    <pattern> [if <guard>] => <body>
    ...
}
```

## Basic Usage

### Literal Matching

```colony
match 42 {
    42 => { echo "The answer"; }
    _ => { echo "Something else"; }
}
```

### String Matching

```colony
match $status {
    "success" => { echo "All good"; }
    "failure" => { echo "Something failed"; }
    _ => { echo "Unknown status"; }
}
```

## Pattern Types

### Wildcard Pattern

The `_` pattern matches any value:

```colony
match $value {
    _ => { echo "Always matches"; }
}
```

### Binding Pattern

Capture the matched value in a variable:

```colony
match $input {
    n => { echo "Got value: $n"; }
}
```

### Or-Patterns

Match multiple alternatives:

```colony
match $code {
    1 | 2 | 3 => { echo "Small number"; }
    _ => { echo "Larger number"; }
}
```

### List Destructuring

Match and extract list elements:

```colony
match [1, 2, 3] {
    [a, b, c] => { echo "Three items: $a, $b, $c"; }
    [first, ..rest] => { echo "First: $first"; }
    _ => { echo "No match"; }
}
```

### Dictionary Destructuring

Match and extract dictionary fields:

```colony
match {name: "alice", age: 30} {
    {name, age} => { echo "$name is $age years old"; }
    {name} => { echo "Only name: $name"; }
    _ => { echo "No match"; }
}
```

## Guard Clauses

Add conditional logic to patterns with `if` guards:

```colony
match $value {
    n if n > 0 => { echo "Positive"; }
    n if n < 0 => { echo "Negative"; }
    _ => { echo "Zero"; }
}
```

Guards evaluate after pattern matching succeeds. If the guard fails, the next arm is tried.

## Body Types

### Block Bodies

Execute multiple statements:

```colony
match $action {
    "build" => {
        echo "Building project...";
        cargo build --release;
        echo "Build complete";
    }
    "test" => {
        echo "Running tests...";
        cargo test;
    }
    _ => { echo "Unknown action"; }
}
```

### Expression Bodies

Return a truthy/falsy value:

```colony
match $value {
    1 => 1    # Truthy (exit 0)
    _ => 0    # Falsy (exit 1)
}
```

Expression bodies set the exit status based on whether the result is truthy.

## Match Semantics

- **First match wins**: Only the first matching arm executes
- **No fallthrough**: Unlike case statements, arms don't fall through
- **Optional default**: A non-matching value simply exits with status 0
- **Variable scope**: Bound variables are local to the arm's body

## Implementation Details

### Files Involved

- `crates/colony-parser/colony.pest` - Grammar rules for match syntax
- `crates/colony-parser/src/ast.rs` - AST nodes (MatchStatement, MatchArm, MatchPattern)
- `crates/colony-parser/src/builder.rs` - AST construction from parse tree
- `crates/colony-shell/src/shell/execution/control_flow.rs` - Match execution logic
- `tests/scripts/bash-compat/51_match.bats` - 24 comprehensive tests

### Data Flow

1. Parser recognizes `match` keyword and expression
2. Each arm is parsed with pattern, optional guard, and body
3. AST builder creates MatchStatement with nested MatchArm nodes
4. Executor evaluates the match expression
5. For each arm, pattern matching is attempted
6. If pattern matches and guard passes, body executes
7. Execution stops after first matching arm

### Pattern Matching Algorithm

```rust
for arm in match_statement.arms {
    if pattern_matches(arm.pattern, value) {
        if let Some(guard) = arm.guard {
            if !evaluate_guard(guard) {
                continue;  // Try next arm
            }
        }
        return execute_body(arm.body);
    }
}
// No match, exit success
```

## Examples

### HTTP Status Code Handler

```colony
match $http_status {
    200 | 201 | 204 => { echo "Success"; }
    400 | 401 | 403 | 404 => { echo "Client error"; }
    500 | 502 | 503 => { echo "Server error"; }
    code if code >= 300 && code < 400 => { echo "Redirect"; }
    _ => { echo "Unknown status"; }
}
```

### Command Line Argument Parsing

```colony
match $1 {
    "--help" | "-h" => { show_help; }
    "--version" | "-v" => { show_version; }
    "--verbose" => { set_verbose=true; }
    arg if [[ $arg == --* ]] => { echo "Unknown option: $arg"; exit 1; }
    _ => { echo "Processing: $1"; }
}
```

## Comparison with Case

| Feature | `match` | `case` |
|---------|---------|--------|
| Pattern types | Literals, bindings, destructuring | Glob patterns |
| Guards | Yes (`if` clause) | No |
| Destructuring | Yes (lists, dicts) | No |
| Fallthrough | No | Optional (`;;&`) |
| Expression bodies | Yes | No |
| POSIX compatible | No | Yes |

Use `match` for structured data and modern control flow. Use `case` for string pattern matching and POSIX compatibility.

## Testing

The match statement has 24 comprehensive BATS tests covering:

- Basic literal and string matching
- Wildcard patterns
- Or-patterns
- Binding patterns with variable capture
- Guard clauses with truthy/falsy evaluation
- List destructuring with exact length and rest patterns
- Dictionary destructuring
- Multi-statement block bodies
- Expression bodies with exit status

Run tests with:

```bash
bats tests/scripts/bash-compat/51_match.bats
```

## Future Enhancements

Potential future additions:

- Range patterns: `1..10 => { ... }`
- Type patterns: `Int(n) => { ... }`
- Regex patterns: `/pattern/ => { ... }`
- Exhaustiveness checking
- Pattern compilation optimization
