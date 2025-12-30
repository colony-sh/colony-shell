use crate::component::{ComponentOutput, PromptComponent, TuplePattern};
use crate::context::ComponentContext;
use crate::{Color, Style, StyledSpan};
use colony_tuple::Value;

pub struct JobsComponent;

impl JobsComponent {
    pub fn new() -> Self {
        Self
    }
}

impl Default for JobsComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptComponent for JobsComponent {
    fn id(&self) -> &str {
        "jobs"
    }

    fn subscriptions(&self) -> Vec<TuplePattern> {
        vec![vec![Value::Str("job_change".to_string()), Value::Wildcard]]
    }

    fn render(&self, ctx: &ComponentContext) -> ComponentOutput {
        let total = ctx.running_jobs + ctx.stopped_jobs;

        if total == 0 {
            return ComponentOutput::hidden();
        }

        let text = format!("⚙ {}", total);
        let color = if ctx.stopped_jobs > 0 {
            Color::Red
        } else {
            Color::Yellow
        };

        let spans = vec![StyledSpan::new(text, Style::default().fg(color))];

        ComponentOutput::with_data(
            spans,
            serde_json::json!({
                "running": ctx.running_jobs,
                "stopped": ctx.stopped_jobs,
            }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn test_context(running: usize, stopped: usize) -> ComponentContext {
        ComponentContext {
            cwd: std::path::PathBuf::from("/"),
            env: HashMap::new(),
            last_exit_code: 0,
            running_jobs: running,
            stopped_jobs: stopped,
            model: None,
        }
    }

    #[test]
    fn test_jobs_none_hidden() {
        let component = JobsComponent::new();
        let ctx = test_context(0, 0);
        let output = component.render(&ctx);

        assert!(!output.visible);
    }

    #[test]
    fn test_jobs_running() {
        let component = JobsComponent::new();
        let ctx = test_context(2, 0);
        let output = component.render(&ctx);

        assert!(output.visible);
        assert!(output.spans[0].text.contains("2"));
    }

    #[test]
    fn test_jobs_stopped() {
        let component = JobsComponent::new();
        let ctx = test_context(0, 1);
        let output = component.render(&ctx);

        assert!(output.visible);
    }
}
