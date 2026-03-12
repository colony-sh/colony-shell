//! Parser and watcher for HEARTBEAT.md files
//!
//! HEARTBEAT.md is written by external agents and watched by the shell.
//! The HeartbeatParser converts markdown into `HeartbeatEntry` structs.
//! The HeartbeatWatcher uses the `notify` crate to detect file changes.

use std::path::{Path, PathBuf};
use std::sync::mpsc;

use notify::{Event, EventKind, RecursiveMode, Watcher};

use super::types::{HeartbeatEntry, HeartbeatStatus};

pub struct HeartbeatParser;

impl HeartbeatParser {
    /// Parse HEARTBEAT.md content into heartbeat entries.
    ///
    /// Format: `## agent: <name>` starts an entry, followed by `- key: value` lines.
    pub fn parse(content: &str) -> Vec<HeartbeatEntry> {
        let mut entries = Vec::new();
        let mut current_name: Option<String> = None;
        let mut fields: Vec<(String, String)> = Vec::new();

        for line in content.lines() {
            // New agent heartbeat entry
            if let Some(heading) = line.strip_prefix("## agent:") {
                // Finalize previous entry
                if let Some(name) = current_name.take() {
                    if let Some(entry) = build_entry(name, &fields) {
                        entries.push(entry);
                    }
                }
                current_name = Some(heading.trim().to_string());
                fields.clear();
                continue;
            }

            // Skip non-agent ## headings and preamble
            if line.starts_with("## ") || line.starts_with("# ") || line.trim() == "---" {
                continue;
            }

            if current_name.is_some() {
                if let Some(item) = line.strip_prefix("- ") {
                    let item = item.trim();
                    if let Some((key, value)) = item.split_once(':') {
                        fields.push((key.trim().to_string(), value.trim().to_string()));
                    }
                }
            }
        }

        // Finalize last entry
        if let Some(name) = current_name.take() {
            if let Some(entry) = build_entry(name, &fields) {
                entries.push(entry);
            }
        }

        entries
    }
}

fn build_entry(agent_name: String, fields: &[(String, String)]) -> Option<HeartbeatEntry> {
    let get = |key: &str| -> Option<String> {
        fields
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
    };

    // Required: status, updated
    let status_str = get("status")?;
    let updated = get("updated")?;

    let status = HeartbeatStatus::from_str_lossy(&status_str);

    let progress = get("progress").and_then(|p| p.trim_end_matches('%').trim().parse::<u8>().ok());

    let pid = get("pid").and_then(|p| p.parse::<u32>().ok());

    Some(HeartbeatEntry {
        agent_name,
        status,
        task: get("task"),
        progress,
        started: get("started"),
        updated,
        pid,
        last_task: get("last_task"),
        error: get("error"),
    })
}

/// Watches HEARTBEAT.md for filesystem changes using the `notify` crate.
///
/// The watcher is non-blocking: call `poll()` to check for updates.
pub struct HeartbeatWatcher {
    _watcher: notify::RecommendedWatcher,
    rx: mpsc::Receiver<notify::Result<Event>>,
    heartbeat_path: PathBuf,
}

impl HeartbeatWatcher {
    /// Create a new HeartbeatWatcher for the given directory.
    ///
    /// Watches for HEARTBEAT.md changes. If the file doesn't exist yet,
    /// watches the parent directory for file creation.
    pub fn new(dir: &Path) -> Result<Self, notify::Error> {
        let heartbeat_path = dir.join("HEARTBEAT.md");
        let (tx, rx) = mpsc::channel();

        let mut watcher = notify::recommended_watcher(move |res| {
            let _ = tx.send(res);
        })?;

        // Watch the directory (non-recursive) to catch file creation and modification
        watcher.watch(dir, RecursiveMode::NonRecursive)?;

        Ok(Self {
            _watcher: watcher,
            rx,
            heartbeat_path,
        })
    }

