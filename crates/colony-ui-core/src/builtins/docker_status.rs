use crate::component::{ComponentOutput, PromptComponent, TuplePattern};
use crate::context::ComponentContext;
use crate::{Color, Style, StyledSpan};
use std::process::Command;

pub struct DockerStatusComponent;

impl DockerStatusComponent {
    pub fn new() -> Self {
        Self
    }

    fn get_running_count() -> Option<usize> {
        let output = Command::new("docker").args(["ps", "-q"]).output().ok()?;

        if output.status.success() {
            let count = String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter(|l| !l.is_empty())
                .count();
            Some(count)
        } else {
            None
        }
    }
}

impl Default for DockerStatusComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptComponent for DockerStatusComponent {
    fn id(&self) -> &str {
        "docker_status"
    }

    fn subscriptions(&self) -> Vec<TuplePattern> {
        vec![]
    }

    fn render(&self, _ctx: &ComponentContext) -> ComponentOutput {
        match Self::get_running_count() {
            Some(count) if count > 0 => {
                let text = format!("🐳 {}", count);
                let spans = vec![StyledSpan::new(text, Style::default().fg(Color::Cyan))];
                ComponentOutput::with_data(spans, serde_json::json!({ "running": count }))
            }
            _ => ComponentOutput::hidden(),
        }
    }
}
