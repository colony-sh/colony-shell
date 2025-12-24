# Plan: Implement Export Functions (export -f) for Colony Shell

## Overview

Implement bash's ability to export shell functions to child processes via environment variables.

## Research Summary

### Bash Function Export Format

Bash uses `BASH_FUNC_<name>%%` environment variables to pass exported functions:

```
BASH_FUNC_myfunc%%=() {  echo hello
}
```

Key points:
- Prefix: `BASH_FUNC_`
- Suffix: `%%`
- Value starts with `() {`
- Function body follows

### Import on Startup

When bash starts, it scans environment for `BASH_FUNC_*%%` variables and imports them as functions.

## Implementation Plan

### 1. Extend ExpansionContext (context.rs)

Add tracking for exported functions:
```rust
exported_functions: HashSet<String>,  // Track which functions are marked for export
```

Methods to add:
- `export_function(&mut self, name: &str)` - Mark function for export
- `unexport_function(&mut self, name: &str)` - Remove export flag
- `is_function_exported(&self, name: &str) -> bool`
- `exported_functions(&self) -> impl Iterator<Item = &String>`
- `get_functions(&self) -> &HashMap<String, FunctionDef>` - Get all functions

### 2. Extend export Builtin (builtins.rs)

Add `-f` and `-fn` flag handling:
- `export -f funcname` - Export a function
- `export -fn funcname` - Remove export flag from function
- `export -f` (no args) - List all exported functions

### 3. Extend declare Builtin (builtins.rs)

Add function listing:
- `declare -f` - Show all function definitions
- `declare -f funcname` - Show specific function definition
- `declare -F` - Show function names only
- `declare -F funcname` - Show function name if defined

### 4. Serialize Functions to Environment

When spawning child colony processes:
- Iterate exported functions
- Encode as `BASH_FUNC_name%%=() { body }` in environment

### 5. Load Functions on Shell Startup (main.rs)

During shell initialization:
- Scan environment for `BASH_FUNC_*%%` variables
- Parse function definition from value
- Register function in ExpansionContext

### 6. Create BATS Tests

Create `tests/scripts/bash-compat/44_exportfunc.bats` with comprehensive tests.

## Files to Modify

1. `crates/colony-expand/src/context.rs` - Add exported_functions tracking
2. `crates/colony-exec/src/builtins.rs` - Extend export and declare builtins
3. `crates/colony-shell/src/main.rs` - Load exported functions on startup
4. `crates/colony-exec/src/command.rs` - Pass exported functions to child processes
5. `tests/scripts/bash-compat/44_exportfunc.bats` - Test suite

## Test Cases

1. Basic export -f
2. Export function to subshell
3. Export function to child colony process
4. declare -f shows definitions
5. declare -F shows names only
6. export -fn removes export flag
7. Function not available after unexport
8. Import from environment on startup
9. Invalid function names rejected
10. Empty function body handling
