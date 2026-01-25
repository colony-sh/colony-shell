use crate::component::{
    SidebarComponent, SidebarLine, SidebarOutput, TuplePattern,
};
use crate::context::ComponentContext;
use crate::{Color, Style, StyledSpan};
use colony_tuple::Value;
use std::time::Instant;

pub struct SessionSidebarComponent {
    started_at: Instant,
}

impl SessionSidebarComponent {
    pub fn new() -> Self {
        Self {
            started_at: Instant::now(),
        }
    }
}

impl Default for SessionSidebarComponent {
    fn default() -> Self {
        Self::new()
    }
}

fn format_uptime(elapsed: std::time::Duration) -> String {
    let secs = elapsed.as_secs();
    let hours = secs / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;

    if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, seconds)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}

impl SidebarComponent for SessionSidebarComponent {
    fn id(&self) -> &str {
        "session"
    }

    fn title(&self) -> &str {
        "Session"
    }

    fn subscriptions(&self) -> Vec<TuplePattern> {
        vec![vec![Value::Str("cmd_complete".to_string())]]
    }

    fn render_block(&self, ctx: &ComponentContext) -> SidebarOutput {
        let mut lines = Vec::new();

        // Shell name
        let shell_name = ctx.get_env("SHELL").unwrap_or("colony");
        lines.push(
            SidebarLine::new(StyledSpan::new(
                shell_name.to_string(),
                Style::default().fg(Color::Cyan),
            ))
            .with_icon(StyledSpan::new(
                "\u{f489}",
                Style::default().fg(Color::Green),
            )),
        );

        // PID
        let pid = std::process::id();
        lines.push(
            SidebarLine::new(StyledSpan::new(
                format!("PID {}", pid),
                Style::default().fg(Color::BrightBlack),
            ))
            .with_icon(StyledSpan::new(
                "\u{f85a}",
                Style::default().fg(Color::BrightBlack),
            )),
        );

        // Uptime
        let uptime = format_uptime(self.started_at.elapsed());
        lines.push(
            SidebarLine::new(StyledSpan::new(
                uptime,
                Style::default().fg(Color::BrightBlack),
            ))
            .with_icon(StyledSpan::new(
                "\u{f017}",
                Style::default().fg(Color::Yellow),
            )),
        );

        // Model (if set)
        if let Some(ref model) = ctx.model {
            lines.push(
                SidebarLine::new(StyledSpan::new(
                    model.clone(),
                    Style::default().fg(Color::Blue),
                ))
                .with_icon(StyledSpan::new(
                    "\u{f544}",
                    Style::default().fg(Color::Blue),
                )),
            );
        }

        // Hostname (if set)
        if let Some(ref hostname) = ctx.hostname {
            lines.push(
                SidebarLine::new(StyledSpan::new(
                    hostname.clone(),
                    Style::default().fg(Color::Magenta),
                ))
                .with_icon(StyledSpan::new(
                    "\u{f108}",
                    Style::default().fg(Color::Magenta),
                )),
            );
        }

        // CWD
        let cwd_display = ctx.cwd.to_string_lossy().to_string();
        lines.push(
            SidebarLine::new(StyledSpan::new(
                cwd_display,
                Style::default().fg(Color::Cyan),
            ))
            .with_icon(StyledSpan::new(
                "\u{f07b}",
                Style::default().fg(Color::Cyan),
            )),
        );

        SidebarOutput::from_sidebar_lines_with_data(
            lines,
            serde_json::json!({
                "pid": std::process::id(),
                "model": ctx.model,
            }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn test_context() -> ComponentContext {
        ComponentContext {
            cwd: std::path::PathBuf::from("/home/test"),
            env: {
                let mut env = HashMap::new();
                env.insert("SHELL".to_string(), "colony".to_string());
                env
            },
            last_exit_code: 0,
            running_jobs: 0,
            stopped_jobs: 0,
            model: Some("claude-3".to_string()),
            last_command_duration: None,
            hostname: Some("myhost".to_string()),
        }
    }

    #[test]
    fn test_session_id() {
        let component = SessionSidebarComponent::new();
        assert_eq!(component.id(), "session");
    }

    #[test]
    fn test_session_title() {
        let component = SessionSidebarComponent::new();
        assert_eq!(component.title(), "Session");
    }

    #[test]
    fn test_session_position() {
        let component = SessionSidebarComponent::new();
        assert_eq!(component.position(), SidebarPosition::Right);
    }

    #[test]
    fn test_session_render_has_lines() {
        let component = SessionSidebarComponent::new();
        let ctx = test_context();
        let output = component.render(&ctx);

        assert!(output.visible);
        // Should have shell, pid, uptime, model, hostname, cwd = 6 lines
        assert!(output.lines.len() >= 5);
    }

    #[test]
    fn test_session_render_without_model() {
        let component = SessionSidebarComponent::new();
        let mut ctx = test_context();
        ctx.model = None;
        ctx.hostname = None;
        let output = component.render(&ctx);

        assert!(output.visible);
        // shell, pid, uptime, cwd = 4 lines (no model, no hostname)
        assert_eq!(output.lines.len(), 4);
    }

    #[test]
    fn test_format_uptime_seconds() {
        let d = std::time::Duration::from_secs(45);
        assert_eq!(format_uptime(d), "45s");
    }

    #[test]
    fn test_format_uptime_minutes() {
        let d = std::time::Duration::from_secs(125);
        assert_eq!(format_uptime(d), "2m 5s");
    }

    #[test]
    fn test_format_uptime_hours() {
        let d = std::time::Duration::from_secs(3665);
        assert_eq!(format_uptime(d), "1h 1m 5s");
    }
}
