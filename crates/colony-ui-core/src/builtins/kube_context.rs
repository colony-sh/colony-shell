use crate::component::{ComponentOutput, PromptComponent, TuplePattern};
use crate::context::ComponentContext;
use crate::{Color, Style, StyledSpan};
use std::process::Command;

pub struct KubeContextComponent;

impl KubeContextComponent {
    pub fn new() -> Self {
        Self
    }

    fn get_current_context() -> Option<String> {
        let output = Command::new("kubectl")
            .args(["config", "current-context"])
            .output()
            .ok()?;

        if output.status.success() {
            Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            None
        }
    }
}

impl Default for KubeContextComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptComponent for KubeContextComponent {
    fn id(&self) -> &str {
        "kube_context"
    }

    fn subscriptions(&self) -> Vec<TuplePattern> {
        vec![]
    }

    fn render(&self, _ctx: &ComponentContext) -> ComponentOutput {
        match Self::get_current_context() {
            Some(context) => {
                let text = format!("⎈ {}", context);
                let spans = vec![StyledSpan::new(text, Style::default().fg(Color::Blue))];
                ComponentOutput::with_data(spans, serde_json::json!({ "context": context }))
            }
            None => ComponentOutput::hidden(),
        }
    }
}
