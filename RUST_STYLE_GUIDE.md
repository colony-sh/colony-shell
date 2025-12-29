# Colony Shell — Rust Coding Guide

This guide establishes idiomatic Rust patterns and conventions for the Colony Shell project. Follow these guidelines to maintain consistency and readability across the codebase.

---

## Table of Contents

1. [Option and Result Handling](#option-and-result-handling)
2. [Error Handling](#error-handling)
3. [Naming Conventions](#naming-conventions)
4. [Code Organization](#code-organization)
5. [Ownership and Borrowing](#ownership-and-borrowing)
6. [Iterators and Functional Patterns](#iterators-and-functional-patterns)
7. [Structs and Enums](#structs-and-enums)
8. [Pattern Matching](#pattern-matching)
9. [Async Code](#async-code)
10. [Unsafe Code](#unsafe-code)
11. [Testing](#testing)
12. [Documentation](#documentation)
13. [Performance Considerations](#performance-considerations)

---

## Option and Result Handling

### Avoid the Pyramid of Doom

Nested `if let` statements reduce readability. Prefer combinator chains or `let-else`.

```rust
// ❌ Avoid: Nested if-let pyramid
if let Some(a) = foo() {
    if let Some(b) = bar(a) {
        if let Some(c) = baz(b) {
            do_something(c);
        }
    }
}

// ✅ Prefer: Combinator chain
let result = foo()
    .and_then(|a| bar(a))
    .and_then(|b| baz(b));

if let Some(c) = result {
    do_something(c);
}

// ✅ Prefer: let-else for early returns
fn process() -> Option<Output> {
    let Some(a) = foo() else { return None };
    let Some(b) = bar(a) else { return None };
    let Some(c) = baz(b) else { return None };
    Some(do_something(c))
}

// ✅ Prefer: if-let chains (Rust 1.76+)
if let Some(a) = foo()
    && let Some(b) = bar(a)
    && let Some(c) = baz(b)
{
    do_something(c);
}
```

### Nested `if let` is Always Wrong

If you write `if let` inside another `if let`, stop and refactor.
```rust
// ❌ NEVER do this
if let Some(x) = foo() {
    if let Ok(y) = bar(x) {
        do_thing(y);
    }
}

// ✅ Use let-else
let Some(x) = foo() else { return };
let Ok(y) = bar(x) else { return };
do_thing(y);
```

This applies even when matching enum variants:
```rust
// ❌ Nested matching
if let Some(Action::Command(cmd)) = get_action() {
    if let Ok(parsed) = parse(&cmd) {
        execute(parsed);
    }
}

// ✅ Sequential let-else
let Some(Action::Command(cmd)) = get_action() else { return };
let Ok(parsed) = parse(&cmd) else { return };
execute(parsed);
```

**No exceptions.** If you think you need nested `if let`, you don't.

### Use the Right Combinator

| Goal | Combinator |
|------|------------|
| Transform inner value | `.map()` |
| Transform with fallible fn | `.and_then()` |
| Provide default value | `.unwrap_or()`, `.unwrap_or_default()` |
| Lazily compute default | `.unwrap_or_else()` |
| Convert Option to Result | `.ok_or()`, `.ok_or_else()` |
| Check condition on value | `.is_some_and()`, `.is_ok_and()` |
| Filter by predicate | `.filter()` |
| Take ownership, leave None | `.take()` |

### Avoid Unnecessary `unwrap()` and `expect()`

```rust
// ❌ Avoid in production paths
let value = some_option.unwrap();

// ✅ Use expect() with context when panic is intentional
let value = some_option.expect("config must be initialized before use");

// ✅ Prefer propagation or handling
let value = some_option.ok_or(ShellError::MissingValue)?;
```

Reserve `unwrap()` for:
- Tests
- Cases where `None`/`Err` is provably impossible
- Prototyping (mark with `// TODO: handle error`)

---

## Error Handling

### Use `thiserror` for Library Errors

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ShellError {
    #[error("command not found: {0}")]
    CommandNotFound(String),

    #[error("invalid syntax at line {line}: {message}")]
    SyntaxError { line: usize, message: String },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("environment variable error: {0}")]
    Env(#[from] std::env::VarError),
}
```

### Use `anyhow` for Application-Level Errors

```rust
use anyhow::{Context, Result};

fn load_config() -> Result<Config> {
    let contents = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read config from {}", path.display()))?;
    
    toml::from_str(&contents)
        .context("failed to parse config file")
}
```

### Error Propagation Patterns

```rust
// ✅ Use ? for propagation
fn read_file(path: &Path) -> Result<String, io::Error> {
    let mut file = File::open(path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    Ok(contents)
}

// ✅ Add context when crossing module boundaries
pub fn execute_script(path: &Path) -> Result<ExitStatus> {
    let source = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read script: {}", path.display()))?;
    
    parse_and_execute(&source)
        .context("script execution failed")
}
```

---

## Naming Conventions

### General Rules

| Item | Convention | Example |
|------|------------|---------|
| Crates | `snake_case` | `colony_shell` |
| Modules | `snake_case` | `command_parser` |
| Types (struct, enum, trait) | `PascalCase` | `CommandExecutor` |
| Functions, methods | `snake_case` | `execute_command` |
| Local variables | `snake_case` | `exit_code` |
| Constants | `SCREAMING_SNAKE_CASE` | `MAX_HISTORY_SIZE` |
| Type parameters | Single uppercase or short `PascalCase` | `T`, `Item`, `Ctx` |
| Lifetimes | Short lowercase | `'a`, `'src`, `'ctx` |

### Naming Patterns

```rust
// Constructors
impl Shell {
    pub fn new() -> Self { ... }              // Default constructor
    pub fn with_config(cfg: Config) -> Self   // Parameterized constructor
    pub fn from_env() -> Result<Self>         // Fallible from external source
}

// Conversions (see std::convert traits)
impl Command {
    pub fn as_str(&self) -> &str { ... }      // Cheap, borrowed view
    pub fn to_string(&self) -> String { ... } // Expensive, creates owned
    pub fn into_inner(self) -> Inner { ... }  // Consumes self
}

// Getters (no `get_` prefix)
impl Job {
    pub fn pid(&self) -> Pid { ... }          // ✅
    pub fn get_pid(&self) -> Pid { ... }      // ❌ Avoid
}

// Predicates
impl Process {
    pub fn is_running(&self) -> bool { ... }
    pub fn has_children(&self) -> bool { ... }
}

// Fallible operations
impl Shell {
    pub fn try_parse(input: &str) -> Result<Ast> { ... }
}
```

---

## Code Organization

### Module Structure

```
src/
├── lib.rs              # Public API, re-exports
├── main.rs             # Entry point, minimal logic
├── shell/
│   ├── mod.rs          # Module root, public interface
│   ├── executor.rs     # Command execution
│   ├── parser.rs       # Input parsing
│   └── builtin/
│       ├── mod.rs
│       ├── cd.rs
│       └── export.rs
├── config.rs           # Configuration handling
└── error.rs            # Error types
```

### Module Visibility

```rust
// In mod.rs, be explicit about public interface
mod executor;
mod parser;
mod state;

pub use executor::Executor;
pub use parser::{Parser, Ast};
// state remains private
```

### Import Organization

```rust
// 1. Standard library
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// 2. External crates
use anyhow::{Context, Result};
use tokio::sync::mpsc;

// 3. Crate-level imports
use crate::config::Config;
use crate::error::ShellError;

// 4. Module-level imports
use super::parser::Token;
```

---

## Ownership and Borrowing

### Prefer Borrowing Over Cloning

```rust
// ❌ Unnecessary clone
fn process(data: Vec<String>) {
    for item in data.clone() {
        println!("{}", item);
    }
    // data is still usable but we didn't need to clone
}

// ✅ Borrow when possible
fn process(data: &[String]) {
    for item in data {
        println!("{}", item);
    }
}
```

### Use `Cow` for Conditional Ownership

```rust
use std::borrow::Cow;

fn normalize_path(path: &str) -> Cow<'_, str> {
    if path.starts_with("~/") {
        Cow::Owned(path.replacen("~", &home_dir(), 1))
    } else {
        Cow::Borrowed(path)
    }
}
```

### Lifetime Elision

Let the compiler infer lifetimes when possible:

```rust
// ✅ Elided (preferred when unambiguous)
fn first_word(s: &str) -> &str { ... }

// Explicit equivalent
fn first_word<'a>(s: &'a str) -> &'a str { ... }
```

Add explicit lifetimes when:
- Multiple references with different lifetimes
- Clarity is improved
- The compiler requires them

---

## Iterators and Functional Patterns

### Prefer Iterator Chains Over Loops

```rust
// ❌ Imperative style
let mut results = Vec::new();
for item in items {
    if item.is_valid() {
        results.push(item.process());
    }
}

// ✅ Functional style
let results: Vec<_> = items
    .iter()
    .filter(|item| item.is_valid())
    .map(|item| item.process())
    .collect();
```

### Use `collect()` Wisely

```rust
// Collect into specific types
let vec: Vec<_> = iter.collect();
let set: HashSet<_> = iter.collect();
let map: HashMap<_, _> = pairs.collect();
let result: Result<Vec<_>, _> = fallible_iter.collect();
let string: String = chars.collect();
```

### Useful Iterator Adaptors

```rust
// Enumerate for indices
for (i, item) in items.iter().enumerate() { ... }

// Zip for parallel iteration
for (a, b) in left.iter().zip(right.iter()) { ... }

// Chain for concatenation
let all = first.iter().chain(second.iter());

// Flatten nested iterators
let flat: Vec<_> = nested.into_iter().flatten().collect();

// Partition by predicate
let (valid, invalid): (Vec<_>, Vec<_>) = items.into_iter()
    .partition(|item| item.is_valid());
```

---

## Structs and Enums

### Use Builder Pattern for Complex Structs

```rust
#[derive(Default)]
pub struct CommandBuilder {
    program: Option<String>,
    args: Vec<String>,
    env: HashMap<String, String>,
    cwd: Option<PathBuf>,
}

impl CommandBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn program(mut self, program: impl Into<String>) -> Self {
        self.program = Some(program.into());
        self
    }

    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn build(self) -> Result<Command, BuildError> {
        let program = self.program.ok_or(BuildError::MissingProgram)?;
        Ok(Command { program, args: self.args, ... })
    }
}

// Usage
let cmd = CommandBuilder::new()
    .program("ls")
    .arg("-la")
    .build()?;
```

### Use `#[non_exhaustive]` for Extensibility

```rust
#[non_exhaustive]
pub enum Signal {
    Interrupt,
    Terminate,
    Kill,
}

#[non_exhaustive]
pub struct JobStatus {
    pub pid: u32,
    pub running: bool,
}
```

### Derive Common Traits

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CommandId(u64);

#[derive(Debug, Clone, Default)]
pub struct Options {
    pub verbose: bool,
    pub color: ColorChoice,
}
```

Derive order: `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `PartialOrd`, `Ord`, `Hash`, `Default`

---

## Pattern Matching

### Prefer `match` Over `if-else` Chains

```rust
// ❌ Multiple if-else
if status == Status::Running {
    handle_running()
} else if status == Status::Stopped {
    handle_stopped()
} else {
    handle_other()
}

// ✅ Match expression
match status {
    Status::Running => handle_running(),
    Status::Stopped => handle_stopped(),
    _ => handle_other(),
}
```

### Use Match Guards Sparingly

```rust
match value {
    Some(x) if x > 0 => println!("positive: {}", x),
    Some(x) if x < 0 => println!("negative: {}", x),
    Some(_) => println!("zero"),
    None => println!("nothing"),
}
```

### Destructure in Match Arms

```rust
match command {
    Command::Cd { path } => change_directory(path),
    Command::Export { name, value } => set_env(name, value),
    Command::External { program, args } => execute(program, args),
}
```

### Use `matches!` for Boolean Checks

```rust
// ❌ Verbose
let is_whitespace = match c {
    ' ' | '\t' | '\n' => true,
    _ => false,
};

// ✅ Concise
let is_whitespace = matches!(c, ' ' | '\t' | '\n');
```

---

## Async Code

### Use `tokio` as the Async Runtime

```rust
#[tokio::main]
async fn main() -> Result<()> {
    let shell = Shell::new().await?;
    shell.run().await
}
```

### Prefer `async fn` Over Returning `impl Future`

```rust
// ✅ Preferred
async fn fetch_data(url: &str) -> Result<Data> {
    // ...
}

// Use impl Future when you need to name the type or for perf
fn fetch_data(url: &str) -> impl Future<Output = Result<Data>> + '_ {
    async move { ... }
}
```

### Avoid Blocking in Async Contexts

```rust
// ❌ Blocks the runtime
async fn bad_read(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(Into::into)
}

// ✅ Use async I/O
async fn good_read(path: &Path) -> Result<String> {
    tokio::fs::read_to_string(path).await.map_err(Into::into)
}

// ✅ Or spawn blocking for CPU-intensive work
async fn compute_hash(data: Vec<u8>) -> Hash {
    tokio::task::spawn_blocking(move || expensive_hash(&data))
        .await
        .expect("task panicked")
}
```

### Structured Concurrency

```rust
use tokio::task::JoinSet;

async fn run_parallel_tasks(tasks: Vec<Task>) -> Vec<Result<Output>> {
    let mut set = JoinSet::new();
    
    for task in tasks {
        set.spawn(async move { task.execute().await });
    }
    
    let mut results = Vec::new();
    while let Some(result) = set.join_next().await {
        results.push(result.expect("task panicked"));
    }
    results
}
```

---

## Unsafe Code

### Minimize Unsafe Usage

Colony Shell interacts with libc for terminal control, signals, and process management. Encapsulate unsafe code in safe abstractions.

```rust
// ✅ Safe wrapper around unsafe operation
pub fn is_interactive() -> bool {
    // SAFETY: isatty is safe to call with any file descriptor;
    // it returns 0 for invalid fds.
    unsafe { libc::isatty(libc::STDIN_FILENO) != 0 }
}

// ✅ Encapsulated unsafe with invariants documented
pub struct RawTerminal {
    original: libc::termios,
}

impl RawTerminal {
    pub fn enable() -> io::Result<Self> {
        let mut original: libc::termios = unsafe { std::mem::zeroed() };
        // SAFETY: We pass a valid pointer to a termios struct
        if unsafe { libc::tcgetattr(libc::STDIN_FILENO, &mut original) } != 0 {
            return Err(io::Error::last_os_error());
        }
        // ... configure raw mode ...
        Ok(Self { original })
    }
}
```

### Document Safety Invariants

Every `unsafe` block must have a `// SAFETY:` comment explaining why it's safe:

```rust
// SAFETY: We've verified that:
// 1. The pointer is non-null (checked above)
// 2. The memory is properly aligned for T
// 3. The lifetime is valid for the duration of this scope
unsafe {
    ptr.as_ref()
}
```

---

## Testing

### Unit Tests in Same File

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_command() {
        let result = parse("ls -la");
        assert_eq!(result.program, "ls");
        assert_eq!(result.args, vec!["-la"]);
    }

    #[test]
    fn test_parse_empty_returns_error() {
        assert!(parse("").is_err());
    }
}
```

### Use `#[should_panic]` for Panic Tests

```rust
#[test]
#[should_panic(expected = "index out of bounds")]
fn test_invalid_index_panics() {
    let v = vec![1, 2, 3];
    let _ = v[10];
}
```

### Property-Based Testing with `proptest`

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn parse_roundtrip(input in "[a-z]+( [a-z]+)*") {
        let parsed = parse(&input)?;
        let serialized = parsed.to_string();
        prop_assert_eq!(input, serialized);
    }
}
```

### Integration Tests in `tests/`

```rust
// tests/integration.rs
use colony_shell::Shell;

#[tokio::test]
async fn test_shell_executes_simple_command() {
    let mut shell = Shell::new_test().await;
    let output = shell.execute("echo hello").await.unwrap();
    assert_eq!(output.stdout.trim(), "hello");
}
```

---

## Documentation

### Document Public Items

```rust
/// A shell command ready for execution.
///
/// Commands can be either built-in shell commands (like `cd` or `export`)
/// or external programs that will be spawned as child processes.
///
/// # Examples
///
/// ```
/// use colony_shell::Command;
///
/// let cmd = Command::parse("ls -la")?;
/// let status = cmd.execute()?;
/// ```
pub struct Command {
    // ...
}
```

### Use `#[doc(hidden)]` for Internal Public Items

```rust
/// Internal parser state. Not part of the public API.
#[doc(hidden)]
pub struct ParserState { ... }
```

### Module-Level Documentation

```rust
//! # Shell Command Parser
//!
//! This module provides parsing functionality for shell commands,
//! including support for pipes, redirects, and subshells.
//!
//! ## Architecture
//!
//! The parser is a recursive descent parser that produces an AST...
```

---

## Performance Considerations

### Avoid Premature Optimization

Write clear, idiomatic code first. Profile before optimizing.

### Use `&str` Over `String` in Parameters

```rust
// ❌ Forces allocation
fn process(s: String) { ... }

// ✅ Accepts both String and &str
fn process(s: &str) { ... }

// ✅ Even more flexible with impl AsRef
fn process(s: impl AsRef<str>) { ... }
```

### Pre-allocate Collections When Size is Known

```rust
// ❌ May reallocate multiple times
let mut vec = Vec::new();
for i in 0..1000 {
    vec.push(i);
}

// ✅ Single allocation
let mut vec = Vec::with_capacity(1000);
for i in 0..1000 {
    vec.push(i);
}

// ✅ Or use collect
let vec: Vec<_> = (0..1000).collect();
```

### Use `SmallVec` for Small Collections

```rust
use smallvec::SmallVec;

// Stack-allocated for ≤4 elements, heap for more
type Args = SmallVec<[String; 4]>;
```

### Benchmark Critical Paths

```rust
// benches/parser.rs
use criterion::{criterion_group, criterion_main, Criterion};

fn bench_parse(c: &mut Criterion) {
    c.bench_function("parse simple", |b| {
        b.iter(|| parse("ls -la /home"))
    });
}

criterion_group!(benches, bench_parse);
criterion_main!(benches);
```

---

## Project-Specific Conventions

### Signal Handling

Always use the signal abstraction layer; never call libc signal functions directly:

```rust
// ✅ Use our signal module
use crate::signal::{Signal, SignalHandler};

let handler = SignalHandler::new();
handler.on(Signal::Interrupt, || { ... });
```

### Job Control

Jobs must always be tracked through the `JobTable`:

```rust
// ✅ Register jobs properly
let job = Job::spawn(command)?;
self.job_table.add(job);
```

### Terminal State

Always use RAII guards for terminal mode changes:

```rust
// ✅ Automatic cleanup on drop
let _raw = RawMode::enable()?;
// terminal is raw here
// restored automatically when _raw drops
```

---

## Clippy Lints

The project enforces these Clippy lints. Run `cargo clippy` before committing:

```toml
# Cargo.toml
[lints.clippy]
all = "warn"
pedantic = "warn"
nursery = "warn"

# Allowed exceptions
module_name_repetitions = "allow"
must_use_candidate = "allow"
```

---

## Summary Checklist

Before submitting code, verify:

- [ ] No nested `if let` pyramids
- [ ] Errors have context with `.context()` or `.with_context()`
- [ ] No `unwrap()` outside tests without justification
- [ ] Public items are documented
- [ ] Unsafe blocks have `// SAFETY:` comments
- [ ] Tests cover happy path and error cases
- [ ] `cargo fmt` has been run
- [ ] `cargo clippy` passes
- [ ] No unnecessary allocations (prefer `&str` over `String`, etc.)