# System Variables Implementation Plan

## Goal
Implement bash-compatible system variables for Colony shell.

## Variables to Implement

### User/Process Variables
- `UID` - Real user ID (from `getuid()`)
- `EUID` - Effective user ID (from `geteuid()`)
- `PPID` - Parent process ID (from `getppid()`)
- `USER` - Username (fallback to passwd lookup if not in env)
- `SHELL` - Path to shell executable

### Machine Variables
- `HOSTNAME` - Machine hostname (from `gethostname()`)
- `HOSTTYPE` - CPU architecture (e.g., `x86_64`, `aarch64`)
- `OSTYPE` - OS type with version (e.g., `darwin24.4.0`, `linux-gnu`)
- `MACHTYPE` - Full machine type (e.g., `aarch64-apple-darwin24.4.0`)

## Implementation Approach

### Phase 1: Add nix dependency to colony-expand
The colony-expand crate needs `nix` for `getuid()`, `geteuid()`, `getppid()`, `gethostname()`.

### Phase 2: Compute machine type at init
For values that don't change, compute once at shell initialization:
- `HOSTNAME` - from `nix::unistd::gethostname()`
- `HOSTTYPE` - from `std::env::consts::ARCH`
- `OSTYPE` - constructed from OS + version
- `MACHTYPE` - combined `ARCH-vendor-OS`

### Phase 3: Dynamic values in get_var_or_env
For values that could theoretically change:
- `UID`, `EUID`, `PPID` - compute on access (though they don't change)

### Architecture Notes
- These are read-only - bash doesn't allow modifying UID/EUID/etc.
- Add to ExpansionContext with cached values
- Check these BEFORE user variables in get_var_or_env

## Reference Values (macOS M1)
```
UID=501 EUID=501 PPID=73612
HOSTNAME=mcfearsome-LKGRK7JLWT
HOSTTYPE=aarch64 OSTYPE=darwin24.4.0 MACHTYPE=aarch64-apple-darwin24.4.0
SHELL=/bin/zsh USER=mcfearsome
```

## Testing
```bash
./target/debug/colony -c 'echo "UID=$UID EUID=$EUID PPID=$PPID"'
./target/debug/colony -c 'echo "HOSTNAME=$HOSTNAME"'
./target/debug/colony -c 'echo "HOSTTYPE=$HOSTTYPE OSTYPE=$OSTYPE MACHTYPE=$MACHTYPE"'
./target/debug/colony -c 'echo "SHELL=$SHELL USER=$USER"'
```
