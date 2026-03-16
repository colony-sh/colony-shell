//! Context channel (fd 3) sidebar panel
//!
//! Displays stdcontext entries written to file descriptor 3.
//! Entries appear most recent first.

use crate::component::{
    Color, ComponentLine, SidebarComponent, SidebarOutput, Style, StyledSpan, TuplePattern,
};
use crate::context::ComponentContext;
use colony_tuple::Value;

/// Maximum number of context entries retained in the panel buffer
const MAX_ENTRIES: usize = 100;

/// A single context entry from fd 3
#[derive(Debug, Clone)]
pub struct ContextEntry {
    pub text: String,
    pub timestamp: std::time::Instant,
}

/// Sidebar panel displaying stdcontext (fd 3) channel data
pub struct ContextPanel {
    /// Buffer of context entries, newest first
    entries: Vec<ContextEntry>,
}

impl ContextPanel {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Push a new context entry into the panel buffer.
    ///
    /// Entries exceeding `MAX_ENTRIES` are dropped from the tail (oldest).
    pub fn push_entry(&mut self, text: String) {
        self.entries.insert(
            0,
            ContextEntry {
                text,
                timestamp: std::time::Instant::now(),
            },
        );
        self.entries.truncate(MAX_ENTRIES);
    }

    /// Clear all context entries
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
}

impl Default for ContextPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl SidebarComponent for ContextPanel {
    fn id(&self) -> &str {
        "context"
    }

    fn title(&self) -> &str {
        "Context"
    }

    fn default_width(&self) -> u16 {
        30
    }

    fn subscriptions(&self) -> Vec<TuplePattern> {
        vec![vec![
            Value::Str("channel_write".to_string()),
            Value::Str("3".to_string()),
            Value::Wildcard,
        ]]
    }

    fn render_block(&self, _ctx: &ComponentContext) -> SidebarOutput {
        let mut lines = Vec::new();

        if self.entries.is_empty() {
            lines.push(ComponentLine::new(vec![StyledSpan::new(
                "No context entries",
                Style::default().dim().italic(),
            )]));
            lines.push(ComponentLine::plain(""));
            lines.push(ComponentLine::new(vec![StyledSpan::new(
                "Write to fd 3 to add context",
                Style::default().dim(),
            )]));
            return SidebarOutput::visible(lines);
        }

        // Entry count header
        lines.push(ComponentLine::new(vec![StyledSpan::new(
            format!("{} entries", self.entries.len()),
            Style::default().fg(Color::Cyan).bold(),
        )]));
        lines.push(ComponentLine::plain(""));

        // Render entries newest first
        for (i, entry) in self.entries.iter().enumerate() {
            let age = entry.timestamp.elapsed();
            let age_str = format_age(age);

            // Index + age metadata
            lines.push(ComponentLine::new(vec![
                StyledSpan::new(
                    format!("[{}]", i),
                    Style::default().fg(Color::BrightBlack),
                ),
                StyledSpan::new(
                    format!(" {}", age_str),
                    Style::default().fg(Color::BrightBlack).dim(),
                ),
            ]));

            // Entry text, truncated to avoid overflow
            let display_text = truncate_text(&entry.text, 120);
            lines.push(
                ComponentLine::new(vec![StyledSpan::new(
                    display_text,
                    Style::default().fg(Color::White),
                )])
                .with_indent(1),
            );
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
            ..Default::default()
        }
    }

    #[test]
    fn test_context_panel_empty() {
        let panel = ContextPanel::new();
        let ctx = test_context();
        let output = panel.render_block(&ctx);
        assert!(output.visible);
        assert!(!output.lines.is_empty());
        // Should show placeholder text
        assert!(output.lines[0].spans[0].text.contains("No context"));
    }

    #[test]
    fn test_context_panel_with_entries() {
        let mut panel = ContextPanel::new();
        panel.push_entry("first context item".to_string());
        panel.push_entry("second context item".to_string());

        let ctx = test_context();
        let output = panel.render_block(&ctx);
        assert!(output.visible);
        // Header + blank + 2 entries (each 2 lines: index+age, text)
        assert!(output.lines.len() >= 4);
    }

    #[test]
    fn test_context_panel_ordering() {
        let mut panel = ContextPanel::new();
        panel.push_entry("older".to_string());
        panel.push_entry("newer".to_string());

        // Newest should be first in internal buffer
        assert_eq!(panel.entries[0].text, "newer");
        assert_eq!(panel.entries[1].text, "older");
    }

    #[test]
    fn test_context_panel_max_entries() {
        let mut panel = ContextPanel::new();
        for i in 0..150 {
            panel.push_entry(format!("entry {}", i));
        }
        assert_eq!(panel.len(), MAX_ENTRIES);
    }

    #[test]
    fn test_context_panel_clear() {
        let mut panel = ContextPanel::new();
        panel.push_entry("entry".to_string());
        assert!(!panel.is_empty());
        panel.clear();
        assert!(panel.is_empty());
    }

    #[test]
    fn test_truncate_text() {
        assert_eq!(truncate_text("short", 10), "short");
        assert_eq!(truncate_text("a very long string", 10), "a very...");
    }

    #[test]
    fn test_format_age() {
        assert_eq!(format_age(std::time::Duration::from_secs(5)), "5s ago");
        assert_eq!(format_age(std::time::Duration::from_secs(120)), "2m ago");
        assert_eq!(format_age(std::time::Duration::from_secs(7200)), "2h ago");
    }

    #[test]
    fn test_subscriptions() {
        let panel = ContextPanel::new();
        let subs = panel.subscriptions();
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].len(), 3);
        assert_eq!(subs[0][0], Value::Str("channel_write".to_string()));
        assert_eq!(subs[0][1], Value::Str("3".to_string()));
    }
}
