# Heartbeat Scheduler Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a scheduled agent execution system driven by HEARTBEAT.md checklists with configurable intervals, security modes, and a web dashboard via the serve builtin.

**Architecture:** The heartbeat scheduler is a background tokio task inside `colony-exec` that reads a user-authored HEARTBEAT.md checklist on a configurable interval, creates a fresh `colony-agent::Agent` per tick with a security-mode-scoped `ToolRegistry`, captures structured findings (PASS/WARN/ALERT), stores runs in the shared reflection SQLite DB, and emits tuples for reactive coordination. A `heartbeat render` subcommand generates a self-contained HTML dashboard that integrates with the `serve` builtin.

**Tech Stack:** Rust, colony-agent, rusqlite, tokio (timer/spawn), colony-tuple, colony-reflection (DB path), serde/serde_json, uuid, chrono

**Spec:** `docs/superpowers/specs/2026-03-16-heartbeat-scheduler-design.md`

---

## File Structure

### New files in `crates/colony-exec/src/heartbeat/`

| File | Responsibility |
|------|---------------|
| `mod.rs` | `HeartbeatScheduler`: start/stop/run/status/interval, background task lifecycle, config state |
| `parser.rs` | Checklist parser: YAML frontmatter extraction (`HeartbeatConfig`), Markdown checkbox items (`ChecklistItem`), empty-body detection |
| `runner.rs` | Per-tick execution: context building (system prompt + checklist + CONTEXT.md), `Agent` creation with custom `ToolRegistry`, timeout enforcement, finding extraction |
| `types.rs` | `HeartbeatConfig`, `ChecklistItem`, `HeartbeatRun`, `Finding`, `Severity`, `HeartbeatStatus`, `SecurityMode` |
| `storage.rs` | SQLite read/write to shared reflection DB: `CREATE TABLE IF NOT EXISTS heartbeat_runs`, insert, query, 7-day pruning |
| `dashboard.rs` | HTML rendering for serve integration: checklist section, run history table, live findings with color coding |

### New crate: `crates/colony-heartbeat-bin/`

| File | Responsibility |
|------|---------------|
| `Cargo.toml` | Crate manifest for panel binary |
| `src/main.rs` | Panel protocol: subscribe to `heartbeat_complete` tuples, render status/findings/next-run, key bindings (Enter = run, l = log) |

### Modified files

| File | Changes |
|------|---------|
| `Cargo.toml` (workspace root) | Add `"crates/colony-heartbeat-bin"` to `[workspace.members]` |
| `crates/colony-exec/Cargo.toml` | Add `colony-agent` dependency |
| `crates/colony-exec/src/lib.rs` | Add `pub mod heartbeat;` module declaration |
| `crates/colony-exec/src/builtins.rs` | Add `"heartbeat"` to `is_builtin_base()` match; add dispatch to `builtin_heartbeat()` in `execute_builtin()` match; add `builtin_heartbeat()` function |
| `crates/colony-exec/src/special_files/heartbeat.rs` | **Replace entirely**: remove `HeartbeatParser` (agent-status format) and `HeartbeatWatcher`; re-export from new `heartbeat::parser` module or keep as thin redirect |
| `crates/colony-exec/src/special_files/mod.rs` | Update `HeartbeatParser` import to use new checklist parser; update `try_load_heartbeat_md()` to return new types |
| `crates/colony-exec/src/special_files/types.rs` | Replace `HeartbeatEntry` and F030 `HeartbeatStatus` with new checklist-oriented types; keep `SpecialFileType::Heartbeat` variant unchanged |
| `crates/colony-shell/Cargo.toml` | Add `colony-heartbeat-bin` binary entry (`[[bin]]` section) |

---

## Chunk 1: Types and Checklist Parser

### Task 1: Create heartbeat module skeleton and types

**Files:**
- Create: `crates/colony-exec/src/heartbeat/mod.rs`
- Create: `crates/colony-exec/src/heartbeat/types.rs`
- Modify: `crates/colony-exec/src/lib.rs`

- [ ] **Step 1: Create the heartbeat module directory**

```bash
mkdir -p crates/colony-exec/src/heartbeat
```

- [ ] **Step 2: Create `types.rs` with all core types**

In `crates/colony-exec/src/heartbeat/types.rs`:

```rust
use serde::{Deserialize, Serialize};

/// Security mode controlling which tools the heartbeat agent can use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SecurityMode {
    /// Read-only: file_read, shell_exec (stdout capture), search
    Notify,
    /// Read + write: adds file_write
    Active,
    /// Unrestricted: full shell_exec
    Full,
}

impl Default for SecurityMode {
    fn default() -> Self { Self::Notify }
}

impl SecurityMode {
    pub fn from_str_lossy(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "active" => Self::Active,
            "full" => Self::Full,
            _ => Self::Notify,
        }
    }
}

/// Parsed HEARTBEAT.md frontmatter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatConfig {
    pub interval_secs: u64,
    pub mode: SecurityMode,
    pub notify: bool,
    pub timeout_secs: u64,
    pub max_tool_calls: u32,
    pub token_budget: u32,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            interval_secs: 1800, // 30m
            mode: SecurityMode::Notify,
            notify: true,
            timeout_secs: 60,
            max_tool_calls: 20,
            token_budget: 10_000,
        }
    }
}

/// A single checklist item from HEARTBEAT.md.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChecklistItem {
    pub text: String,
    pub checked: bool,
}

/// Severity of a heartbeat finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Pass,
    Warn,
    Alert,
}

/// A single finding extracted from agent output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub severity: Severity,
    pub message: String,
}

/// Overall status of a heartbeat run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HeartbeatStatus {
    Pass,
    Warn,
    Alert,
    Unstructured,
    Timeout,
    Error,
}

impl std::fmt::Display for HeartbeatStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Pass => "pass",
            Self::Warn => "warn",
            Self::Alert => "alert",
            Self::Unstructured => "unstructured",
            Self::Timeout => "timeout",
            Self::Error => "error",
        };
        f.write_str(s)
    }
}

impl HeartbeatStatus {
    pub fn from_str_lossy(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "pass" => Self::Pass,
            "warn" => Self::Warn,
            "alert" => Self::Alert,
            "unstructured" => Self::Unstructured,
            "timeout" => Self::Timeout,
            "error" => Self::Error,
            _ => Self::Unstructured,
        }
    }
}

/// Record of a single heartbeat run, stored in SQLite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatRun {
    pub id: String,
    pub timestamp: i64,
    pub duration_ms: u64,
    pub status: HeartbeatStatus,
    pub findings: Vec<Finding>,
    pub raw_output: String,
    pub checklist_path: String,
    pub interval_secs: u64,
}
```

