use crate::component::{ComponentOutput, PromptComponent, TuplePattern};
use crate::context::ComponentContext;
use crate::{Color, Style, StyledSpan};
use colony_tuple::Value;
use std::fs;
use std::path::Path;

pub struct RustVersionComponent;

impl RustVersionComponent {
    pub fn new() -> Self {
        Self
    }

    fn detect_version(cwd: &Path) -> Option<String> {
        if let Ok(content) = fs::read_to_string(cwd.join("rust-toolchain.toml")) {
            for line in content.lines() {
                if line.contains("channel") {
                    if let Some(version) = line.split('"').nth(1) {
                        return Some(version.to_string());
                    }
                }
            }
        }

        if let Ok(v) = fs::read_to_string(cwd.join("rust-toolchain")) {
            return Some(v.trim().to_string());
        }

        if cwd.join("Cargo.toml").exists() {
            return Some("rust".to_string());
        }

        None
    }
}

impl Default for RustVersionComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptComponent for RustVersionComponent {
    fn id(&self) -> &str {
        "rust_version"
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
                let text = format!("🦀 {}", version);
                let spans = vec![StyledSpan::new(
                    text,
                    Style::default().fg(Color::Rgb(222, 165, 132)),
                )];
                ComponentOutput::with_data(spans, serde_json::json!({ "version": version }))
            }
            None => ComponentOutput::hidden(),
        }
    }
}
