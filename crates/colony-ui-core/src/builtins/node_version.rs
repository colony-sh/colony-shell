use crate::component::{ComponentOutput, PromptComponent, TuplePattern};
use crate::context::ComponentContext;
use crate::{Color, Style, StyledSpan};
use colony_tuple::Value;
use std::fs;
use std::path::Path;

pub struct NodeVersionComponent;

impl NodeVersionComponent {
    pub fn new() -> Self {
        Self
    }

    fn detect_version(cwd: &Path) -> Option<String> {
        if let Ok(v) = fs::read_to_string(cwd.join(".nvmrc")) {
            return Some(v.trim().to_string());
        }

        if let Ok(v) = fs::read_to_string(cwd.join(".node-version")) {
            return Some(v.trim().to_string());
        }

        if let Ok(content) = fs::read_to_string(cwd.join("package.json")) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(engines) = json.get("engines") {
                    if let Some(node) = engines.get("node").and_then(|v| v.as_str()) {
                        return Some(node.to_string());
                    }
                }
            }
        }

        if cwd.join("package.json").exists() {
            return Some("node".to_string());
        }

        None
    }
}

impl Default for NodeVersionComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptComponent for NodeVersionComponent {
    fn id(&self) -> &str {
        "node_version"
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
                let text = format!("⬢ {}", version);
                let spans = vec![StyledSpan::new(text, Style::default().fg(Color::Green))];
                ComponentOutput::with_data(
                    spans,
                    serde_json::json!({ "version": version }),
                )
            }
            None => ComponentOutput::hidden(),
        }
    }
}