- [ ] **Step 3: Create `mod.rs` with module declarations**

In `crates/colony-exec/src/heartbeat/mod.rs`:

```rust
//! Heartbeat Scheduler (F080)
//!
//! A scheduled agent execution system driven by user-authored HEARTBEAT.md
//! checklists. The agent processes the checklist on a configurable interval,
//! executes checks using shell tools, reports findings, and stores results
//! in the shared reflection database.

pub mod types;
pub mod parser;
// pub mod runner;    -- added in Chunk 3
// pub mod storage;   -- added in Chunk 2
// pub mod dashboard; -- added in Chunk 6

pub use types::*;
pub use parser::parse_heartbeat;
```

- [ ] **Step 4: Add module declaration to `lib.rs`**

In `crates/colony-exec/src/lib.rs`, add `pub mod heartbeat;` alongside the other module declarations.

- [ ] **Step 5: Verify it compiles**

Run: `cargo check -p colony-exec`
Expected: compiles (parser module not yet created, will be added next)

### Task 2: Implement checklist parser with YAML frontmatter

**Files:**
- Create: `crates/colony-exec/src/heartbeat/parser.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Write tests first**

In `crates/colony-exec/src/heartbeat/parser.rs`, add tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::heartbeat::types::SecurityMode;

    #[test]
    fn test_parse_with_frontmatter_and_items() {
        let content = "---\ninterval: 15m\nmode: active\ntimeout: 120s\n---\n\n# Heartbeat\n\n- [ ] Check tests\n- [x] Already done\n- [ ] Check disk\n";
        let (config, items) = parse_heartbeat(content);
        assert_eq!(config.interval_secs, 900);
        assert_eq!(config.mode, SecurityMode::Active);
        assert_eq!(config.timeout_secs, 120);
        assert_eq!(items.len(), 3);
        assert!(!items[0].checked);
        assert!(items[1].checked);
        assert!(!items[2].checked);
    }

    #[test]
    fn test_parse_no_frontmatter() {
        let content = "# Heartbeat\n\n- [ ] Check tests\n";
        let (config, items) = parse_heartbeat(content);
        assert_eq!(config.interval_secs, 1800); // default 30m
        assert_eq!(config.mode, SecurityMode::Notify);
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn test_parse_empty_body() {
        let content = "---\ninterval: 5m\n---\n\n# Heartbeat\n\n";
        let (config, items) = parse_heartbeat(content);
        assert_eq!(config.interval_secs, 300);
        assert!(items.is_empty());
    }

    #[test]
    fn test_parse_duration_formats() {
        assert_eq!(parse_duration("5m"), Some(300));
        assert_eq!(parse_duration("15m"), Some(900));
        assert_eq!(parse_duration("30m"), Some(1800));
        assert_eq!(parse_duration("1h"), Some(3600));
        assert_eq!(parse_duration("4h"), Some(14400));
        assert_eq!(parse_duration("1d"), Some(86400));
        assert_eq!(parse_duration("60s"), Some(60));
        assert_eq!(parse_duration("120s"), Some(120));
        assert_eq!(parse_duration("garbage"), None);
    }

    #[test]
    fn test_active_items_only() {
        let content = "- [ ] Active item\n- [x] Done item\n";
        let (_, items) = parse_heartbeat(content);
        let active: Vec<_> = items.iter().filter(|i| !i.checked).collect();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].text, "Active item");
    }

    #[test]
    fn test_body_is_empty_check() {
        assert!(body_is_empty("# Heartbeat\n\n## Section\n\n"));
        assert!(body_is_empty("   \n\n  \n"));
        assert!(body_is_empty(""));
        assert!(!body_is_empty("- [ ] Check something\n"));
    }
}
```

- [ ] **Step 2: Implement the parser**

In `crates/colony-exec/src/heartbeat/parser.rs`:

```rust
//! Checklist parser for HEARTBEAT.md files.
//!
//! Parses optional YAML frontmatter (delimited by `---`) for configuration
//! and Markdown checkbox items (`- [ ]` / `- [x]`) as checklist entries.

use super::types::{ChecklistItem, HeartbeatConfig, SecurityMode};

/// Parse a HEARTBEAT.md file into configuration and checklist items.
pub fn parse_heartbeat(content: &str) -> (HeartbeatConfig, Vec<ChecklistItem>) {
    let (frontmatter, body) = split_frontmatter(content);
    let config = parse_frontmatter(frontmatter);
    let items = parse_checklist(body);
    (config, items)
}

/// Check whether the body (after frontmatter) contains only headers and whitespace.
pub fn body_is_empty(body: &str) -> bool {
    body.lines().all(|line| {
        let trimmed = line.trim();
        trimmed.is_empty() || trimmed.starts_with('#')
    })
}
```

The implementation should include:
- `split_frontmatter(content) -> (Option<&str>, &str)` -- splits on `---` delimiters
- `parse_frontmatter(yaml: Option<&str>) -> HeartbeatConfig` -- parses known keys: `interval`, `mode`, `notify`, `timeout`, `max_tool_calls`, `token_budget`
- `parse_duration(s: &str) -> Option<u64>` -- handles `5m`, `15m`, `30m`, `1h`, `4h`, `1d`, `60s`, `120s`
- `parse_checklist(body: &str) -> Vec<ChecklistItem>` -- extracts `- [ ]` and `- [x]` lines

The frontmatter parser should use simple line-by-line key-value parsing (not a full YAML parser dependency). Each line is `key: value`. Unknown keys are ignored.

- [ ] **Step 3: Verify tests pass**

Run: `cargo test -p colony-exec -- heartbeat::parser`

- [ ] **Step 4: Commit**

```bash
git add crates/colony-exec/src/heartbeat/ crates/colony-exec/src/lib.rs
git commit -m "feat(heartbeat): add F080 types and checklist parser"
```

---

## Chunk 2: Replace F030 HeartbeatParser and Update Types

### Task 3: Replace the F030 HeartbeatParser and HeartbeatWatcher

**Files:**
- Modify: `crates/colony-exec/src/special_files/heartbeat.rs`
- Modify: `crates/colony-exec/src/special_files/types.rs`
- Modify: `crates/colony-exec/src/special_files/mod.rs`

The existing `HeartbeatParser` in `special_files/heartbeat.rs` parses the agent-status format (`## agent: <name>` with `- key: value` metadata). The spec says to replace this with the checklist format. The `SpecialFileType::Heartbeat` variant stays.

