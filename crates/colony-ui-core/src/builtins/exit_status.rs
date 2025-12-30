use crate::component::{ComponentOutput, PromptComponent, TuplePattern};
use crate::context::ComponentContext;
use crate::{Color, Style, StyledSpan};
use colony_tuple::Value;

pub struct ExitStatusComponent;

impl ExitStatusComponent {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ExitStatusComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptComponent for ExitStatusComponent {
    fn id(&self) -> &str {
        "exit_status"
    }

    fn subscriptions(&self) -> Vec<TuplePattern> {
        vec![vec![Value::Str("cmd_complete".to_string()), Value::Wildcard]]
    }

    fn render(&self, ctx: &ComponentContext) -> ComponentOutput {
        if ctx.last_exit_code == 0 {
            return ComponentOutput::hidden();
        }

        let text = format!("✘ {}", ctx.last_exit_code);
        let spans = vec![StyledSpan::new(
            text,
            Style::default().fg(Color::Red).bold(),
        )];

        ComponentOutput::with_data(
            spans,
            serde_json::json!({
                "code": ctx.last_exit_code,
            }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn test_context(exit_code: i32) -> ComponentContext {
        ComponentContext {
            cwd: std::path::PathBuf::from("/"),
            env: HashMap::new(),
            last_exit_code: exit_code,
            running_jobs: 0,
            stopped_jobs: 0,
            model: None,
        }
    }

    #[test]
    fn test_exit_status_zero_hidden() {
        let component = ExitStatusComponent::new();
        let ctx = test_context(0);
        let output = component.render(&ctx);

        assert!(!output.visible);
    }

    #[test]
    fn test_exit_status_nonzero_visible() {
        let component = ExitStatusComponent::new();
        let ctx = test_context(1);
        let output = component.render(&ctx);

        assert!(output.visible);
        assert!(output.spans[0].text.contains("1"));
    }

    #[test]
    fn test_exit_status_127_not_found() {
        let component = ExitStatusComponent::new();
        let ctx = test_context(127);
        let output = component.render(&ctx);

        assert!(output.visible);
        assert!(output.spans[0].text.contains("127"));
    }
}
