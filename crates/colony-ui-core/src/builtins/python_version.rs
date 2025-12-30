use crate::component::{ComponentOutput, PromptComponent, TuplePattern};
use crate::context::ComponentContext;
use crate::{Color, Style, StyledSpan};
use colony_tuple::Value;
use std::fs;
use std::path::Path;

pub struct PythonVersionComponent;

impl PythonVersionComponent {
    pub fn new() -> Self {
        Self
    }

    fn detect_version(cwd: &Path) -> Option<String> {
        if let Ok(v) = fs::read_to_string(cwd.join(".python-version")) {
            return Some(v.trim().to_string());
        }

        if let Ok(content) = fs::read_to_string(cwd.join("pyproject.toml")) {
            for line in content.lines() {
                if line.contains("python") && line.contains("=") {
                    if let Some(version) = line.split('"').nth(1) {
                        return Some(version.trim_start_matches('^').to_string());
                    }
                }
            }
        }

        if cwd.join(".venv").exists() || cwd.join("venv").exists() {
            return Some("venv".to_string());
        }

        None
    }
}

impl Default for PythonVersionComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptComponent for PythonVersionComponent {
    fn id(&self) -> &str {
        "python_version"
    }

    fn subscriptions(&self) -> Vec<TuplePattern> {
        vec![vec![
            Value::Str("dir_change".to_string()),
            Value::Wildcard,
            Value::Wildcard,
        ]]
    }

    fn render(&self, ctx: &ComponentContext) -> ComponentOutput {
        match Self::detect_version(&ctx.cwd) {
            Some(version) => {
                let text = format!("🐍 {}", version);
                let spans = vec![StyledSpan::new(text, Style::default().fg(Color::Yellow))];
                ComponentOutput::with_data(
                    spans,
                    serde_json::json!({ "version": version }),
                )
            }
            None => ComponentOutput::hidden(),
        }
    }
}
