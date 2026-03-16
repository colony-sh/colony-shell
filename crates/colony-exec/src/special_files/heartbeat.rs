//! Heartbeat file discovery for F030 special files.
//!
//! The checklist parser lives in `crate::heartbeat::parser` (F080).
//! This module re-exports it for F030 integration and provides
//! backwards compatibility for the special files system.
//!
//! The old agent-status parser (`HeartbeatParser` with `## agent: <name>`
//! headings) and `HeartbeatWatcher` have been removed. The F080 heartbeat
//! scheduler uses a timer-based approach instead of filesystem watching.

pub use crate::heartbeat::parser::{body_is_empty, parse_heartbeat};
pub use crate::heartbeat::types::{ChecklistItem, HeartbeatConfig};
