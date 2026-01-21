use crate::component::{ComponentOutput, PromptComponent, TuplePattern};
use crate::context::ComponentContext;
use crate::{Color, Style, StyledSpan};

pub struct HostnameComponent {
    show_user: bool,
}

impl HostnameComponent {
    pub fn new() -> Self {
        Self { show_user: false }
    }

    pub fn with_user() -> Self {
        Self { show_user: true }
    }
}

impl Default for HostnameComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptComponent for HostnameComponent {
    fn id(&self) -> &str {
        "hostname"
    }

    fn subscriptions(&self) -> Vec<TuplePattern> {
        vec![]
    }

    fn render(&self, ctx: &ComponentContext) -> ComponentOutput {
        let is_ssh = ctx.get_env("SSH_CLIENT").is_some() || ctx.get_env("SSH_TTY").is_some();

        if !is_ssh {
            return ComponentOutput::hidden();
        }

        let hostname = ctx.hostname.as_deref().unwrap_or("unknown");

        let text = if self.show_user {
            let user = ctx.get_env("USER").unwrap_or("user");
            format!("{}@{}", user, hostname)
        } else {
            hostname.to_string()
        };

        let spans = vec![StyledSpan::new(
            text.clone(),
            Style::default().fg(Color::Magenta),
        )];
        ComponentOutput::with_data(
            spans,
            serde_json::json!({
                "hostname": hostname,
                "ssh": true,
            }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn test_ctx(ssh: bool, hostname: Option<&str>) -> ComponentContext {
        let mut env = HashMap::new();
        if ssh {
            env.insert("SSH_CLIENT".to_string(), "1.2.3.4 5678 22".to_string());
        }
        env.insert("USER".to_string(), "testuser".to_string());
        ComponentContext {
            cwd: std::path::PathBuf::from("/"),
            env,
            last_exit_code: 0,
            running_jobs: 0,
            stopped_jobs: 0,
            model: None,
            last_command_duration: None,
            hostname: hostname.map(|s| s.to_string()),
        }
    }

    #[test]
    fn test_hidden_without_ssh() {
        let c = HostnameComponent::new();
        assert!(!c.render(&test_ctx(false, Some("myhost"))).visible);
    }

    #[test]
    fn test_visible_with_ssh() {
        let c = HostnameComponent::new();
        let output = c.render(&test_ctx(true, Some("myhost")));
        assert!(output.visible);
        assert!(output.spans[0].text.contains("myhost"));
    }

    #[test]
    fn test_with_user() {
        let c = HostnameComponent::with_user();
        let output = c.render(&test_ctx(true, Some("myhost")));
        assert!(output.spans[0].text.contains("testuser@myhost"));
    }
}
