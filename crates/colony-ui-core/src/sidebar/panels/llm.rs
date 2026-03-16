//! LLM conversation channel (fd 4) sidebar panel
//!
//! Displays LLM conversation messages written to file descriptor 4.
//! Messages are rendered with role-colored badges.

use crate::component::{
    Color, ComponentLine, SidebarComponent, SidebarOutput, Style, StyledSpan, TuplePattern,
};
use crate::context::ComponentContext;
use colony_tuple::Value;

/// Maximum number of messages retained in the panel buffer
const MAX_MESSAGES: usize = 50;

/// Maximum lines shown per message before truncation
const MAX_LINES_PER_MESSAGE: usize = 3;

/// Role of an LLM conversation message
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmRole {
    User,
    Assistant,
    System,
    Tool,
}

impl LlmRole {
    /// Short badge label for display
    fn badge(&self) -> &'static str {
        match self {
            Self::User => "[USR]",
            Self::Assistant => "[AST]",
            Self::System => "[SYS]",
            Self::Tool => "[TOOL]",
        }
    }

    /// Color associated with this role
    fn color(&self) -> Color {
        match self {
            Self::User => Color::Blue,
            Self::Assistant => Color::Green,
            Self::System => Color::Yellow,
            Self::Tool => Color::Cyan,
        }
    }
}

/// A single LLM conversation message
#[derive(Debug, Clone)]
pub struct LlmMessage {
    pub role: LlmRole,
    pub content: String,
    pub timestamp: std::time::Instant,
}

/// Sidebar panel displaying LLM conversation messages (fd 4)
pub struct LlmPanel {
    /// Buffer of messages, newest first
    messages: Vec<LlmMessage>,
}

impl LlmPanel {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
        }
    }

    /// Push a new LLM message into the panel buffer.
    ///
    /// Messages exceeding `MAX_MESSAGES` are dropped from the tail (oldest).
    pub fn push_message(&mut self, role: LlmRole, content: String) {
        self.messages.insert(
            0,
            LlmMessage {
                role,
                content,
                timestamp: std::time::Instant::now(),
            },
        );
        self.messages.truncate(MAX_MESSAGES);
    }

    /// Clear all messages
    pub fn clear(&mut self) {
        self.messages.clear();
    }

    /// Number of messages currently stored
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// Whether the message buffer is empty
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }
}

impl Default for LlmPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl SidebarComponent for LlmPanel {
    fn id(&self) -> &str {
        "llm"
    }

    fn title(&self) -> &str {
        "LLM"
    }

    fn default_width(&self) -> u16 {
        32
    }

    fn subscriptions(&self) -> Vec<TuplePattern> {
        vec![vec![
            Value::Str("channel_write".to_string()),
            Value::Str("4".to_string()),
            Value::Wildcard,
        ]]
    }

