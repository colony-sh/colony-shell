use crate::component::{ComponentOutput, PromptComponent, TuplePattern};
use crate::context::ComponentContext;
use crate::{Color, Style, StyledSpan};
use colony_tuple::Value;

pub struct PwdComponent {
    max_depth: usize,
}

impl PwdComponent {
    pub fn new() -> Self {
        Self { max_depth: 0 }
    }

    pub fn with_max_depth(max_depth: usize) -> Self {
        Self { max_depth }
    }

    fn truncate_path(&self, path: &str) -> String {
        if self.max_depth == 0 {
            return path.to_string();
        }

        let components: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

        if components.len() <= self.max_depth {
            return path.to_string();
        }

        let truncated: Vec<&str> = components
            .iter()
            .rev()
            .take(self.max_depth)
            .copied()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();

        format!("...{}", truncated.join("/"))
    }
}

impl Default for PwdComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptComponent for PwdComponent {
    fn id(&self) -> &str {
        "pwd"
    }

    fn subscriptions(&self) -> Vec<TuplePattern> {
        vec![vec![
            Value::Str("dir_change".to_string()),
            Value::Wildcard,
            Value::Wildcard,
        ]]
    }

    fn render(&self, ctx: &ComponentContext) -> ComponentOutput {
        let cwd = ctx.cwd.to_string_lossy();

        let display_path = if let Some(home) = ctx.home_dir() {
            if cwd.starts_with(home) {
                if cwd.len() == home.len() {
                    "~".to_string()
                } else {
                    format!("~{}", &cwd[home.len()..])
                }
            } else {
                cwd.to_string()
            }
        } else {
            cwd.to_string()
        };

        let display_path = self.truncate_path(&display_path);

        let spans = vec![StyledSpan::new(
            display_path.clone(),
            Style::default().fg(Color::Cyan),
        )];

        ComponentOutput::with_data(
            spans,
            serde_json::json!({
                "full": cwd,
                "display": display_path,
            }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn test_context(cwd: &str, home: Option<&str>) -> ComponentContext {
        let mut env = HashMap::new();
        if let Some(h) = home {
            env.insert("HOME".to_string(), h.to_string());
        }
        ComponentContext {
            cwd: std::path::PathBuf::from(cwd),
            env,
            last_exit_code: 0,
            running_jobs: 0,
            stopped_jobs: 0,
            model: None,
        }
    }

    #[test]
    fn test_pwd_in_home() {
        let pwd = PwdComponent::new();
        let ctx = test_context("/Users/test", Some("/Users/test"));
        let output = pwd.render(&ctx);

        assert!(output.visible);
        assert_eq!(output.spans[0].text, "~");
    }

    #[test]
    fn test_pwd_in_home_subdir() {
        let pwd = PwdComponent::new();
        let ctx = test_context("/Users/test/projects", Some("/Users/test"));
        let output = pwd.render(&ctx);

        assert!(output.visible);
        assert_eq!(output.spans[0].text, "~/projects");
    }

    #[test]
    fn test_pwd_outside_home() {
        let pwd = PwdComponent::new();
        let ctx = test_context("/var/log", Some("/Users/test"));
        let output = pwd.render(&ctx);

        assert!(output.visible);
        assert_eq!(output.spans[0].text, "/var/log");
    }

    #[test]
    fn test_pwd_truncation() {
        let pwd = PwdComponent::with_max_depth(3);
        let ctx = test_context("/a/b/c/d/e/f", None);
        let output = pwd.render(&ctx);

        assert!(output.spans[0].text.starts_with("..."));
        assert!(output.spans[0].text.ends_with("d/e/f"));
    }
}