- [ ] **Step 1: Update types.rs**

In `crates/colony-exec/src/special_files/types.rs`:

1. Remove the old `HeartbeatEntry` struct (lines 96-107) and old `HeartbeatStatus` enum (lines 109-148).
2. Replace with re-exports from the new heartbeat module:

```rust
// Re-export heartbeat types from the F080 scheduler module.
// The heartbeat types are canonical in heartbeat::types and re-exported
// here for backwards compatibility with the F030 special files system.
pub use crate::heartbeat::types::{ChecklistItem, HeartbeatConfig};
```

3. In `SpecialFilesState` (line 191-199), replace `heartbeat_entries: Vec<HeartbeatEntry>` with:

```rust
pub heartbeat_config: Option<HeartbeatConfig>,
pub heartbeat_items: Vec<ChecklistItem>,
pub heartbeat_path: Option<std::path::PathBuf>,
```

- [ ] **Step 2: Replace heartbeat.rs parser**

Replace the entire contents of `crates/colony-exec/src/special_files/heartbeat.rs` with:

```rust
//! Heartbeat file discovery for F030 special files.
//!
//! The checklist parser lives in `crate::heartbeat::parser`.
//! This module re-exports it for F030 integration and provides
//! the file discovery path for `heartbeat start`.

pub use crate::heartbeat::parser::{parse_heartbeat, body_is_empty};
pub use crate::heartbeat::types::{ChecklistItem, HeartbeatConfig};
```

Remove `HeartbeatWatcher` entirely -- the new scheduler uses a timer-based approach instead of filesystem watching.

- [ ] **Step 3: Update mod.rs imports and loader**

In `crates/colony-exec/src/special_files/mod.rs`:

1. Update the `pub use heartbeat::` line (line 29) to export the new types:
   ```rust
   pub use heartbeat::{parse_heartbeat, body_is_empty, ChecklistItem, HeartbeatConfig};
   ```
   Remove the `HeartbeatWatcher` re-export.

2. Update `SpecialFilesState` initialization in `load()` (line 57-87) and `load_with_hierarchy()` (line 96-157) to use the new fields:
   ```rust
   heartbeat_config: None,
   heartbeat_items: Vec::new(),
   heartbeat_path: None,
   ```

3. Update `try_load_heartbeat_md()` (line 262-266) to use the new parser:
   ```rust
   fn try_load_heartbeat_md(dir: &Path) -> Result<(HeartbeatConfig, Vec<ChecklistItem>, PathBuf), SpecialFilesError> {
       let path = dir.join("HEARTBEAT.md");
       let Some(content) = read_file_if_exists(&path)? else {
           return Err(SpecialFilesError::Io {
               path: path.display().to_string(),
               source: std::io::Error::new(std::io::ErrorKind::NotFound, "not found"),
           });
       };
       let (config, items) = parse_heartbeat(&content);
       Ok((config, items, path))
   }
   ```

4. Update the callers of `try_load_heartbeat_md()` in `load()` and `load_with_hierarchy()` to populate the new `SpecialFilesState` fields. In `load_with_hierarchy()`, heartbeat discovery uses first-found semantics (not merging) since F080 operates on a single HEARTBEAT.md:

   ```rust
   // In load_with_hierarchy(), heartbeat is first-found (not merging)
   if state.heartbeat_path.is_none() {
       if let Ok((config, items, path)) = Self::try_load_heartbeat_md(search_dir) {
           state.heartbeat_config = Some(config);
           state.heartbeat_items = items;
           state.heartbeat_path = Some(path);
       }
   }
   ```

   Note: This changes heartbeat from merging to first-found. The spec says the scheduler operates on a single HEARTBEAT.md, so merging multiple checklists is not needed. Update `SpecialFileType::Heartbeat.merges_up_tree()` to return `false`.

- [ ] **Step 4: Fix all compile errors from type changes**

The `heartbeat_entries` field change will break any code referencing it. Search for all usages:

```bash
cargo check -p colony-exec 2>&1 | head -50
```

Fix each compile error. The main breakage points are:
- `SpecialFilesState` construction in test code
- Any code reading `state.heartbeat_entries`
- The `HeartbeatStatus::from_str_lossy` call sites (old F030 status enum removed)

- [ ] **Step 5: Update tests**

In `crates/colony-exec/src/special_files/mod.rs`, update the test `test_load_all_files` (line 313) to use new HEARTBEAT.md format:

```rust
std::fs::write(dir.path().join("HEARTBEAT.md"), "---\ninterval: 15m\n---\n\n# Heartbeat\n\n- [ ] Check tests\n").unwrap();
```

And update the assertions:
```rust
assert!(state.heartbeat_config.is_some());
assert_eq!(state.heartbeat_items.len(), 1);
```

Update `test_hierarchy_heartbeat_merges` to test first-found semantics instead of merging.

- [ ] **Step 6: Verify all tests pass**

Run: `cargo test -p colony-exec -- special_files`

- [ ] **Step 7: Commit**

```bash
git add crates/colony-exec/src/special_files/ crates/colony-exec/src/heartbeat/
git commit -m "feat(heartbeat): replace F030 HeartbeatParser with checklist format"
```

---

## Chunk 3: Storage Layer

### Task 4: Implement HeartbeatRun storage

**Files:**
- Create: `crates/colony-exec/src/heartbeat/storage.rs`
- Modify: `crates/colony-exec/src/heartbeat/mod.rs`

