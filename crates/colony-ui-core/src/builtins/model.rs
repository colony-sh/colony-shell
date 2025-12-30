use crate::component::{ComponentOutput, PromptComponent, TuplePattern};
use crate::context::ComponentContext;
use crate::{Color, Style, StyledSpan};
use colony_tuple::Value;

pub struct ModelComponent;

impl ModelComponent {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ModelComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptComponent for ModelComponent {
    fn id(&self) -> &str {
        "model"
    }

    fn subscriptions(&self) -> Vec<TuplePattern> {
        vec![vec![Value::Str("model_change".to_string()), Value::Wildcard]]
    }

    fn render(&self, ctx: &ComponentContext) -> ComponentOutput {
        let model = match &ctx.model {
            Some(m) => m,
            None => return ComponentOutput::hidden(),
        };

        let text = format!("🤖 {}", model);
        let spans = vec![StyledSpan::new(text, Style::default().fg(Color::Blue))];

        ComponentOutput::with_data(
            spans,
            serde_json::json!({
                "model": model,
            }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn test_context(model: Option<&str>) -> ComponentContext {
        ComponentContext {
            cwd: std::path::PathBuf::from("/"),
            env: HashMap::new(),
            last_exit_code: 0,
            running_jobs: 0,
            stopped_jobs: 0,
            model: model.map(|s| s.to_string()),
        }
    }

    #[test]
    fn test_model_none_hidden() {
        let component = ModelComponent::new();
        let ctx = test_context(None);
        let output = component.render(&ctx);

        assert!(!output.visible);
    }

    #[test]
    fn test_model_visible() {
        let component = ModelComponent::new();
        let ctx = test_context(Some("gpt-4"));
        let output = component.render(&ctx);

        assert!(output.visible);
        assert!(output.spans[0].text.contains("gpt-4"));
    }
}
