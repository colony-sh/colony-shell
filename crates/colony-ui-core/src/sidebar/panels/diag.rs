//! Diagnostics channel (fd 6) sidebar panel
//!
//! Displays diagnostic entries written to file descriptor 6.
//! Entries are colored by severity level.

use crate::component::{
    Color, ComponentLine, SidebarComponent, SidebarOutput, Style, StyledSpan, TuplePattern,
};
use crate::context::ComponentContext;
use colony_tuple::Value;

/// Maximum number of diagnostic entries retained in the panel buffer
const MAX_ENTRIES: usize = 200;

/// Severity level for a diagnostic entry
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DiagSeverity {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl DiagSeverity {
    /// Short label for display
    fn label(&self) -> &'static str {
        match self {
            Self::Trace => "TRACE",
            Self::Debug => "DEBUG",
            Self::Info => " INFO",
            Self::Warn => " WARN",
            Self::Error => "ERROR",
        }
    }

    /// Style for the severity label badge
    fn badge_style(&self) -> Style {
        match self {
            Self::Trace => Style::default().fg(Color::BrightBlack).dim(),
            Self::Debug => Style::default().fg(Color::White),
            Self::Info => Style::default().fg(Color::Cyan).bold(),
            Self::Warn => Style::default().fg(Color::Yellow).bold(),
            Self::Error => Style::default().fg(Color::Red).bold(),
        }
    }

    /// Style for the message text
    fn text_style(&self) -> Style {
        match self {
            Self::Trace => Style::default().dim(),
            Self::Debug => Style::default().fg(Color::BrightBlack),
            Self::Info => Style::default().fg(Color::White),
            Self::Warn => Style::default().fg(Color::Yellow),
            Self::Error => Style::default().fg(Color::Red),
        }
    }
}

/// A single diagnostic entry from fd 6
#[derive(Debug, Clone)]
pub struct DiagEntry {
    pub severity: DiagSeverity,
    pub message: String,
    pub source: Option<String>,
    pub timestamp: std::time::Instant,
}

/// Sidebar panel displaying diagnostic entries (fd 6) with severity coloring
pub struct DiagPanel {
    /// Buffer of diagnostic entries, newest first
    entries: Vec<DiagEntry>,
    /// Minimum severity filter (entries below this are hidden)
    min_severity: DiagSeverity,
}

impl DiagPanel {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            min_severity: DiagSeverity::Trace,
        }
    }

    /// Push a new diagnostic entry into the panel buffer.
    ///
    /// Entries exceeding `MAX_ENTRIES` are dropped from the tail (oldest).
    pub fn push_entry(
        &mut self,
        severity: DiagSeverity,
        message: String,
        source: Option<String>,
    ) {
        self.entries.insert(
            0,
            DiagEntry {
                severity,
                message,
                source,
                timestamp: std::time::Instant::now(),
            },
        );
        self.entries.truncate(MAX_ENTRIES);
    }

    /// Set the minimum severity filter
    pub fn set_min_severity(&mut self, severity: DiagSeverity) {
        self.min_severity = severity;
    }

    /// Clear all diagnostic entries
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Number of entries currently stored (unfiltered)
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the entry buffer is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Count entries at each severity level
    fn severity_counts(&self) -> (usize, usize, usize, usize, usize) {
        let mut trace = 0;
        let mut debug = 0;
        let mut info = 0;
        let mut warn = 0;
        let mut error = 0;
        for entry in &self.entries {
            match entry.severity {
                DiagSeverity::Trace => trace += 1,
                DiagSeverity::Debug => debug += 1,
                DiagSeverity::Info => info += 1,
                DiagSeverity::Warn => warn += 1,
                DiagSeverity::Error => error += 1,
            }
        }
        (trace, debug, info, warn, error)
    }
}

impl Default for DiagPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl SidebarComponent for DiagPanel {
    fn id(&self) -> &str {
        "diag"
    }

    fn title(&self) -> &str {
        "Diagnostics"
    }

    fn default_width(&self) -> u16 {
        32
    }

    fn subscriptions(&self) -> Vec<TuplePattern> {
        vec![vec![
            Value::Str("channel_write".to_string()),
            Value::Str("6".to_string()),
            Value::Wildcard,
        ]]
    }