- [ ] **Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::heartbeat::types::*;

    fn make_run(status: HeartbeatStatus) -> HeartbeatRun {
        HeartbeatRun {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now().timestamp(),
            duration_ms: 5000,
            status,
            findings: vec![Finding { severity: Severity::Pass, message: "all good".into() }],
            raw_output: "PASS: all good".into(),
            checklist_path: "/project/HEARTBEAT.md".into(),
            interval_secs: 1800,
        }
    }

    #[test]
    fn test_create_table_and_insert() {
        let store = HeartbeatStore::open_in_memory().unwrap();
        let run = make_run(HeartbeatStatus::Pass);
        store.insert(&run).unwrap();
        let runs = store.recent_runs(10).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].id, run.id);
    }

    #[test]
    fn test_recent_runs_ordering() {
        let store = HeartbeatStore::open_in_memory().unwrap();
        for i in 0..5 {
            let mut run = make_run(HeartbeatStatus::Pass);
            run.timestamp = 1000 + i;
            store.insert(&run).unwrap();
        }
        let runs = store.recent_runs(3).unwrap();
        assert_eq!(runs.len(), 3);
        assert!(runs[0].timestamp > runs[1].timestamp); // reverse chronological
    }

    #[test]
    fn test_get_run_by_id() {
        let store = HeartbeatStore::open_in_memory().unwrap();
        let run = make_run(HeartbeatStatus::Warn);
        store.insert(&run).unwrap();
        let fetched = store.get_run(&run.id).unwrap();
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().status, HeartbeatStatus::Warn);
    }

    #[test]
    fn test_prune_old_runs() {
        let store = HeartbeatStore::open_in_memory().unwrap();
        let mut old_run = make_run(HeartbeatStatus::Pass);
        old_run.timestamp = chrono::Utc::now().timestamp() - (8 * 86400); // 8 days ago
        store.insert(&old_run).unwrap();
        let recent_run = make_run(HeartbeatStatus::Pass);
        store.insert(&recent_run).unwrap();
        store.prune(7).unwrap(); // 7-day retention
        let runs = store.recent_runs(100).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].id, recent_run.id);
    }
}
```

- [ ] **Step 2: Implement HeartbeatStore**

In `crates/colony-exec/src/heartbeat/storage.rs`:

```rust
//! SQLite storage for heartbeat run records.
//!
//! Uses the shared reflection database at ~/.colony/reflection/observations.db.
//! Creates its own table via CREATE TABLE IF NOT EXISTS -- does not depend on
//! colony-reflection's migration system.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use rusqlite::Connection;
use super::types::{HeartbeatRun, HeartbeatStatus, Finding, Severity};
```

The `HeartbeatStore` struct:
- `conn: Mutex<Connection>` (same pattern as `ObservationStore`)
- `open(db_path: &Path) -> Result<Self>` -- opens DB, sets WAL mode, calls `init_schema()`
- `open_in_memory() -> Result<Self>` -- for tests
- `default_path() -> PathBuf` -- returns `dirs::home_dir().join(".colony/reflection/observations.db")` (same path as `ObservationStore::default_path()`)
- `init_schema()` -- executes:
  ```sql
  CREATE TABLE IF NOT EXISTS heartbeat_runs (
      id TEXT PRIMARY KEY,
      timestamp INTEGER NOT NULL,
      duration_ms INTEGER NOT NULL,
      status TEXT NOT NULL,
      findings_json TEXT NOT NULL,
      raw_output TEXT NOT NULL,
      checklist_path TEXT NOT NULL,
      interval_secs INTEGER NOT NULL
  );
  CREATE INDEX IF NOT EXISTS idx_heartbeat_timestamp ON heartbeat_runs(timestamp DESC);
  ```
- `insert(&self, run: &HeartbeatRun) -> Result<()>` -- serializes `run.findings` to JSON via `serde_json::to_string()`
- `recent_runs(&self, limit: usize) -> Result<Vec<HeartbeatRun>>` -- `ORDER BY timestamp DESC LIMIT ?`
- `get_run(&self, id: &str) -> Result<Option<HeartbeatRun>>` -- by primary key
- `prune(&self, retention_days: u64) -> Result<u64>` -- `DELETE WHERE timestamp < ?` based on `Utc::now() - retention_days * 86400`

When reading rows back, deserialize `findings_json` with `serde_json::from_str::<Vec<Finding>>()`, falling back to empty vec on parse failure. Parse `status` with `HeartbeatStatus::from_str_lossy()`.

- [ ] **Step 3: Update mod.rs**

Add to `crates/colony-exec/src/heartbeat/mod.rs`:
```rust
pub mod storage;
pub use storage::HeartbeatStore;
```

- [ ] **Step 4: Verify tests pass**

Run: `cargo test -p colony-exec -- heartbeat::storage`

- [ ] **Step 5: Commit**

```bash
git add crates/colony-exec/src/heartbeat/storage.rs crates/colony-exec/src/heartbeat/mod.rs
git commit -m "feat(heartbeat): add HeartbeatStore for run persistence"
```

---

## Chunk 4: Finding Extraction and Per-Tick Runner

### Task 5: Implement finding extraction

**Files:**
- Add to: `crates/colony-exec/src/heartbeat/runner.rs`

- [ ] **Step 1: Write finding extraction tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_findings_structured() {
        let output = "PASS: Tests passing (147/147)\nWARN: 3 new TODOs\nALERT: Server down\nSome other text\nPASS: Disk ok\n";
        let findings = extract_findings(output);
        assert_eq!(findings.len(), 4);
        assert_eq!(findings[0].severity, Severity::Pass);
        assert_eq!(findings[0].message, "Tests passing (147/147)");
        assert_eq!(findings[1].severity, Severity::Warn);
        assert_eq!(findings[2].severity, Severity::Alert);
        assert_eq!(findings[3].severity, Severity::Pass);
    }

    #[test]
    fn test_extract_findings_none_found() {
        let output = "I checked everything and it looks fine.\nNo issues detected.";
        let findings = extract_findings(output);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_derive_status_pass() {
        let findings = vec![
            Finding { severity: Severity::Pass, message: "ok".into() },
            Finding { severity: Severity::Pass, message: "ok".into() },
        ];
        assert_eq!(derive_status(&findings), HeartbeatStatus::Pass);
    }

    #[test]
    fn test_derive_status_warn() {
        let findings = vec![
            Finding { severity: Severity::Pass, message: "ok".into() },
            Finding { severity: Severity::Warn, message: "warning".into() },
        ];
        assert_eq!(derive_status(&findings), HeartbeatStatus::Warn);
    }

    #[test]
    fn test_derive_status_alert() {
        let findings = vec![
            Finding { severity: Severity::Warn, message: "warning".into() },
            Finding { severity: Severity::Alert, message: "alert".into() },
        ];
        assert_eq!(derive_status(&findings), HeartbeatStatus::Alert);
    }

    #[test]
    fn test_derive_status_empty_is_unstructured() {
        let findings: Vec<Finding> = vec![];
        assert_eq!(derive_status(&findings), HeartbeatStatus::Unstructured);
    }
}
```

- [ ] **Step 2: Implement finding extraction functions**

```rust
/// Extract structured findings from agent output.
///
/// Looks for lines starting with `PASS:`, `WARN:`, or `ALERT:`.
pub fn extract_findings(output: &str) -> Vec<Finding> { ... }

/// Derive overall status from a set of findings.
///
/// - All Pass -> Pass
/// - Any Warn (no Alert) -> Warn
/// - Any Alert -> Alert
/// - Empty findings -> Unstructured
pub fn derive_status(findings: &[Finding]) -> HeartbeatStatus { ... }
```

