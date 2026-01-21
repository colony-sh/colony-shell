use crate::component::{ComponentOutput, PromptComponent, TuplePattern};
use crate::context::ComponentContext;
use crate::{Color, Style, StyledSpan};

pub struct AwsProfileComponent;

impl AwsProfileComponent {
    pub fn new() -> Self {
        Self
    }
}

impl Default for AwsProfileComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptComponent for AwsProfileComponent {
    fn id(&self) -> &str {
        "aws_profile"
    }

    fn subscriptions(&self) -> Vec<TuplePattern> {
        vec![]
    }

    fn render(&self, ctx: &ComponentContext) -> ComponentOutput {
        match ctx.get_env("AWS_PROFILE") {
            Some(profile) if profile != "default" => {
                let text = format!("☁ {}", profile);
                let spans = vec![StyledSpan::new(
                    text,
                    Style::default().fg(Color::Rgb(255, 153, 0)),
                )];
                ComponentOutput::with_data(spans, serde_json::json!({ "profile": profile }))
            }
            _ => ComponentOutput::hidden(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn test_ctx(profile: Option<&str>) -> ComponentContext {
        let mut env = HashMap::new();
        if let Some(p) = profile {
            env.insert("AWS_PROFILE".to_string(), p.to_string());
        }
        ComponentContext {
            cwd: std::path::PathBuf::from("/"),
            env,
            last_exit_code: 0,
            running_jobs: 0,
            stopped_jobs: 0,
            model: None,
            last_command_duration: None,
            hostname: None,
        }
    }

    #[test]
    fn test_hidden_when_none() {
        let c = AwsProfileComponent::new();
        assert!(!c.render(&test_ctx(None)).visible);
    }

    #[test]
    fn test_hidden_when_default() {
        let c = AwsProfileComponent::new();
        assert!(!c.render(&test_ctx(Some("default"))).visible);
    }

    #[test]
    fn test_visible_with_profile() {
        let c = AwsProfileComponent::new();
        let output = c.render(&test_ctx(Some("production")));
        assert!(output.visible);
        assert!(output.spans[0].text.contains("production"));
    }
}