    fn render_block(&self, _ctx: &ComponentContext) -> SidebarOutput {
        let mut lines = Vec::new();

        if self.entries.is_empty() {
            lines.push(ComponentLine::new(vec![StyledSpan::new(
                "No diagnostics",
                Style::default().dim().italic(),
            )]));
            lines.push(ComponentLine::plain(""));
            lines.push(ComponentLine::new(vec![StyledSpan::new(
                "Diagnostics appear on fd 6",
                Style::default().dim(),
            )]));
            return SidebarOutput::visible(lines);
        }

        // Severity summary header
        let (trace, debug, info, warn, error) = self.severity_counts();
        let mut summary_spans = Vec::new();
        if error > 0 {
            summary_spans.push(StyledSpan::new(
                format!("{}E", error),
                Style::default().fg(Color::Red).bold(),
            ));
            summary_spans.push(StyledSpan::plain(" "));
        }
        if warn > 0 {
            summary_spans.push(StyledSpan::new(
                format!("{}W", warn),
                Style::default().fg(Color::Yellow).bold(),
            ));
            summary_spans.push(StyledSpan::plain(" "));
        }
        if info > 0 {
            summary_spans.push(StyledSpan::new(
                format!("{}I", info),
                Style::default().fg(Color::Cyan),
            ));
            summary_spans.push(StyledSpan::plain(" "));
        }
        if debug > 0 {
            summary_spans.push(StyledSpan::new(
                format!("{}D", debug),
                Style::default().fg(Color::White),
            ));
            summary_spans.push(StyledSpan::plain(" "));
        }
        if trace > 0 {
            summary_spans.push(StyledSpan::new(
                format!("{}T", trace),
                Style::default().fg(Color::BrightBlack).dim(),
            ));
        }
        if !summary_spans.is_empty() {
            lines.push(ComponentLine::new(summary_spans));
            lines.push(ComponentLine::plain(""));
        }

        // Render visible entries (filtered by severity)
        let visible_entries: Vec<&DiagEntry> = self
            .entries
            .iter()
            .filter(|e| e.severity >= self.min_severity)
            .collect();

        if visible_entries.is_empty() {
            lines.push(ComponentLine::new(vec![StyledSpan::new(
                format!(
                    "All {} entries below {:?} filter",
                    self.entries.len(),
                    self.min_severity
                ),
                Style::default().dim(),
            )]));
            return SidebarOutput::visible(lines);
        }

        for entry in &visible_entries {
            let age = format_age(entry.timestamp.elapsed());

            // Severity badge + optional source + age
            let mut badge_spans = vec![
                StyledSpan::new(entry.severity.label(), entry.severity.badge_style()),
            ];
            if let Some(src) = &entry.source {
                badge_spans.push(StyledSpan::new(
                    format!(" {}", src),
                    Style::default().fg(Color::BrightBlack),
                ));
            }
            badge_spans.push(StyledSpan::new(
                format!(" {}", age),
                Style::default().fg(Color::BrightBlack).dim(),
            ));
            lines.push(ComponentLine::new(badge_spans));

            // Message text with severity-appropriate styling
            let display = truncate_text(&entry.message, 80);
            lines.push(
                ComponentLine::new(vec![StyledSpan::new(
                    display,
                    entry.severity.text_style(),
                )])
                .with_indent(1),
            );
        }

        SidebarOutput::visible(lines)
    }