    /// Non-blocking check for heartbeat updates.
    ///
    /// Returns parsed entries if the HEARTBEAT.md file was modified or created.
    pub fn poll(&self) -> Option<Vec<HeartbeatEntry>> {
        let mut had_modify = false;

        // Drain all pending events
        while let Ok(Ok(event)) = self.rx.try_recv() {
            if matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_))
                && event.paths.iter().any(|p| p.ends_with("HEARTBEAT.md")) {
                    had_modify = true;
                }
        }

        if had_modify {
            self.read_and_parse()
        } else {
            None
        }
    }

    fn read_and_parse(&self) -> Option<Vec<HeartbeatEntry>> {
        let content = std::fs::read_to_string(&self.heartbeat_path).ok()?;
        Some(HeartbeatParser::parse(&content))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_single_heartbeat() {
        let content = r#"# HEARTBEAT.md

Agent heartbeat status file.

---

## agent: code-reviewer
- status: working
- task: Reviewing src/main.rs
- progress: 45%
- started: 2025-01-15T14:30:00Z
- updated: 2025-01-15T14:32:15Z
- pid: 12345
"#;
        let entries = HeartbeatParser::parse(content);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].agent_name, "code-reviewer");
        assert_eq!(entries[0].status, HeartbeatStatus::Working);
        assert_eq!(entries[0].task, Some("Reviewing src/main.rs".to_string()));
        assert_eq!(entries[0].progress, Some(45));
        assert_eq!(entries[0].pid, Some(12345));
        assert_eq!(entries[0].updated, "2025-01-15T14:32:15Z");
    }

    #[test]
    fn test_parse_multiple_heartbeats() {
        let content = r#"# HEARTBEAT.md

---

## agent: worker-1
- status: working
- task: Processing data
- progress: 75%
- updated: 2025-01-15T14:30:00Z

## agent: worker-2
- status: idle
- last_task: Generated tests
- updated: 2025-01-15T14:25:00Z
"#;
        let entries = HeartbeatParser::parse(content);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].agent_name, "worker-1");
        assert_eq!(entries[0].status, HeartbeatStatus::Working);
        assert_eq!(entries[0].progress, Some(75));
        assert_eq!(entries[1].agent_name, "worker-2");
        assert_eq!(entries[1].status, HeartbeatStatus::Idle);
        assert_eq!(entries[1].last_task, Some("Generated tests".to_string()));
    }

    #[test]
    fn test_parse_heartbeat_with_error() {
        let content = r#"# HEARTBEAT.md

---

## agent: failing-agent
- status: error
- error: Connection timeout
- updated: 2025-01-15T14:30:00Z
"#;
        let entries = HeartbeatParser::parse(content);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].status, HeartbeatStatus::Error);
        assert_eq!(entries[0].error, Some("Connection timeout".to_string()));
    }

    #[test]
    fn test_parse_empty_heartbeat() {
        let content = "# HEARTBEAT.md\n\nNo agents running.\n";
        let entries = HeartbeatParser::parse(content);
        assert!(entries.is_empty());
    }

    #[test]
    fn test_missing_required_fields_skipped() {
        let content = r#"# HEARTBEAT.md

---

## agent: incomplete
- task: Something
"#;
        // Missing status and updated -- should be skipped
        let entries = HeartbeatParser::parse(content);
        assert!(entries.is_empty());
    }

    #[test]
    fn test_unknown_status_becomes_unknown() {
        let content = r#"# HEARTBEAT.md

---

## agent: test
- status: banana
- updated: 2025-01-15T14:30:00Z
"#;
        let entries = HeartbeatParser::parse(content);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].status, HeartbeatStatus::Unknown);
    }

    #[test]
    fn test_all_status_values() {
        assert_eq!(
            HeartbeatStatus::from_str_lossy("idle"),
            HeartbeatStatus::Idle
        );
        assert_eq!(
            HeartbeatStatus::from_str_lossy("working"),
            HeartbeatStatus::Working
        );
        assert_eq!(
            HeartbeatStatus::from_str_lossy("thinking"),
            HeartbeatStatus::Thinking
        );
        assert_eq!(
            HeartbeatStatus::from_str_lossy("streaming"),
            HeartbeatStatus::Streaming
        );
        assert_eq!(
            HeartbeatStatus::from_str_lossy("error"),
            HeartbeatStatus::Error
        );
        assert_eq!(
            HeartbeatStatus::from_str_lossy("done"),
            HeartbeatStatus::Done
        );
        assert_eq!(
            HeartbeatStatus::from_str_lossy("garbage"),
            HeartbeatStatus::Unknown
        );
    }
}