- [ ] **Step 3: Verify tests pass**

Run: `cargo test -p colony-exec -- heartbeat::runner`

### Task 6: Add colony-agent dependency to colony-exec

**Files:**
- Modify: `crates/colony-exec/Cargo.toml`

- [ ] **Step 1: Add dependency**

In `crates/colony-exec/Cargo.toml`, add under `[dependencies]`:

```toml
colony-agent = { path = "../colony-agent" }
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p colony-exec`

Note: This creates a dependency from colony-exec to colony-agent. Currently colony-agent depends on colony-exec (via `AgentConfig::apply_definition` which uses `colony_exec::special_files::AgentDefinition`). **Check for circular dependency.** If circular, the heartbeat scheduler will need to be invoked from colony-shell instead, passing an `Agent` handle down. In that case, `runner.rs` takes `Agent` as a parameter rather than constructing it.

Resolution if circular: Move the agent invocation into `colony-shell` and have `runner.rs` accept a trait or callback for "run agent with this prompt". The types, parser, storage, and finding extraction stay in `colony-exec`. Document whichever approach is used.

### Task 7: Implement per-tick runner

**Files:**
- Continue: `crates/colony-exec/src/heartbeat/runner.rs`

- [ ] **Step 1: Implement `build_tool_registry(mode: SecurityMode) -> ToolRegistry`**

```rust
use colony_agent::tool::{ToolRegistry, ToolClassification};
use colony_agent::tool::builtins::{FileRead, ShellExec, FileWrite, Search};

/// Build a ToolRegistry scoped to the given security mode.
///
/// Tool names match colony-agent's registry: file_read, shell_exec, search, file_write.
pub fn build_tool_registry(mode: SecurityMode) -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    // All modes get read-only tools
    registry.register_tool(Box::new(FileRead), ToolClassification::Programmatic);
    registry.register_tool(Box::new(ShellExec), ToolClassification::Programmatic);
    registry.register_tool(Box::new(Search), ToolClassification::Programmatic);

    match mode {
        SecurityMode::Active | SecurityMode::Full => {
            registry.register_tool(Box::new(FileWrite), ToolClassification::Programmatic);
        }
        SecurityMode::Notify => {}
    }

    // All tools auto-approved (no interactive approval in heartbeat)
    registry.set_auto_approve(vec!["*".to_string()]);
    registry
}
```

Note: In `notify` and `active` modes, `ShellExec` is registered with `Programmatic` classification (auto-approved, captures stdout). The agent can run commands but the tool itself enforces read-only behavior since it only returns stdout/stderr. In `full` mode the same tool is used but with no additional restrictions.

- [ ] **Step 2: Implement `build_agent_prompt`**

```rust
/// Build the system prompt for a heartbeat tick.
pub fn build_agent_prompt(
    checklist_content: &str,
    context_content: Option<&str>,
    cwd: &str,
    git_branch: Option<&str>,
) -> String {
    let mut prompt = String::from(
        "You are a heartbeat monitor. Execute each checklist item below, \
         report findings concisely. Do not infer tasks from prior runs.\n\n\
         Format each finding as:\n\
         PASS: <message>\n\
         WARN: <message>\n\
         ALERT: <message>\n\n"
    );

    prompt.push_str(&format!("Working directory: {}\n", cwd));
    if let Some(branch) = git_branch {
        prompt.push_str(&format!("Git branch: {}\n", branch));
    }
    prompt.push_str(&format!("Timestamp: {}\n\n", chrono::Utc::now().to_rfc3339()));

    if let Some(ctx) = context_content {
        prompt.push_str("## Project Context\n\n");
        prompt.push_str(ctx);
        prompt.push_str("\n\n");
    }

    prompt.push_str("## Checklist\n\n");
    prompt.push_str(checklist_content);
    prompt
}
```

- [ ] **Step 3: Implement `run_tick`**

This is the core per-tick execution function. It:
1. Reads HEARTBEAT.md from the discovered path
2. Parses frontmatter + checklist
3. Skips if body is empty
4. Builds the agent context
5. Creates a fresh Agent with custom ToolRegistry
6. Invokes `agent.process_full(prompt)` with timeout
7. Extracts findings from output
8. Builds and returns a `HeartbeatRun`

```rust
/// Execute a single heartbeat tick.
///
/// Returns `None` if the checklist is empty (skip).
pub async fn run_tick(
    checklist_path: &Path,
    context_content: Option<&str>,
) -> Result<Option<HeartbeatRun>, HeartbeatError> { ... }
```

The function should:
- Use `tokio::time::timeout(Duration::from_secs(config.timeout_secs), ...)` for timeout enforcement
- On timeout: return a `HeartbeatRun` with `status: HeartbeatStatus::Timeout`, empty findings
- On agent creation failure: return `HeartbeatRun` with `status: HeartbeatStatus::Error`
- On success: extract findings, derive status, return full `HeartbeatRun`

Define `HeartbeatError` in `types.rs`:
```rust
#[derive(Debug, thiserror::Error)]
pub enum HeartbeatError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("agent error: {0}")]
    Agent(String),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("no HEARTBEAT.md found")]
    NotFound,
}
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p colony-exec`

- [ ] **Step 5: Commit**

```bash
git add crates/colony-exec/src/heartbeat/runner.rs crates/colony-exec/Cargo.toml
git commit -m "feat(heartbeat): add per-tick runner with security-scoped tool registry"
```

---

## Chunk 5: HeartbeatScheduler and Builtin

### Task 8: Implement HeartbeatScheduler

**Files:**
- Modify: `crates/colony-exec/src/heartbeat/mod.rs`

- [ ] **Step 1: Implement the scheduler struct**

The `HeartbeatScheduler` manages the background tokio task:

```rust
use std::sync::Arc;
use parking_lot::Mutex;
use tokio::task::JoinHandle;

/// Background heartbeat scheduler state.
pub struct HeartbeatScheduler {
    /// Handle to the background task (None if not running).
    task_handle: Option<JoinHandle<()>>,
    /// Shared state for status queries.
    state: Arc<Mutex<SchedulerState>>,
    /// Cancellation signal.
    cancel: tokio::sync::watch::Sender<bool>,
}

struct SchedulerState {
    running: bool,
    checklist_path: PathBuf,
    interval_secs: u64,
    last_run: Option<chrono::DateTime<chrono::Utc>>,
    last_status: Option<HeartbeatStatus>,
    next_run: Option<chrono::DateTime<chrono::Utc>>,
    active_item_count: usize,
}
```

