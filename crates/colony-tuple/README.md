# colony-tuple

Linda-style tuple space for agent coordination in Colony Shell.

## Overview

This crate provides two tuple space implementations:

1. **`TupleSpace`** - In-memory, ephemeral (single process, lives in RAM)
2. **`PersistentTupleSpace`** - SQLite-backed, persistent (multi-process, survives restarts)

## When to Use Each

### Use `TupleSpace` (in-memory) for:
- Single-shell coordination (agents within one shell process)
- Temporary coordination (don't need persistence)
- Maximum performance (sub-microsecond operations)
- Testing and prototyping

### Use `PersistentTupleSpace` (SQLite) for:
- Multi-shell coordination (coordination across shell instances)
- Distributed locks that survive crashes
- Task queues that persist across restarts
- Inter-session messaging
- Long-running workflows

## Linda Primitives

Both implementations support the classic Linda operations:

| Operation | Blocking | Destructive | Description |
|-----------|----------|-------------|-------------|
| `out(tuple)` | No | N/A | Write tuple to space |
| `in(pattern)` | Yes | Yes | Wait for and remove matching tuple |
| `rd(pattern)` | Yes | No | Wait for and read matching tuple |
| `inp(pattern)` | No | Yes | Try to remove matching tuple |
| `rdp(pattern)` | No | No | Try to read matching tuple |

## Quick Start

### In-Memory Example

```rust
use colony_tuple::{TupleSpace, Tuple, Value};

let space = TupleSpace::new();

// Write a tuple
space.out(Tuple::from(vec![
    Value::Str("task".to_string()),
    Value::Str("build".to_string()),
]));

// Read it back with wildcard pattern
let pattern = Tuple::pattern(&[
    Value::Str("task".to_string()),
    Value::Wildcard,  // Matches any value
]);

if let Some(task) = space.inp(pattern) {
    println!("Got task: {:?}", task.fields[1]);
}
```

### Persistent Example

```rust
use colony_tuple::{PersistentTupleSpace, Tuple, Value};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Opens ~/.config/colony/tuple-space.db
    let space = PersistentTupleSpace::open_default().await?;

    // Write persists to disk
    space.out(Tuple::from(vec![
        Value::Str("lock".to_string()),
        Value::Str("deploy".to_string()),
        Value::Str("session-123".to_string()),
    ])).await?;

    // Even if this shell exits, another shell can see the lock
    Ok(())
}
```

## Storage Architecture

### In-Memory (`TupleSpace`)

```
┌─────────────────────────────────────────┐
│         scc::HashMap<u64, Vec<Tuple>>   │  <- Bucketed by length
│                                         │
│  Bucket 2: [("lock", "deploy")]        │
│  Bucket 3: [("task", "build", "...")]  │
│  Bucket 4: [("event", "...", ...)]     │
└─────────────────────────────────────────┘
         ↓
   parking_lot::Condvar (for blocking ops)
```

### Persistent (`PersistentTupleSpace`)

```
┌──────────────────────┐     ┌──────────────────────┐
│   Memory Cache       │ ←→  │   SQLite Database    │
│   (TupleSpace)       │     │   ~/.config/colony/  │
│                      │     │   tuple-space.db     │
│  Hot path: < 1μs     │     │  Persistent: durable │
└──────────────────────┘     └──────────────────────┘
         ↓                            ↓
    Fast reads                  Survives restarts
    (sub-microsecond)           (multi-process)
```

**Write-through**: `out()` writes to disk first, then memory
**Read-through**: `inp/rdp()` try memory first, fallback to disk

## Use Cases

### 1. Distributed Locks

```rust
// Shell A: Acquire lock
space.out(Tuple::from(vec![
    Value::Str("lock".to_string()),
    Value::Str("deploy".to_string()),
    Value::Str(session_id()),
])).await?;

// Do critical work...

// Release lock
let pattern = Tuple::pattern(&[
    Value::Str("lock".to_string()),
    Value::Str("deploy".to_string()),
    Value::Wildcard,
]);
space.inp(pattern).await?;
```

```rust
// Shell B: Check lock
let pattern = Tuple::pattern(&[
    Value::Str("lock".to_string()),
    Value::Str("deploy".to_string()),
    Value::Wildcard,
]);

if let Some(lock) = space.rdp(pattern).await? {
    println!("Lock held by: {:?}", lock.fields[2]);
} else {
    println!("Lock is available");
}
```

### 2. Task Queue (Producer/Consumer)

```rust
// Producer: Add task
space.out(Tuple::from(vec![
    Value::Str("task".to_string()),
    Value::Str("build".to_string()),
    Value::Dict(/* work details */),
])).await?;

// Worker: Take and process task
let pattern = Tuple::pattern(&[
    Value::Str("task".to_string()),
    Value::Wildcard,
    Value::Wildcard,
]);

while let Some(task) = space.inp(pattern.clone()).await? {
    process_task(&task);
}
```

### 3. Inter-Session Messaging

```rust
// Session A: Send message
space.out(Tuple::from(vec![
    Value::Str("message".to_string()),
    Value::Str("session-b".to_string()),  // Recipient
    Value::Str("Hello from A!".to_string()),
])).await?;

// Session B: Receive message (blocking)
let pattern = Tuple::pattern(&[
    Value::Str("message".to_string()),
    Value::Str("session-b".to_string()),  // My ID
    Value::Wildcard,                       // Any message
]);

if let Some(msg) = space.r#in(pattern).await? {
    println!("Received: {:?}", msg.fields[2]);
}
```

### 4. Reactive Environment

```rust
// When export is called, emit change notification
space.out(Tuple::from(vec![
    Value::Str("env_change".to_string()),
    Value::Str("PATH".to_string()),
    Value::Str("/old/path".to_string()),
    Value::Str("/new/path".to_string()),
])).await?;

// Agent watches for changes
let pattern = Tuple::pattern(&[
    Value::Str("env_change".to_string()),
    Value::Wildcard,  // Any variable
    Value::Wildcard,  // Old value
    Value::Wildcard,  // New value
]);

while let Some(change) = space.r#in(pattern.clone()).await? {
    let var_name = &change.fields[1];
    let new_value = &change.fields[3];
    println!("{:?} changed to {:?}", var_name, new_value);
}
```

## Configuration

### Default Location

```rust
PersistentTupleSpace::open_default().await?
```

Stores at: `~/.config/colony/tuple-space.db`

### Custom Location

```rust
use colony_tuple::PersistentConfig;

let config = PersistentConfig {
    db_path: PathBuf::from("/tmp/my-tuples.db"),
    max_tuples: 50_000,
    cleanup_interval_secs: 600,  // 10 minutes
};

let space = PersistentTupleSpace::open(config).await?;
```

### Per-Directory (Future)

For project-specific coordination:
```
my-project/
  .colony/
    tuple-space.db    # Project-local tuple space
    config.toml       # Project configuration
```

## Performance

### In-Memory (`TupleSpace`)

- **Write (`out`)**: ~100 ns (just append to bucket)
- **Read (`inp/rdp`)**: ~500 ns - 2 μs (scan bucket)
- **Blocking (`in/rd`)**: 100ms timeout loop + Condvar

### Persistent (`PersistentTupleSpace`)

- **Write (`out`)**: ~100 μs (SQLite insert + memory write)
- **Read (cache hit)**: ~500 ns (same as in-memory)
- **Read (cache miss)**: ~50 μs (SQLite query + memory write)
- **Blocking**: Same as in-memory + periodic disk checks

### Memory Overhead

- Each tuple: ~200 bytes (in-memory) + ~300 bytes (SQLite)
- 10,000 tuples: ~5 MB total (2 MB memory + 3 MB disk)
- Cache is write-through, so no stale data

## Concurrency

Both implementations are **fully thread-safe**:

- `TupleSpace`: Uses `scc::HashMap` (lock-free) + `parking_lot::Mutex/Condvar`
- `PersistentTupleSpace`: Uses `parking_lot::Mutex` around SQLite connection
- Safe to `clone()` and share across threads (cheap Arc cloning)

## Limitations

### Current

- No expiration support yet (tuples live forever unless removed)
- No cross-instance notifications (polling-based, not event-driven)
- SQLite is single-writer (blocking writes from multiple processes)

### Future (F011: IPC)

Will add:
- `ipc-channel` for true multi-process coordination
- Event-driven notifications between instances
- Lock-free concurrent access from multiple shells

## Examples

Run the included examples:

```bash
# Distributed lock demo (run 3 terminals)
cargo run --example distributed_lock -- worker-1
cargo run --example distributed_lock -- worker-2
cargo run --example distributed_lock -- worker-3

# Task queue (producer/consumer)
cargo run --example task_queue producer
cargo run --example task_queue worker
```

## Testing

```bash
# Run all tests
cargo test -p colony-tuple

# Run specific test
cargo test -p colony-tuple test_persistence_across_open

# Run with output
cargo test -p colony-tuple -- --nocapture
```

Current test coverage:
- 8 in-memory tests
- 7 persistent tests
- 8 doc tests
- **Total: 23 tests, all passing**

## Integration

### In Colony Shell

```rust
use colony_tuple::PersistentTupleSpace;

// Create shared tuple space
let space = PersistentTupleSpace::open_default().await?;

// Pass to executor, agent, prompt manager
let executor = Executor::new(/* ... */, space.clone());
let agent = Agent::new(/* ... */, space.clone());
let prompt = PromptManager::new(/* ... */, space);
```

### In Shell Scripts (Future)

```colony
# Export emits tuple
export PATH="/new/path"
# Emits: ("env_change", "PATH", "/old", "/new")

# Builtin commands
ts:out ("task", "build", {project: "colony"})
var task = ts:in ("task", _, _)
lock:acquire "deploy"
```

## License

Same as Colony Shell project.