    fn render_block(&self, _ctx: &ComponentContext) -> SidebarOutput {
        let mut lines = Vec::new();

        if self.messages.is_empty() {
            lines.push(ComponentLine::new(vec![StyledSpan::new(
                "No LLM messages",
                Style::default().dim().italic(),
            )]));
            lines.push(ComponentLine::plain(""));
            lines.push(ComponentLine::new(vec![StyledSpan::new(
                "Messages appear on fd 4",
                Style::default().dim(),
            )]));
            return SidebarOutput::visible(lines);
        }

        // Message count header
        lines.push(ComponentLine::new(vec![StyledSpan::new(
            format!("{} messages", self.messages.len()),
            Style::default().fg(Color::Cyan).bold(),
        )]));
        lines.push(ComponentLine::plain(""));

        // Render messages newest first
        for msg in &self.messages {
            let role_style = Style::default().fg(msg.role.color()).bold();

            // Role badge line
            lines.push(ComponentLine::new(vec![
                StyledSpan::new(msg.role.badge(), role_style),
                StyledSpan::new(
                    format!(
                        " {}",
                        format_age(msg.timestamp.elapsed())
                    ),
                    Style::default().fg(Color::BrightBlack).dim(),
                ),
            ]));

            // Message content, truncated to MAX_LINES_PER_MESSAGE
            let content_lines: Vec<&str> = msg.content.lines().collect();
            let truncated = content_lines.len() > MAX_LINES_PER_MESSAGE;
            let visible_lines = if truncated {
                &content_lines[..MAX_LINES_PER_MESSAGE]
            } else {
                &content_lines
            };

            for line_text in visible_lines {
                let display = truncate_line(line_text, 80);
                lines.push(
                    ComponentLine::new(vec![StyledSpan::new(
                        display,
                        Style::default().fg(Color::White),
                    )])
                    .with_indent(1),
                );
            }

            if truncated {
                lines.push(
                    ComponentLine::new(vec![StyledSpan::new(
                        format!("... ({} more lines)", content_lines.len() - MAX_LINES_PER_MESSAGE),
                        Style::default().fg(Color::BrightBlack).dim(),
                    )])
                    .with_indent(1),
                );
            }

            // Blank separator between messages
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

/// Truncate a single line to a maximum character length
fn truncate_line(text: &str, max_chars: usize) -> String {
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
    fn test_llm_panel_empty() {
        let panel = LlmPanel::new();
        let ctx = test_context();
        let output = panel.render_block(&ctx);
        assert!(output.visible);
        assert!(!output.lines.is_empty());
        assert!(output.lines[0].spans[0].text.contains("No LLM"));
    }

    #[test]
    fn test_llm_panel_with_messages() {
        let mut panel = LlmPanel::new();
        panel.push_message(LlmRole::User, "Hello, world".to_string());
        panel.push_message(LlmRole::Assistant, "Hi there!".to_string());

        let ctx = test_context();
        let output = panel.render_block(&ctx);
        assert!(output.visible);
        // Header + blank + 2 messages (badge + content + blank each)
        assert!(output.lines.len() >= 6);
    }

    #[test]
    fn test_llm_panel_role_badges() {
        assert_eq!(LlmRole::User.badge(), "[USR]");
        assert_eq!(LlmRole::Assistant.badge(), "[AST]");
        assert_eq!(LlmRole::System.badge(), "[SYS]");
        assert_eq!(LlmRole::Tool.badge(), "[TOOL]");
    }

    #[test]
    fn test_llm_panel_role_colors() {
        assert_eq!(LlmRole::User.color(), Color::Blue);
        assert_eq!(LlmRole::Assistant.color(), Color::Green);
        assert_eq!(LlmRole::System.color(), Color::Yellow);
        assert_eq!(LlmRole::Tool.color(), Color::Cyan);
    }

    #[test]
    fn test_llm_panel_truncation() {
        let mut panel = LlmPanel::new();
        let long_content = "Line 1\nLine 2\nLine 3\nLine 4\nLine 5\nLine 6";
        panel.push_message(LlmRole::Assistant, long_content.to_string());

        let ctx = test_context();
        let output = panel.render_block(&ctx);
        assert!(output.visible);

        // Should contain a "... (X more lines)" indicator
        let has_truncation = output.lines.iter().any(|l| {
            l.spans
                .iter()
                .any(|s| s.text.contains("more lines"))
        });
        assert!(has_truncation);
    }

    #[test]
    fn test_llm_panel_ordering() {
        let mut panel = LlmPanel::new();
        panel.push_message(LlmRole::User, "first".to_string());
        panel.push_message(LlmRole::Assistant, "second".to_string());

        assert_eq!(panel.messages[0].content, "second");
        assert_eq!(panel.messages[1].content, "first");
    }

    #[test]
    fn test_llm_panel_max_messages() {
        let mut panel = LlmPanel::new();
        for i in 0..80 {
            panel.push_message(LlmRole::User, format!("msg {}", i));
        }
        assert_eq!(panel.len(), MAX_MESSAGES);
    }

    #[test]
    fn test_llm_panel_clear() {
        let mut panel = LlmPanel::new();
        panel.push_message(LlmRole::User, "test".to_string());
        assert!(!panel.is_empty());
        panel.clear();
        assert!(panel.is_empty());
    }

    #[test]
    fn test_subscriptions() {
        let panel = LlmPanel::new();
        let subs = panel.subscriptions();
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0][0], Value::Str("channel_write".to_string()));
        assert_eq!(subs[0][1], Value::Str("4".to_string()));
    }

    #[test]
    fn test_truncate_line() {
        assert_eq!(truncate_line("short", 10), "short");
        assert_eq!(truncate_line("a very long string here", 10), "a very...");
    }
}