    fn handle_action(&self, _action: &str, _ctx: &ComponentContext) {
        // Future: filter severity via action, e.g. "filter:warn"
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
    fn test_diag_panel_empty() {
        let panel = DiagPanel::new();
        let ctx = test_context();
        let output = panel.render_block(&ctx);
        assert!(output.visible);
        assert!(!output.lines.is_empty());
        assert!(output.lines[0].spans[0].text.contains("No diagnostics"));
    }

    #[test]
    fn test_diag_panel_with_entries() {
        let mut panel = DiagPanel::new();
        panel.push_entry(DiagSeverity::Info, "server started".to_string(), None);
        panel.push_entry(
            DiagSeverity::Warn,
            "deprecated API".to_string(),
            Some("api".to_string()),
        );
        panel.push_entry(
            DiagSeverity::Error,
            "connection failed".to_string(),
            Some("net".to_string()),
        );

        let ctx = test_context();
        let output = panel.render_block(&ctx);
        assert!(output.visible);
        // Summary + blank + 3 entries (each 2 lines: badge, text)
        assert!(output.lines.len() >= 8);
    }

    #[test]
    fn test_diag_severity_labels() {
        assert_eq!(DiagSeverity::Trace.label(), "TRACE");
        assert_eq!(DiagSeverity::Debug.label(), "DEBUG");
        assert_eq!(DiagSeverity::Info.label(), " INFO");
        assert_eq!(DiagSeverity::Warn.label(), " WARN");
        assert_eq!(DiagSeverity::Error.label(), "ERROR");
    }

    #[test]
    fn test_diag_severity_badge_styles() {
        // Verify badge styles produce the expected foreground colors
        assert_eq!(DiagSeverity::Trace.badge_style().fg, Some(Color::BrightBlack));
        assert_eq!(DiagSeverity::Info.badge_style().fg, Some(Color::Cyan));
        assert_eq!(DiagSeverity::Warn.badge_style().fg, Some(Color::Yellow));
        assert_eq!(DiagSeverity::Error.badge_style().fg, Some(Color::Red));
    }

    #[test]
    fn test_diag_severity_ordering() {
        assert!(DiagSeverity::Trace < DiagSeverity::Debug);
        assert!(DiagSeverity::Debug < DiagSeverity::Info);
        assert!(DiagSeverity::Info < DiagSeverity::Warn);
        assert!(DiagSeverity::Warn < DiagSeverity::Error);
    }

    #[test]
    fn test_diag_panel_severity_filter() {
        let mut panel = DiagPanel::new();
        panel.push_entry(DiagSeverity::Trace, "trace msg".to_string(), None);
        panel.push_entry(DiagSeverity::Warn, "warn msg".to_string(), None);
        panel.push_entry(DiagSeverity::Error, "error msg".to_string(), None);

        panel.set_min_severity(DiagSeverity::Warn);

        let ctx = test_context();
        let output = panel.render_block(&ctx);
        assert!(output.visible);

        // Trace entry should be filtered out -- only warn + error visible
        // Each visible entry is 2 lines (badge + message) plus summary + blank
        let text_lines: Vec<&ComponentLine> = output
            .lines
            .iter()
            .filter(|l| l.spans.iter().any(|s| s.text.contains("trace msg")))
            .collect();
        assert!(text_lines.is_empty(), "trace entry should be filtered");
    }

    #[test]
    fn test_diag_panel_severity_counts() {
        let mut panel = DiagPanel::new();
        panel.push_entry(DiagSeverity::Info, "info".to_string(), None);
        panel.push_entry(DiagSeverity::Warn, "warn1".to_string(), None);
        panel.push_entry(DiagSeverity::Warn, "warn2".to_string(), None);
        panel.push_entry(DiagSeverity::Error, "err".to_string(), None);

        let (trace, debug, info, warn, error) = panel.severity_counts();
        assert_eq!(trace, 0);
        assert_eq!(debug, 0);
        assert_eq!(info, 1);
        assert_eq!(warn, 2);
        assert_eq!(error, 1);
    }

    #[test]
    fn test_diag_panel_max_entries() {
        let mut panel = DiagPanel::new();
        for i in 0..300 {
            panel.push_entry(DiagSeverity::Info, format!("entry {}", i), None);
        }
        assert_eq!(panel.len(), MAX_ENTRIES);
    }

    #[test]
    fn test_diag_panel_clear() {
        let mut panel = DiagPanel::new();
        panel.push_entry(DiagSeverity::Error, "test".to_string(), None);
        assert!(!panel.is_empty());
        panel.clear();
        assert!(panel.is_empty());
    }

    #[test]
    fn test_diag_panel_with_source() {
        let mut panel = DiagPanel::new();
        panel.push_entry(
            DiagSeverity::Error,
            "timeout".to_string(),
            Some("network".to_string()),
        );

        let ctx = test_context();
        let output = panel.render_block(&ctx);
        assert!(output.visible);

        // Should contain the source label
        let has_source = output.lines.iter().any(|l| {
            l.spans.iter().any(|s| s.text.contains("network"))
        });
        assert!(has_source);
    }

    #[test]
    fn test_subscriptions() {
        let panel = DiagPanel::new();
        let subs = panel.subscriptions();
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0][0], Value::Str("channel_write".to_string()));
        assert_eq!(subs[0][1], Value::Str("6".to_string()));
    }
}
