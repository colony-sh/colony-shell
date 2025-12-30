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

    pub fn registry_mut(&mut self) -> &mut ComponentRegistry {
        &mut self.registry
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
