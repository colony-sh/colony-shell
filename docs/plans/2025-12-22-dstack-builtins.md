# Directory Stack Builtins Implementation Plan

## Overview
Implement `dirs`, `pushd`, and `popd` builtins for Colony shell.

## Implementation Steps

### 1. Add directory stack to ExpansionContext
- Add `dir_stack: Vec<PathBuf>` field
- Initialize with current directory on shell start
- Add accessor methods: `dir_stack()`, `dir_stack_mut()`, `push_dir()`, `pop_dir()`, etc.

### 2. Implement helper functions
- `tilde_abbreviate(path, home)` - Convert `/home/user/foo` to `~/foo`
- `parse_stack_index(arg)` - Parse `+N` and `-N` notation

### 3. Implement dirs builtin
- No options: Print stack space-separated with tilde abbreviation
- `-c`: Clear stack (keep only current dir)
- `-l`: Long listing (no tilde abbreviation)
- `-p`: One entry per line
- `-v`: Numbered entries per line
- `+N`: Display Nth from left
- `-N`: Display Nth from right

### 4. Implement pushd builtin
- No args: Swap top two entries
- `dir`: Push dir and cd to it
- `+N`: Rotate, bring Nth from left to top
- `-N`: Rotate, bring Nth from right to top
- `-n`: Manipulate stack without cd

### 5. Implement popd builtin
- No args: Pop top and cd to new top
- `+N`: Remove Nth from left
- `-N`: Remove Nth from right
- `-n`: Manipulate stack without cd

### 6. Update cd builtin
- Keep stack[0] synced with PWD

### 7. Create BATS tests
- File: `tests/scripts/bash-compat/21_dstack.bats`