Methods:
- `start(checklist_path: PathBuf, tuple_space: TupleSpace) -> Result<(), HeartbeatError>` -- validates HEARTBEAT.md exists, parses initial config, spawns background task
- `stop()` -- sends cancel signal, awaits task join
- `run_once(checklist_path: &Path, tuple_space: &TupleSpace) -> Result<Option<HeartbeatRun>, HeartbeatError>` -- single tick (for `heartbeat run`)
- `status() -> Option<SchedulerStatus>` -- returns last run, next run, interval, item count
- `set_interval(secs: u64)` -- override interval without modifying HEARTBEAT.md
- `is_running() -> bool`

The background task loop:
```rust
loop {
    tokio::select! {
        _ = tokio::time::sleep(Duration::from_secs(interval)) => {
            // Re-read HEARTBEAT.md for config hot-reload
            // Run tick
            // Store result
            // Emit tuples
            // Prune old runs
        }
        _ = cancel_rx.changed() => {
            break;
        }
    }
}
```

- [ ] **Step 2: Implement tuple emission**

After each tick, emit tuples to the tuple space:

```rust
// heartbeat_complete tuple
tuple_space.write(Tuple::new(vec![
    Value::String("heartbeat_complete".into()),
    Value::String(checklist_path.display().to_string()),
    Value::String(run.id.clone()),
    Value::String(run.status.to_string()),
    Value::Integer(run.findings.len() as i64),
    Value::Integer(run.duration_ms as i64),
]));

// Per-finding tuples (if notify=true)
for finding in &run.findings {
    tuple_space.write(Tuple::new(vec![
        Value::String("heartbeat_finding".into()),
        Value::String(checklist_path.display().to_string()),
        Value::String(run.id.clone()),
        Value::String(format!("{:?}", finding.severity).to_lowercase()),
        Value::String(finding.message.clone()),
    ]));
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p colony-exec`

### Task 9: Implement heartbeat builtin

**Files:**
- Modify: `crates/colony-exec/src/builtins.rs`

- [ ] **Step 1: Add `"heartbeat"` to `is_builtin_base()`**

In `crates/colony-exec/src/builtins.rs`, add `"heartbeat"` to the `is_builtin_base()` match (around line 128, before the closing `)`):

```rust
            | "sidebar"
            | "heartbeat"
    )
```

- [ ] **Step 2: Add dispatch in `execute_builtin()`**

In the main match block (around line 268), add:

```rust
"heartbeat" => builtin_heartbeat(&args, tuple_space, session_id),
```

Note: The `execute_builtin` function is synchronous but `heartbeat start/run` need async. Two approaches:
1. Store a `HeartbeatScheduler` in a `OnceCell`/`Lazy` static (matches how flux stores state)
2. Use `tokio::runtime::Handle::current().block_on()` for async operations

