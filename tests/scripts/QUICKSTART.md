# Quick Start: Running Colony Tests

## Setup (One-Time)

```bash
# Install BATS testing framework
git clone https://github.com/bats-core/bats-core.git /tmp/bats-core

# Build colony
cargo build --bin colony
```

## Run Tests

```bash
# From project root
cd /path/to/colony-shell

# Run all tests
COLONY_BIN="$(pwd)/target/debug/colony" /tmp/bats-core/bin/bats tests/scripts/bash-compat/*.bats

# Run specific test file
COLONY_BIN="$(pwd)/target/debug/colony" /tmp/bats-core/bin/bats tests/scripts/bash-compat/01_builtins.bats

# Verbose (show all output)
COLONY_BIN="$(pwd)/target/debug/colony" /tmp/bats-core/bin/bats --print-output-on-failure tests/scripts/bash-compat/01_builtins.bats
```

## Current Status

**⚠️ BLOCKED**: Colony needs `-c` flag implementation

Most tests currently fail because:
```bash
colony -c "echo hello"  # Not implemented yet
```

**Next step**: Implement `-c` flag in `crates/colony-shell/src/main.rs`

Once `-c` is implemented, tests should pass! 🎉

## Test Structure

- `bash-compat/` - POSIX compliance tests (compare against bash)
- `colony-variants/` - Colony-specific feature demos
- Each `.bats` file contains multiple `@test` blocks
- BATS provides TAP-compliant output

## Example Test

```bash
@test "echo builtin - simple output" {
    run "$COLONY" -c "echo hello"
    [ "$status" -eq 0 ]
    [ "$output" = "hello" ]
}
```

See `tests/scripts/STATUS.md` for detailed results and analysis.
