//! Tool call channel (fd 5) sidebar panel
//!
//! Displays tool call request/result pairs written to file descriptor 5.
//! Pending requests are highlighted in yellow.

use crate::component::{
    Color, ComponentLine, SidebarComponent, SidebarOutput, Style, StyledSpan, TuplePattern,
};
use crate::context::ComponentContext;
use colony_tuple::Value;

/// Maximum number of tool call entries retained in the panel buffer
const MAX_ENTRIES: usize = 50;

/// Status of a tool call
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolCallStatus {
    /// Request sent, awaiting result
    Pending,
    /// Result received successfully
    Success,
    /// Result received with error
    Error,
}

/// A tool call request/result pair
#[derive(Debug, Clone)]
pub struct ToolCallEntry {
    /// Name of the tool being called
    pub tool_name: String,
    /// Request summary (truncated arguments)
    pub request_summary: String,
    /// Result summary (if completed)
    pub result_summary: Option<String>,
    /// Call status
    pub status: ToolCallStatus,
    /// Timestamp of the request
    pub timestamp: std::time::Instant,
}

/// Sidebar panel displaying tool call/result pairs (fd 5)
pub struct ToolPanel {
    /// Buffer of tool call entries, newest first
    entries: Vec<ToolCallEntry>,
}

impl ToolPanel {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Add a new pending tool call request.
    ///
    /// Entries exceeding `MAX_ENTRIES` are dropped from the tail (oldest).
    pub fn push_request(&mut self, tool_name: String, request_summary: String) {
        self.entries.insert(
            0,
            ToolCallEntry {
                tool_name,
                request_summary,
                result_summary: None,
                status: ToolCallStatus::Pending,
                timestamp: std::time::Instant::now(),
            },
        );
        self.entries.truncate(MAX_ENTRIES);
    }

    /// Complete a pending tool call with a result.
    ///
    /// Matches the most recent pending entry with the given tool name.
    pub fn complete_call(&mut self, tool_name: &str, result: String, is_error: bool) {
        let entry = self
            .entries
            .iter_mut()
            .find(|e| e.tool_name == tool_name && e.status == ToolCallStatus::Pending);

        if let Some(entry) = entry {
            entry.result_summary = Some(result);
            entry.status = if is_error {
                ToolCallStatus::Error
            } else {
                ToolCallStatus::Success
            };
        }
    }

    /// Clear all entries
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Number of entries currently stored
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the entry buffer is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Count of currently pending calls
    pub fn pending_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| e.status == ToolCallStatus::Pending)
            .count()
    }
}

impl Default for ToolPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl SidebarComponent for ToolPanel {
    fn id(&self) -> &str {
        "tools"
    }

    fn title(&self) -> &str {
        "Tools"
    }

    fn default_width(&self) -> u16 {
        30
    }

    fn subscriptions(&self) -> Vec<TuplePattern> {
        vec![vec![
            Value::Str("channel_write".to_string()),
            Value::Str("5".to_string()),
            Value::Wildcard,
        ]]
    }

    fn render_block(&self, _ctx: &ComponentContext) -> SidebarOutput {
        let mut lines = Vec::new();

        if self.entries.is_empty() {
            lines.push(ComponentLine::new(vec![StyledSpan::new(
                "No tool calls",
                Style::default().dim().italic(),
            )]));
            lines.push(ComponentLine::plain(""));
            lines.push(ComponentLine::new(vec![StyledSpan::new(
                "Tool calls appear on fd 5",
                Style::default().dim(),
            )]));
            return SidebarOutput::visible(lines);
        }

        // Summary header
        let pending = self.pending_count();
        let total = self.entries.len();
        let mut header_spans = vec![StyledSpan::new(
            format!("{} calls", total),
            Style::default().fg(Color::Cyan).bold(),
        )];
        if pending > 0 {
            header_spans.push(StyledSpan::new(
                format!(" ({} pending)", pending),
                Style::default().fg(Color::Yellow).bold(),
            ));
        }
        lines.push(ComponentLine::new(header_spans));
        lines.push(ComponentLine::plain(""));

        // Render entries newest first
        for entry in &self.entries {
            let (status_icon, name_style) = match entry.status {
                ToolCallStatus::Pending => (
                    "\u{25cb}",
                    Style::default().fg(Color::Yellow).bold(),
                ),
                ToolCallStatus::Success => (
                    "\u{25cf}",
                    Style::default().fg(Color::Green),
                ),
                ToolCallStatus::Error => (
                    "\u{25cf}",
                    Style::default().fg(Color::Red),
                ),
            };

            let age = format_age(entry.timestamp.elapsed());

            // Tool name with status icon
            lines.push(
                ComponentLine::new(vec![
                    StyledSpan::new(
                        format!("{} ", status_icon),
                        match entry.status {
                            ToolCallStatus::Pending => Style::default().fg(Color::Yellow),
                            ToolCallStatus::Success => Style::default().fg(Color::Green),
                            ToolCallStatus::Error => Style::default().fg(Color::Red),
                        },
                    ),
                    StyledSpan::new(entry.tool_name.clone(), name_style),
                    StyledSpan::new(
                        format!(" {}", age),
                        Style::default().fg(Color::BrightBlack).dim(),
                    ),
                ]),
            );

            // Request summary
            let req_display = truncate_text(&entry.request_summary, 60);
            lines.push(
                ComponentLine::new(vec![
                    StyledSpan::new("req: ", Style::default().dim()),
                    StyledSpan::new(req_display, Style::default().fg(Color::White)),
                ])
                .with_indent(1),
            );

            // Result summary (if present)
            match (&entry.status, &entry.result_summary) {
                (ToolCallStatus::Pending, _) => {
                    lines.push(
                        ComponentLine::new(vec![StyledSpan::new(
                            "awaiting result...",
                            Style::default().fg(Color::Yellow).italic(),
                        )])
                        .with_indent(1),
                    );
                }
                (ToolCallStatus::Success, Some(result)) => {
                    let res_display = truncate_text(result, 60);
                    lines.push(
                        ComponentLine::new(vec![
                            StyledSpan::new("res: ", Style::default().dim()),
                            StyledSpan::new(res_display, Style::default().fg(Color::Green)),
                        ])
                        .with_indent(1),
                    );
                }
                (ToolCallStatus::Error, Some(result)) => {
                    let res_display = truncate_text(result, 60);
                    lines.push(
                        ComponentLine::new(vec![
                            StyledSpan::new("err: ", Style::default().fg(Color::Red)),
                            StyledSpan::new(
                                res_display,
                                Style::default().fg(Color::Red).italic(),
                            ),
                        ])
                        .with_indent(1),
                    );
                }
                _ => {}
            }

            // Blank separator
            lines.push(ComponentLine::plain(""));
        }

        SidebarOutput::visible(lines)
    }
}

