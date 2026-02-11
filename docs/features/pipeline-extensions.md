# Pipeline Extensions (F058)

## Overview

Colony Shell extends the traditional pipe operator (`|`) with three specialized pipeline operators for advanced control flow and error handling. These operators enable diagnostics, early termination, and error recovery within pipelines.

## Operators

### Standard Pipe (`|`)

The traditional POSIX pipe operator connects stdout of one command to stdin of the next:

```colony
ls -la | grep ".txt" | wc -l
```

### Tap Pipe (`|>`)

The tap operator copies data for diagnostic observation without interrupting the pipeline flow:

```colony
cat data.json | transform |> tee debug.json | validate
```

**Behavior:**
- Data flows to both the next command AND to stderr (fd 3)
- Useful for debugging intermediate pipeline stages
- Does not alter pipeline semantics

### Guard Pipe (`?>`)

The guard operator creates a checkpoint that stops the pipeline if the command fails:

```colony
fetch_data ?> validate_schema ?> transform | save
```

**Behavior:**
- If any guarded command fails (non-zero exit), the pipeline stops immediately
- Remaining commands after the failure point are not executed
- Enables "fail fast" semantics for critical pipeline stages

### Error Handler Pipe (`!>`)

The error handler operator defines a recovery path if the main pipeline fails:

```colony
risky_operation | process !> handle_error
```

**Behavior:**
- The main pipeline executes normally
- If any command in the main pipeline fails, the handler executes
- Handler receives error context and can perform cleanup or recovery
- Must be the final operator in the pipeline

## Usage Patterns

### Debugging with Tap

Inspect data at intermediate stages:

```colony
# Debug JSON transformation pipeline
curl https://api.example.com/data |> tee raw.json \
    | jq '.results' |> tee filtered.json \
    | process_data \
    | save_to_db
```

### Fail-Fast with Guard

Ensure data quality at each stage:

```colony
# Stop immediately if validation fails
fetch_user_data \
    ?> validate_json \
    ?> check_permissions \
    ?> sanitize_input \
    | update_database
```

Without guards, a validation failure might corrupt the database. With guards, the pipeline stops before the update.

### Error Recovery with Handler

Define fallback behavior for failures:

```colony
# Try primary method, fall back to alternative
primary_fetch | expensive_process !> {
    echo "Primary path failed, using cache";
    load_from_cache;
}
```

### Combining Operators

Mix operators for comprehensive control:

```colony
# Debug, guard, and handle errors
fetch_data \
    |> tee input.log \
    ?> validate \
    ?> transform \
    |> tee output.log \
    ?> save \
    !> rollback_transaction
```

## Implementation Details

### Files Involved

- `crates/colony-parser/colony.pest` - Grammar for pipeline operators
- `crates/colony-parser/src/ast.rs` - PipeOp enum variants
- `crates/colony-exec/src/pipeline.rs` - Extended pipeline execution logic
- `tests/scripts/bash-compat/53_pipeline_ext.bats` - 24 comprehensive tests

### Execution Strategy

#### Tap Pipe (`|>`)

1. Duplicate stdout to a diagnostic stream (fd 3 or stderr)
2. Continue normal pipeline flow to next command
3. Both streams operate concurrently

#### Guard Pipe (`?>`)

1. Split pipeline into segments at each guard
2. Execute each segment sequentially
3. Check exit status after each segment
4. Stop if any segment fails (non-zero exit)
5. Respect `pipefail` option if set

#### Error Handler Pipe (`!>`)

1. Split pipeline into main part and handler
2. Execute main pipeline with `pipefail` enforced
3. If main pipeline succeeds, return success
4. If main pipeline fails, execute handler
5. Return handler's exit status

### Operator Precedence

All pipe operators have the same precedence and are left-associative:

```colony
a | b |> c ?> d !> e
# Parsed as: ((((a | b) |> c) ?> d) !> e)
```

### Error Semantics

| Operator | Stops on Error? | Handler? | Pipefail? |
|----------|----------------|----------|-----------|
| `\|`     | No (unless pipefail) | No | Optional |
| `\|>`    | No | No | Inherits |
| `?>`     | Yes | No | Implicit |
| `!>`     | No (catches) | Yes | Forced |

## Examples

### Data Pipeline with Validation

```colony
proc process_user_csv(file) {
    cat "$file" \
        |> tee raw_input.csv \
        ?> validate_csv_schema \
        ?> check_required_fields \
        ?> remove_duplicates \
        |> tee cleaned.csv \
        ?> import_to_database \
        !> {
            echo "Pipeline failed, check logs";
            cleanup_temp_files;
            return 1;
        }
}
```

### HTTP Request with Retry

```colony
proc fetch_with_fallback(url, cache_file) {
    curl "$url" \
        ?> jq '.data' \
        ?> validate_response \
        !> {
            echo "Fetch failed, trying cache";
            cat "$cache_file";
        }
}
```

### Build Pipeline

```colony
# Stop immediately if linting or tests fail
proc ci_build() {
    echo "Running CI pipeline..." \
        |> tee ci.log;

    cargo fmt --check \
        ?> cargo clippy -- -D warnings \
        ?> cargo test --all \
        ?> cargo build --release \
        !> {
            echo "Build failed, check ci.log";
            notify_team "Build failure";
            return 1;
        }
}
```

## Comparison with Other Shells

### Bash/POSIX

Bash has limited pipeline control:

```bash
# Bash: pipefail must be set globally
set -o pipefail
cmd1 | cmd2 | cmd3
# All or nothing, no per-segment control
```

Colony Shell provides fine-grained control:

```colony
cmd1 ?> cmd2 | cmd3
# Only cmd1→cmd2 segment is guarded
```

### PowerShell

PowerShell uses try/catch for error handling:

```powershell
try {
    Get-Data | Process-Data | Save-Data
} catch {
    Handle-Error
}
```

Colony Shell integrates error handling directly in pipelines:

```colony
get_data | process_data | save_data !> handle_error
```

## Testing

The pipeline extensions have 24 comprehensive BATS tests covering:

- Basic tap pipe operation
- Tap pipe with multiple stages
- Guard pipe stopping on first failure
- Guard pipe continuing on success
- Error handler invocation on failure
- Error handler skipped on success
- Combined operators in complex pipelines
- Exit status propagation
- Pipefail interaction

Run tests with:

```bash
bats tests/scripts/bash-compat/53_pipeline_ext.bats
```

## Best Practices

### When to Use Tap (`|>`)

- Debugging data transformations
- Logging intermediate results
- Monitoring pipeline progress
- Capturing data for analysis without interrupting flow

### When to Use Guard (`?>`)

- Validating data quality at each stage
- Ensuring prerequisites before expensive operations
- Implementing fail-fast for critical pipelines
- Preventing cascading failures

### When to Use Error Handler (`!>`)

- Providing fallback paths for unreliable operations
- Cleanup on pipeline failure
- Logging errors with context
- Graceful degradation

### Avoid Overuse

Don't use guards everywhere:

```colony
# ❌ Overly defensive
cat file ?> grep pattern ?> sed 's/a/b/' ?> sort

# ✅ Guard only critical stages
cat file | grep pattern | sed 's/a/b/' ?> validate | save
```

## Future Enhancements

Potential future additions:

- Parallel pipes: `&|` for concurrent execution
- Broadcast pipes: `*>` to send to multiple destinations
- Conditional pipes: `|?` to filter based on exit status
- Buffered pipes: `|b>` with configurable buffer size
- Rate-limited pipes: `|@` with throughput control