Check how `flux` handles this pattern (it's also a builtin that manages background state). Follow the same approach.

- [ ] **Step 3: Implement `builtin_heartbeat()`**

```rust
fn builtin_heartbeat(
    args: &[String],
    tuple_space: &TupleSpace,
    session_id: &str,
) -> Result<BuiltinResult> {
    let Some(subcommand) = args.first() else {
        eprintln!("heartbeat: missing subcommand");
        eprintln!("Usage: heartbeat <start|stop|run|status|interval|log|render>");
        return Ok(BuiltinResult::Failure(1));
    };

    match subcommand.as_str() {
        "start" => { /* discover HEARTBEAT.md, validate provider, start scheduler */ }
        "stop" => { /* stop scheduler */ }
        "run" => { /* run single tick immediately */ }
        "status" => { /* print last run, next run, interval, item count */ }
        "interval" => {
            let Some(duration_str) = args.get(1) else {
                eprintln!("heartbeat interval: missing duration (e.g., 15m, 1h)");
                return Ok(BuiltinResult::Failure(1));
            };
            /* parse duration, set interval */
        }
        "log" => {
            /* if args.get(1) is a run_id, show full output for that run */
            /* else show recent run history table */
        }
        "render" => { /* render HTML dashboard to stdout */ }
        other => {
            eprintln!("heartbeat: unknown subcommand '{}'", other);
            return Ok(BuiltinResult::Failure(1));
        }
    }
}
```

The `start` subcommand should:
1. Discover HEARTBEAT.md via `SpecialFilesLoader::build_search_path()` (current dir -> parents -> ~/.colony/)
2. Validate that an LLM provider is available (attempt `ModelRouter::auto_detect()`)
3. Parse frontmatter for initial config
4. Create and start the `HeartbeatScheduler`
5. Print confirmation: `"heartbeat: started (interval: 30m, mode: notify, items: 5)"`

The `status` subcommand should print:
```
Heartbeat: running
  Checklist: /project/HEARTBEAT.md (5 active items)
  Interval: 30m
  Mode: notify
  Last run: 14:30 (PASS, 8.2s)
  Next run: 15:00
```

The `log` subcommand with no args should print a table:
```
Run ID     Time   Duration  Status  Findings
abc123..   14:30  8.2s      PASS    0 warn, 0 alert
def456..   14:00  12.1s     WARN    1 warn, 0 alert
```

The `log <run_id>` subcommand should print the full `raw_output` for that run.

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p colony-exec`

- [ ] **Step 5: Commit**

```bash
git add crates/colony-exec/src/heartbeat/mod.rs crates/colony-exec/src/builtins.rs
git commit -m "feat(heartbeat): add HeartbeatScheduler and heartbeat builtin"
```

---

## Chunk 6: Dashboard and Serve Integration

### Task 10: Implement HTML dashboard rendering

**Files:**
- Create: `crates/colony-exec/src/heartbeat/dashboard.rs`
- Modify: `crates/colony-exec/src/heartbeat/mod.rs`

- [ ] **Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::heartbeat::types::*;

    #[test]
    fn test_render_dashboard_contains_sections() {
        let checklist = "- [ ] Check tests\n- [x] Done\n";
        let runs = vec![HeartbeatRun {
            id: "run-1".into(),
            timestamp: 1710590400,
            duration_ms: 8200,
            status: HeartbeatStatus::Pass,
            findings: vec![Finding { severity: Severity::Pass, message: "Tests passing".into() }],
            raw_output: "PASS: Tests passing".into(),
            checklist_path: "/project/HEARTBEAT.md".into(),
            interval_secs: 1800,
        }];
        let html = render_dashboard(checklist, &runs, 1800, "notify");
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("Colony Heartbeat"));
        assert!(html.contains("Check tests"));
        assert!(html.contains("Tests passing"));
        assert!(html.contains("meta http-equiv=\"refresh\""));
    }

    #[test]
    fn test_render_finding_colors() {
        let html = render_finding(&Finding { severity: Severity::Pass, message: "ok".into() });
        assert!(html.contains("green") || html.contains("#2d7"));
        let html = render_finding(&Finding { severity: Severity::Warn, message: "warn".into() });
        assert!(html.contains("orange") || html.contains("#f90") || html.contains("yellow"));
        let html = render_finding(&Finding { severity: Severity::Alert, message: "bad".into() });
        assert!(html.contains("red") || html.contains("#e33"));
    }
}
```

- [ ] **Step 2: Implement dashboard rendering**

In `crates/colony-exec/src/heartbeat/dashboard.rs`:

```rust
//! HTML dashboard rendering for heartbeat serve integration.
//!
//! Generates a self-contained HTML page with inline CSS.
//! No JavaScript framework, no build step. Auto-refreshes via
//! `<meta http-equiv="refresh">` matching the heartbeat interval.

use super::types::{Finding, HeartbeatRun, HeartbeatStatus, Severity};
```

Functions:
- `render_dashboard(checklist: &str, runs: &[HeartbeatRun], interval_secs: u64, mode: &str) -> String` -- full HTML page
- `render_checklist_html(checklist: &str) -> String` -- converts markdown checkboxes to HTML `<li>` with checkbox styling
- `render_history_table(runs: &[HeartbeatRun]) -> String` -- reverse-chronological table with expandable raw output rows
- `render_findings_html(run: &HeartbeatRun) -> String` -- latest run findings with color coding
- `render_finding(finding: &Finding) -> String` -- single finding with severity color

The HTML should include:
- `<meta http-equiv="refresh" content="{interval_secs}">` for auto-refresh
- Inline CSS with dark theme (matches terminal aesthetic)
- Three sections: Current Checklist, Run History, Live Findings
- PASS in green, WARN in amber/yellow, ALERT in red
- Run history rows clickable/expandable with raw output in `<pre>` blocks
- Status badge colors matching severity

- [ ] **Step 3: Wire `heartbeat render` subcommand**

In the `builtin_heartbeat()` function, the `"render"` match arm should:
1. Open the `HeartbeatStore`
2. Read HEARTBEAT.md content
3. Query recent runs
4. Call `render_dashboard()`
5. Print HTML to stdout

```rust
"render" => {
    let store = HeartbeatStore::open(&HeartbeatStore::default_path())
        .map_err(|e| { eprintln!("heartbeat render: {}", e); })?;
    let checklist_path = discover_heartbeat_path()?;
    let checklist = std::fs::read_to_string(&checklist_path)
        .unwrap_or_default();
    let runs = store.recent_runs(20).unwrap_or_default();
    let (config, _) = parse_heartbeat(&checklist);
    let html = dashboard::render_dashboard(
        &checklist, &runs, config.interval_secs,
        &format!("{:?}", config.mode).to_lowercase(),
    );
    print!("{}", html);
    Ok(BuiltinResult::Success(0))
}
```

- [ ] **Step 4: Update mod.rs**

Add to `crates/colony-exec/src/heartbeat/mod.rs`:
```rust
pub mod dashboard;
```

- [ ] **Step 5: Document shell function and serve integration**

The `heartbeat start` subcommand should auto-register the serve endpoint when the serve builtin is detected. This is done by:
1. Defining a shell function: `heartbeat_dashboard() { heartbeat render; }`
2. Registering with serve: `serve endpoint /_heartbeat heartbeat_dashboard --method GET`

Since `heartbeat start` is a builtin and cannot directly define shell functions, it should emit a tuple `("heartbeat_register_dashboard", "/_heartbeat")` that the REPL can react to by defining the function and registering the endpoint.

Alternatively, `heartbeat start` can print instructions:
```
heartbeat: To enable web dashboard, run:
  heartbeat_dashboard() { heartbeat render; }
  serve endpoint /_heartbeat heartbeat_dashboard --method GET
```

- [ ] **Step 6: Verify it compiles**

Run: `cargo check -p colony-exec`

- [ ] **Step 7: Commit**

```bash
git add crates/colony-exec/src/heartbeat/dashboard.rs crates/colony-exec/src/heartbeat/mod.rs
git commit -m "feat(heartbeat): add HTML dashboard and serve integration"
```

---

## Chunk 7: Panel Binary

### Task 11: Create colony-heartbeat-bin panel binary

**Files:**
- Create: `crates/colony-heartbeat-bin/Cargo.toml`
- Create: `crates/colony-heartbeat-bin/src/main.rs`
- Modify: `Cargo.toml` (workspace root)
- Modify: `crates/colony-shell/Cargo.toml`

- [ ] **Step 1: Create Cargo.toml**

```toml
[package]
name = "colony-heartbeat-bin"
version.workspace = true
edition.workspace = true

[dependencies]
colony-panel = { path = "../colony-panel" }
serde = { workspace = true }
serde_json = { workspace = true }
chrono = { workspace = true }
```

- [ ] **Step 2: Add to workspace root Cargo.toml**

In `/Cargo.toml`, add `"crates/colony-heartbeat-bin"` to `[workspace.members]` (after `colony-session-bin`).

- [ ] **Step 3: Add binary entry to colony-shell Cargo.toml**

In `crates/colony-shell/Cargo.toml`, add:

```toml
[[bin]]
name = "heartbeat-panel"
path = "../colony-heartbeat-bin/src/main.rs"
```

- [ ] **Step 4: Implement panel binary**

In `crates/colony-heartbeat-bin/src/main.rs`:

Follow the pattern from `colony-history-bin/src/main.rs`:
1. Emit `PanelConfig` with subscription to `"heartbeat_complete"` tuple
2. Initial render showing "Heartbeat: waiting for data..."
3. Event loop:
   - `InputEvent::Tuple` with key `"heartbeat_complete"` -> update state, re-render
   - `InputEvent::Key` `"Enter"` -> emit tuple `("heartbeat_run_now", "")` (scheduler listens for this)
   - `InputEvent::Key` `"l"` -> toggle log view
   - `InputEvent::Resize` -> re-render
   - `InputEvent::Shutdown` -> exit

Display format:
```
Heartbeat :30m  Last: 14:30 (PASS)  Next: 15:00
  PASS  Tests passing (147/147)
  PASS  No new TODOs
  WARN  Dev server slow (890ms)
  PASS  Disk usage normal
```

Use `FrameLine` with styled text:
- PASS findings: green foreground
- WARN findings: yellow foreground
- ALERT findings: red foreground
- Status line: dim for labels, bold for values

- [ ] **Step 5: Verify it compiles**

Run: `cargo check -p colony-heartbeat-bin`

- [ ] **Step 6: Commit**

```bash
git add crates/colony-heartbeat-bin/ Cargo.toml crates/colony-shell/Cargo.toml
git commit -m "feat(heartbeat): add colony-heartbeat-bin panel binary"
```

---

## Chunk 8: Integration Testing and Polish

### Task 12: Integration test with mocked agent

**Files:**
- Add: `crates/colony-exec/src/heartbeat/mod.rs` (integration test at bottom)

- [ ] **Step 1: Write integration test**

Since `Agent::new()` requires an LLM provider API key, integration tests should use a mock or test double. Two approaches:

**Option A:** Test the pipeline without the agent -- test parser -> runner context building -> finding extraction -> storage -> tuple emission as a unit, passing mock agent output to `extract_findings` and `derive_status`.

**Option B:** If colony-agent supports test providers, use that.

Recommended: Option A. Test each layer independently, mock the agent boundary.

```rust
#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_full_pipeline_without_agent() {
        // 1. Parse a HEARTBEAT.md
        let content = "---\ninterval: 5m\nmode: notify\n---\n\n# Heartbeat\n\n- [ ] Check tests\n";
        let (config, items) = parser::parse_heartbeat(content);
        assert_eq!(config.interval_secs, 300);
        assert_eq!(items.len(), 1);

        // 2. Build agent prompt
        let prompt = runner::build_agent_prompt(content, None, "/project", Some("main"));
        assert!(prompt.contains("heartbeat monitor"));
        assert!(prompt.contains("Check tests"));

        // 3. Simulate agent output
        let agent_output = "PASS: Tests passing (42/42)\nWARN: 1 new TODO in main.rs\n";
        let findings = runner::extract_findings(agent_output);
        assert_eq!(findings.len(), 2);
        let status = runner::derive_status(&findings);
        assert_eq!(status, HeartbeatStatus::Warn);

        // 4. Store run
        let store = storage::HeartbeatStore::open_in_memory().unwrap();
        let run = HeartbeatRun {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now().timestamp(),
            duration_ms: 5000,
            status,
            findings,
            raw_output: agent_output.to_string(),
            checklist_path: "/project/HEARTBEAT.md".to_string(),
            interval_secs: 300,
        };
        store.insert(&run).unwrap();

        // 5. Query back
        let runs = store.recent_runs(10).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].status, HeartbeatStatus::Warn);

        // 6. Render dashboard
        let html = dashboard::render_dashboard(content, &runs, 300, "notify");
        assert!(html.contains("Check tests"));
        assert!(html.contains("Tests passing"));
    }
}
```

- [ ] **Step 2: Verify all tests pass**

Run: `cargo test -p colony-exec -- heartbeat`

### Task 13: Validate full workspace build

- [ ] **Step 1: Full workspace check**

```bash
cargo check
```

Fix any cross-crate breakage from the F030 type changes.

- [ ] **Step 2: Run all tests**

```bash
cargo test
```

- [ ] **Step 3: Run clippy**

```bash
cargo clippy -p colony-exec -- -W clippy::all -W clippy::pedantic
```

Fix any warnings.

- [ ] **Step 4: Final commit**

```bash
git add -A
git commit -m "feat(heartbeat): F080 heartbeat scheduler complete"
```

---

## Dependency Graph

```
colony-shell
  └── colony-exec
        ├── colony-agent (NEW dependency for heartbeat runner)
        │     └── colony-exec (AgentConfig::apply_definition)  ← CIRCULAR?
        ├── colony-tuple
        ├── colony-reflection (DB path only)
        └── rusqlite

colony-heartbeat-bin
  └── colony-panel
```

**Circular dependency risk:** `colony-agent` already depends on `colony-exec` (for `AgentDefinition` in `AgentConfig::apply_definition`). Adding `colony-exec -> colony-agent` creates a cycle. Resolution options:

1. **Extract shared types** -- Move `AgentDefinition` to a shared crate (e.g., `colony-protocol` or `colony-config`) so both can depend on it without cycling.
2. **Invert the dependency** -- Have the heartbeat runner live in `colony-shell` (which already depends on both). The heartbeat module in `colony-exec` handles parsing, storage, types, and dashboard. The agent invocation happens in `colony-shell`.
3. **Trait-based abstraction** -- Define a `trait HeartbeatRunner` in `colony-exec` and implement it in `colony-shell` using `colony-agent`.

**Recommended:** Option 2. Keep the heartbeat module in `colony-exec` for types, parser, storage, and dashboard. Move `run_tick()` (the function that creates an `Agent`) to `colony-shell` where both `colony-agent` and `colony-exec` are available. The `HeartbeatScheduler` in `colony-exec` accepts a callback/closure for the agent invocation:

```rust
// In colony-exec: HeartbeatScheduler accepts a runner function
pub struct HeartbeatScheduler {
    runner: Box<dyn Fn(&Path, &HeartbeatConfig) -> Pin<Box<dyn Future<Output = Result<String, HeartbeatError>>>> + Send + Sync>,
    // ...
}

// In colony-shell: provide the runner implementation
let scheduler = HeartbeatScheduler::new(Box::new(|path, config| {
    Box::pin(async move {
        let mut agent = Agent::with_config(/* ... */).await?;
        agent.process_full(prompt).await.map_err(|e| HeartbeatError::Agent(e.to_string()))
    })
}));
```

This avoids the circular dependency entirely. If this approach is taken, Task 6 (adding colony-agent dep to colony-exec) is skipped, and Task 7 (runner.rs) only contains finding extraction + prompt building (no Agent import). The agent wiring happens in a new file in `colony-shell`.

---

## Summary of Build/Test Commands

| Command | Purpose |
|---------|---------|
| `cargo check -p colony-exec` | Fast type-check after each task |
| `cargo test -p colony-exec -- heartbeat` | Run all heartbeat tests |
| `cargo test -p colony-exec -- heartbeat::parser` | Parser tests only |
| `cargo test -p colony-exec -- heartbeat::storage` | Storage tests only |
| `cargo test -p colony-exec -- heartbeat::runner` | Runner/finding tests only |
| `cargo test -p colony-exec -- special_files` | F030 integration tests |
| `cargo check -p colony-heartbeat-bin` | Panel binary type-check |
| `cargo check` | Full workspace check |
| `cargo test` | Full workspace tests |
| `cargo clippy -p colony-exec` | Lint check |
