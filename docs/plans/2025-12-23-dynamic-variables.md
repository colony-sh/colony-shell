# Dynamic Variables Implementation Plan

Date: 2025-12-23

## Overview

Implement bash-compatible dynamic shell variables that return different values on each access.

## Tasks

### Setup
- [ ] 1. Add `rand` crate to `colony-expand/Cargo.toml` for RANDOM/SRANDOM generation

### Infrastructure
- [ ] 2. Add `shell_start_time: Instant` field to `ExpansionContext` for SECONDS calculation
- [ ] 9. Add LINENO tracking infrastructure (current line number in script/function)

### Dynamic Variables Implementation
- [ ] 3. Implement BASHPID - returns `getpid()` on each access (different in subshells)
- [ ] 4. Implement RANDOM - returns 0-32767 random integer, different each access
- [ ] 5. Implement SRANDOM - returns 32-bit cryptographic random integer
- [ ] 6. Implement SECONDS - seconds since shell start (can be assigned to reset)
- [ ] 7. Implement EPOCHSECONDS - current Unix timestamp (seconds since 1970-01-01)
- [ ] 8. Implement EPOCHREALTIME - Unix timestamp with microsecond precision (123456789.123456)

### Testing
- [ ] 10. Create BATS test file `tests/scripts/bash-compat/34_dynamic_vars.bats`
- [ ] 11. Run tests and fix any issues

## Implementation Notes

### Variable Behaviors

| Variable | Type | Description | Assignable |
|----------|------|-------------|------------|
| RANDOM | int | 0-32767, different each read | Yes (seeds RNG) |
| SRANDOM | int | 32-bit crypto random | No |
| SECONDS | int | Time since shell start | Yes (resets base time) |
| BASHPID | int | Current process PID | No |
| EPOCHSECONDS | int | Unix timestamp | No |
| EPOCHREALTIME | string | Unix timestamp.microseconds | No |
| LINENO | int | Current line number | No |

### Dependencies

- `rand` crate for RANDOM (standard RNG)
- `getrandom` crate for SRANDOM (cryptographic RNG)
- `std::time::Instant` for SECONDS
- `std::time::SystemTime` for EPOCH* variables
- `nix::unistd::getpid()` for BASHPID

### Architecture

Dynamic variables should be checked in `get_var_or_env()` BEFORE checking user variables and environment. They need special handling because:

1. They compute values on access (not stored)
2. Some are assignable (RANDOM seeds RNG, SECONDS resets base)
3. BASHPID must call `getpid()` each time (important for subshells)
