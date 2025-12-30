use crate::component::{Color, ComponentOutput};
use crate::context::ComponentContext;
use crate::registry::ComponentRegistry;
use crate::render::ansi::render_prompt;
use crate::subscription::SubscriptionManager;

pub struct PromptManager {
    registry: ComponentRegistry,
    subscription_manager: SubscriptionManager,
    left_components: Vec<String>,
    right_components: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PromptData {
    pub left: Vec<ComponentData>,
    pub right: Vec<ComponentData>,
}

#[derive(Debug, Clone)]
pub struct ComponentData {
    pub id: String,
    pub output: ComponentOutput,
}

impl PromptManager {
    pub fn new(left: Vec<String>, right: Vec<String>) -> Self {
        let registry = ComponentRegistry::default();
        let mut subscription_manager = SubscriptionManager::new();

        for component in registry.all() {
            subscription_manager.register(component);
        }

        Self {
            registry,
            subscription_manager,
            left_components: left,
            right_components: right,
        }
    }

    pub fn on_event(&mut self, event_type: &str) {
        self.subscription_manager.on_event(event_type);
    }

    pub fn render(&self, ctx: &ComponentContext, width: usize) -> String {
        let left_outputs: Vec<_> = self
            .left_components
            .iter()
            .filter_map(|id| self.registry.get(id))
            .map(|c| c.render(ctx))
            .collect();

        let right_outputs: Vec<_> = self
            .right_components
            .iter()
            .filter_map(|id| self.registry.get(id))
            .map(|c| c.render(ctx))
            .collect();

        render_prompt(&left_outputs, &right_outputs, width)
    }

    pub fn render_data(&self, ctx: &ComponentContext) -> PromptData {
        let left: Vec<_> = self
            .left_components
            .iter()
            .filter_map(|id| {
                let component = self.registry.get(id)?;
                let output = component.render(ctx);
                if output.visible {
                    Some(ComponentData {
                        id: id.clone(),
                        output,
                    })
                } else {
                    None
                }
            })
            .collect();

        let right: Vec<_> = self
            .right_components
            .iter()
            .filter_map(|id| {
                let component = self.registry.get(id)?;
                let output = component.render(ctx);
                if output.visible {
                    Some(ComponentData {
                        id: id.clone(),
                        output,
                    })
                } else {
                    None
                }
            })
            .collect();

        PromptData { left, right }
    }

    pub fn registry_mut(&mut self) -> &mut ComponentRegistry {
        &mut self.registry
    }
}

pub fn color_to_string(c: &Color) -> String {
    match c {
        Color::Black => "black".to_string(),
        Color::Red => "red".to_string(),
        Color::Green => "green".to_string(),
        Color::Yellow => "yellow".to_string(),
        Color::Blue => "blue".to_string(),
        Color::Magenta => "magenta".to_string(),
        Color::Cyan => "cyan".to_string(),
        Color::White => "white".to_string(),
        Color::BrightBlack => "bright-black".to_string(),
        Color::BrightRed => "bright-red".to_string(),
        Color::BrightGreen => "bright-green".to_string(),
        Color::BrightYellow => "bright-yellow".to_string(),
        Color::BrightBlue => "bright-blue".to_string(),
        Color::BrightMagenta => "bright-magenta".to_string(),
        Color::BrightCyan => "bright-cyan".to_string(),
        Color::BrightWhite => "bright-white".to_string(),
        Color::Rgb(r, g, b) => format!("#{:02x}{:02x}{:02x}", r, g, b),
    }
}

impl Default for PromptManager {
    fn default() -> Self {
        Self::new(
            vec![
                "pwd".to_string(),
                "git_branch".to_string(),
                "git_status".to_string(),
            ],
            vec![
                "model".to_string(),
                "jobs".to_string(),
                "exit_status".to_string(),
            ],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn test_context() -> ComponentContext {
        ComponentContext {
            cwd: std::path::PathBuf::from("/home/test/project"),
            env: {
                let mut env = HashMap::new();
                env.insert("HOME".to_string(), "/home/test".to_string());
                env
            },
            last_exit_code: 0,
            running_jobs: 0,
            stopped_jobs: 0,
            model: None,
            last_command_duration: None,
            hostname: None,
        }
    }

    #[test]
    fn test_prompt_manager_default() {
        let manager = PromptManager::default();
        let ctx = test_context();
        let prompt = manager.render(&ctx, 80);

        assert!(prompt.contains("~/project") || prompt.contains("/home/test/project"));
        assert!(prompt.contains(">"));
    }

    #[test]
    fn test_prompt_manager_custom_layout() {
        let manager = PromptManager::new(
            vec!["pwd".to_string()],
            vec!["exit_status".to_string()],
        );
        let ctx = test_context();
        let prompt = manager.render(&ctx, 80);

        assert!(prompt.contains(">"));
    }
}