/// Format an elapsed duration as a human-readable age string
fn format_age(duration: std::time::Duration) -> String {
    let secs = duration.as_secs();
    if secs < 60 {
        format!("{}s ago", secs)
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else {
        format!("{}h ago", secs / 3600)
    }
}

/// Truncate text to a maximum character length, appending "..." when truncated
fn truncate_text(text: &str, max_chars: usize) -> String {
    if text.len() <= max_chars {
        text.to_string()
    } else {
        let mut truncated: String = text.chars().take(max_chars.saturating_sub(3)).collect();
        truncated.push_str("...");
        truncated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_context() -> ComponentContext {
        ComponentContext {
            cwd: std::path::PathBuf::from("/"),
            env: std::collections::HashMap::new(),
            last_exit_code: 0,
            running_jobs: 0,
            stopped_jobs: 0,
            model: None,
            last_command_duration: None,
            hostname: None,
        }
    }

    #[test]
    fn test_tool_panel_empty() {
        let panel = ToolPanel::new();
        let ctx = test_context();
        let output = panel.render_block(&ctx);
        assert!(output.visible);
        assert!(!output.lines.is_empty());
        assert!(output.lines[0].spans[0].text.contains("No tool calls"));
    }

    #[test]
    fn test_tool_panel_pending_request() {
        let mut panel = ToolPanel::new();
        panel.push_request("read_file".to_string(), "/src/main.rs".to_string());

        let ctx = test_context();
        let output = panel.render_block(&ctx);
        assert!(output.visible);
        assert_eq!(panel.pending_count(), 1);

        // Should show pending indicator
        let has_pending = output.lines.iter().any(|l| {
            l.spans
                .iter()
                .any(|s| s.text.contains("awaiting"))
        });
        assert!(has_pending);
    }

    #[test]
    fn test_tool_panel_completed_request() {
        let mut panel = ToolPanel::new();
        panel.push_request("read_file".to_string(), "/src/main.rs".to_string());
        panel.complete_call("read_file", "fn main() {}".to_string(), false);

        assert_eq!(panel.pending_count(), 0);
        assert_eq!(panel.entries[0].status, ToolCallStatus::Success);
    }

    #[test]
    fn test_tool_panel_error_result() {
        let mut panel = ToolPanel::new();
        panel.push_request("read_file".to_string(), "/nonexistent".to_string());
        panel.complete_call("read_file", "file not found".to_string(), true);

        assert_eq!(panel.entries[0].status, ToolCallStatus::Error);
    }

    #[test]
    fn test_tool_panel_max_entries() {
        let mut panel = ToolPanel::new();
        for i in 0..80 {
            panel.push_request(format!("tool_{}", i), format!("args {}", i));
        }
        assert_eq!(panel.len(), MAX_ENTRIES);
    }

    #[test]
    fn test_tool_panel_clear() {
        let mut panel = ToolPanel::new();
        panel.push_request("test".to_string(), "args".to_string());
        assert!(!panel.is_empty());
        panel.clear();
        assert!(panel.is_empty());
    }

    #[test]
    fn test_tool_panel_complete_matches_correct_entry() {
        let mut panel = ToolPanel::new();
        panel.push_request("tool_a".to_string(), "call 1".to_string());
        panel.push_request("tool_b".to_string(), "call 2".to_string());

        // Complete tool_a -- it should match the pending tool_a entry
        panel.complete_call("tool_a", "result".to_string(), false);

        // tool_b (index 0, newest) should still be pending
        assert_eq!(panel.entries[0].status, ToolCallStatus::Pending);
        // tool_a (index 1, older) should be complete
        assert_eq!(panel.entries[1].status, ToolCallStatus::Success);
    }

    #[test]
    fn test_subscriptions() {
        let panel = ToolPanel::new();
        let subs = panel.subscriptions();
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0][0], Value::Str("channel_write".to_string()));
        assert_eq!(subs[0][1], Value::Str("5".to_string()));
    }
}
