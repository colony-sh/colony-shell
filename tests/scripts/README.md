# Comprehensive Test Suite: Real Bash Scripts

This directory contains real-world bash scripts sourced from the web for POSIX compatibility testing.

## Architecture

### Directory Structure

```
tests/scripts/
├── README.md                    # This file
├── SOURCES.md                   # Attribution and sources for all scripts
├── runner.sh                    # Test harness that runs all scripts
├── runner.rs                    # Rust test runner (future)
├── bash-compat/                 # Official bash test suite tests
│   ├── 01_builtins/
│   │   ├── builtins.tests      # From bash git repo
│   │   ├── builtins.right      # Expected output
│   │   ├── LICENSE             # GPL v3
│   │   └── SOURCE.txt          # Git URL and commit
│   ├── 02_varenv/
│   │   ├── varenv.tests
│   │   ├── varenv.right
│   │   ├── LICENSE
│   │   └── SOURCE.txt
│   ├── 03_quote/
│   ├── 04_redir/
│   ├── 05_pipeline/
│   ├── 06_case/
│   ├── 07_conditional/
│   ├── 08_loop/
│   └── 09_func/
└── colony-variants/             # Colony-enhanced demonstrations
    ├── typed-pipes/
    │   ├── demo.sh             # Shows -> and => pipes
    │   ├── demo.right          # Expected output
    │   └── README.md           # Feature explanation
    ├── reactive-tuples/
    │   ├── demo.sh             # Tuple space integration
    │   ├── demo.right
    │   └── README.md
    └── modern-syntax/
        ├── demo.sh             # var/const/setvar
        ├── demo.right
        └── README.md
```

### Metadata Format (metadata.toml)

```toml
# Test metadata for script execution and validation

[test]
name = "Hello World Basic"
description = "Simple hello world script to verify basic execution"
category = "fundamentals"
difficulty = "basic"  # basic | intermediate | advanced

[source]
url = "https://example.com/script.sh"
author = "Original Author"
license = "MIT"
date_sourced = "2025-12-17"

[execution]
# Expected exit code (default: 0)
expected_exit_code = 0

# Whether this script requires input
requires_input = false
input_file = "input.txt"  # optional

# Environment variables needed
[execution.env]
TEST_VAR = "value"

# Expected behavior
[validation]
# Compare output exactly, or just check exit code
mode = "exact"  # exact | exit_code | regex | contains

# For regex mode
output_pattern = "^Hello.*world"

# Timeout in seconds
timeout = 5

# Skip this test (with reason)
skip = false
skip_reason = ""

[colony]
# For colony-variants only: what features are demonstrated
features = ["typed-pipes", "reactive-tuples"]
baseline_script = "../bash-compat/01_hello_world/script.sh"
```

## Test Categories

1. **fundamentals** - Basic shell operations
   - Hello world, echo, variable assignment
   - Basic command execution

2. **expansion** - Variable and command expansion
   - Parameter expansion (`${var}`, `${var:-default}`)
   - Command substitution (`` `cmd` ``, `$(cmd)`)
   - Arithmetic expansion (`$((expr))`)
   - Tilde expansion (`~`, `~user`)

3. **control-flow** - Conditional and loop constructs
   - if/then/else/fi
   - while/until loops
   - for loops (C-style and word list)
   - case statements

4. **pipelines** - Process pipelines and composition
   - Simple pipes (`cmd1 | cmd2`)
   - Multi-stage pipelines
   - Named pipes (FIFOs)

5. **redirections** - File descriptor manipulation
   - Output redirection (`>`, `>>`)
   - Input redirection (`<`)
   - Heredocs (`<<EOF`)
   - File descriptor duplication (`>&`, `<&`)

6. **functions** - Function definitions and calls
   - POSIX function definitions
   - Local variables
   - Return values

7. **advanced** - Complex real-world scenarios
   - Error handling
   - Signal handling
   - Complex pipelines
   - Process substitution

8. **colony-features** - Colony-specific enhancements (colony-variants only)
   - Typed pipes (`->`, `=>`)
   - Reactive tuples
   - Modern syntax

## Test Runner

The `runner.sh` script:

1. **Discovery**: Finds all `metadata.toml` files
2. **Execution**: Runs each script in isolation
3. **Validation**: Compares output against expectations
4. **Reporting**: Generates summary (passed/failed/skipped)

### Usage

```bash
# Run all tests
./runner.sh

# Run specific category
./runner.sh --category expansion

# Run single test
./runner.sh --test bash-compat/01_hello_world

# Compare colony vs bash output
./runner.sh --compare
```

## Script Sourcing Guidelines

When adding scripts from the web:

1. **Always include attribution** in `SOURCES.md`
2. **Respect licenses** - prefer MIT, BSD, Apache, public domain
3. **Minimal modification** - keep bash-compat scripts as-is
4. **Document changes** - if modifications needed, document why
5. **Test first** - verify script works in bash before adding

## Colony Variants

Colony variants should:

1. **Maintain compatibility** - still valid POSIX shell scripts
2. **Demonstrate features** - show off Colony enhancements
3. **Include explanation** - README.md explains what's enhanced
4. **Be educational** - teach Colony features through examples

Example enhancement opportunities:

- Replace verbose pipes with typed pipes
- Add tuple notifications for state changes
- Use modern variable syntax
- Leverage reactive features

## Goals

1. **Validate POSIX compliance** - ensure Colony runs real bash scripts
2. **Surface edge cases** - find parser/executor bugs
3. **Demonstrate Colony features** - show modern enhancements
4. **Educational resource** - teach both bash and Colony

## Contributing

To add a new test:

1. Find a real bash script (with compatible license)
2. Create directory in `bash-compat/NN_name/`
3. Add `script.sh`, `metadata.toml`
4. Update `SOURCES.md` with attribution
5. Optionally create Colony variant in `colony-variants/NN_name/`
6. Test with `./runner.sh --test bash-compat/NN_name`

---

**Status**: Feature F023 - In Progress
**Dependencies**: F021 (pest parser), F003 (expansion), F007 (builtins)
