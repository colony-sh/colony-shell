//! Session info sidebar panel

use crate::component::{
    Color, ComponentLine, SidebarComponent, SidebarOutput, Style, StyledSpan, TuplePattern,
};
use crate::context::ComponentContext;
use colony_tuple::Value;
use std::time::Duration;

/// Session information panel showing token usage, cost, and timing
pub struct SessionPanel {
    /// Total tokens used in session
    total_tokens: u64,
    /// Total input tokens
    input_tokens: u64,
    /// Total output tokens
    output_tokens: u64,
    /// Estimated cost (USD)
    estimated_cost: f64,
    /// Number of commands run
    command_count: u32,
    /// Session start time
    session_start: Option<std::time::Instant>,
}

impl SessionPanel {
    pub fn new() -> Self {
        Self {
            total_tokens: 0,
            input_tokens: 0,
            output_tokens: 0,
            estimated_cost: 0.0,
            command_count: 0,
            session_start: Some(std::time::Instant::now()),
        }
    }

    /// Update token counts (would be called by agent after each interaction)
    pub fn update_tokens(&mut self, input: u64, output: u64) {
        self.input_tokens += input;
        self.output_tokens += output;
        self.total_tokens = self.input_tokens + self.output_tokens;

        // Rough cost estimate (Claude 3.5 Sonnet pricing)
        // Input: $3/MTok, Output: $15/MTok
        self.estimated_cost = (self.input_tokens as f64 * 3.0 / 1_000_000.0)
            + (self.output_tokens as f64 * 15.0 / 1_000_000.0);
    }

    /// Increment command count
    pub fn increment_commands(&mut self) {
        self.command_count += 1;
    }

    fn format_tokens(tokens: u64) -> String {
        if tokens >= 1_000_000 {
            format!("{:.1}M", tokens as f64 / 1_000_000.0)
        } else if tokens >= 1_000 {
            format!("{:.1}K", tokens as f64 / 1_000.0)
        } else {
            tokens.to_string()
        }
    }

    fn format_duration(duration: Duration) -> String {
        let secs = duration.as_secs();
        if secs >= 3600 {
            format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
        } else if secs >= 60 {
            format!("{}m {}s", secs / 60, secs % 60)
        } else {
            format!("{}s", secs)
        }
    }
}

impl Default for SessionPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl SidebarComponent for SessionPanel {
    fn id(&self) -> &str {
        "session"
    }

    fn title(&self) -> &str {
        "Session"
    }

    fn default_width(&self) -> u16 {
        25
    }

    fn subscriptions(&self) -> Vec<TuplePattern> {
        vec![
            vec![Value::Str("cmd_complete".to_string()), Value::Wildcard],
            vec![
                Value::Str("agent_tokens".to_string()),
                Value::Wildcard,
                Value::Wildcard,
            ],
        ]
    }

    fn render_block(&self, _ctx: &ComponentContext) -> SidebarOutput {
        let mut lines = Vec::new();

        // Session duration
        if let Some(start) = self.session_start {
            let duration = start.elapsed();
            lines.push(ComponentLine::new(vec![
                StyledSpan::new("Duration: ", Style::default().dim()),
                StyledSpan::new(
                    Self::format_duration(duration),
                    Style::default().fg(Color::Cyan),
                ),
            ]));
        }

        // Command count
        lines.push(ComponentLine::new(vec![
            StyledSpan::new("Commands: ", Style::default().dim()),
            StyledSpan::new(
                self.command_count.to_string(),
                Style::default().fg(Color::Green),
            ),
        ]));

        // Separator
        lines.push(ComponentLine::plain(""));

        // Token usage header
        lines.push(ComponentLine::new(vec![StyledSpan::new(
            "Token Usage",
            Style::default().bold(),
        )]));

        // Input tokens
        lines.push(ComponentLine::new(vec![
            StyledSpan::new("  Input:  ", Style::default().dim()),
            StyledSpan::new(
                Self::format_tokens(self.input_tokens),
                Style::default().fg(Color::Blue),
            ),
        ]));

        // Output tokens
        lines.push(ComponentLine::new(vec![
            StyledSpan::new("  Output: ", Style::default().dim()),
            StyledSpan::new(
                Self::format_tokens(self.output_tokens),
                Style::default().fg(Color::Yellow),
            ),
        ]));

        // Total tokens
        lines.push(ComponentLine::new(vec![
            StyledSpan::new("  Total:  ", Style::default().dim()),
            StyledSpan::new(
                Self::format_tokens(self.total_tokens),
                Style::default().fg(Color::White).bold(),
            ),
        ]));

        // Separator
        lines.push(ComponentLine::plain(""));

        // Cost estimate
        lines.push(ComponentLine::new(vec![
            StyledSpan::new("Est. Cost: ", Style::default().dim()),
            StyledSpan::new(
                format!("${:.4}", self.estimated_cost),
                Style::default().fg(Color::Green),
            ),
        ]));

        SidebarOutput::visible(lines)
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
    fn test_session_panel_renders() {
        let panel = SessionPanel::new();
        let ctx = test_context();
        let output = panel.render_block(&ctx);
        assert!(output.visible);
        assert!(!output.lines.is_empty());
    }

    #[test]
    fn test_format_tokens() {
        assert_eq!(SessionPanel::format_tokens(500), "500");
        assert_eq!(SessionPanel::format_tokens(1500), "1.5K");
        assert_eq!(SessionPanel::format_tokens(1_500_000), "1.5M");
    }
}
