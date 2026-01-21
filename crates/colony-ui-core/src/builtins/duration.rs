use crate::component::{ComponentOutput, PromptComponent, TuplePattern};
use crate::context::ComponentContext;
use crate::{Color, Style, StyledSpan};
use std::time::Duration;

pub struct DurationComponent {
    threshold: Duration,
}

impl DurationComponent {
    pub fn new() -> Self {
        Self {
            threshold: Duration::from_secs(1),
        }
    }

    pub fn with_threshold(threshold: Duration) -> Self {
        Self { threshold }
    }

    fn format_duration(d: Duration) -> String {
        let secs = d.as_secs();
        let millis = d.subsec_millis();

        if secs >= 3600 {
            let hours = secs / 3600;
            let mins = (secs % 3600) / 60;
            format!("{}h{}m", hours, mins)
        } else if secs >= 60 {
            let mins = secs / 60;
            let s = secs % 60;
            format!("{}m{}s", mins, s)
        } else if secs > 0 {
            format!("{}.{}s", secs, millis / 100)
        } else {
            format!("{}ms", millis)
        }
    }
}

impl Default for DurationComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptComponent for DurationComponent {
    fn id(&self) -> &str {
        "duration"
    }

    fn subscriptions(&self) -> Vec<TuplePattern> {
        vec![]
    }

    fn render(&self, ctx: &ComponentContext) -> ComponentOutput {
        match ctx.last_command_duration {
            Some(d) if d >= self.threshold => {
                let text = format!("⏱ {}", Self::format_duration(d));
                let spans = vec![StyledSpan::new(text, Style::default().fg(Color::Yellow))];
                ComponentOutput::with_data(
                    spans,
                    serde_json::json!({
                        "ms": d.as_millis(),
                        "display": Self::format_duration(d),
                    }),
                )
            }
            _ => ComponentOutput::hidden(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn test_ctx(duration: Option<Duration>) -> ComponentContext {
        ComponentContext {
            cwd: std::path::PathBuf::from("/"),
            env: HashMap::new(),
            last_exit_code: 0,
            running_jobs: 0,
            stopped_jobs: 0,
            model: None,
            last_command_duration: duration,
            hostname: None,
        }
    }

    #[test]
    fn test_hidden_when_none() {
        let c = DurationComponent::new();
        let output = c.render(&test_ctx(None));
        assert!(!output.visible);
    }

    #[test]
    fn test_hidden_below_threshold() {
        let c = DurationComponent::new();
        let output = c.render(&test_ctx(Some(Duration::from_millis(500))));
        assert!(!output.visible);
    }

    #[test]
    fn test_visible_above_threshold() {
        let c = DurationComponent::new();
        let output = c.render(&test_ctx(Some(Duration::from_secs(2))));
        assert!(output.visible);
        assert!(output.spans[0].text.contains("2"));
    }

    #[test]
    fn test_format_minutes() {
        assert_eq!(
            DurationComponent::format_duration(Duration::from_secs(125)),
            "2m5s"
        );
    }

    #[test]
    fn test_format_hours() {
        assert_eq!(
            DurationComponent::format_duration(Duration::from_secs(3725)),
            "1h2m"
        );
    }
}
