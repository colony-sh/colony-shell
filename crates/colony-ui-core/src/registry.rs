use crate::builtins::{
    ExitStatusComponent, GitBranchComponent, GitStatusComponent, JobsComponent, ModelComponent,
    PwdComponent,
};
use crate::component::PromptComponent;
use std::collections::HashMap;

pub struct ComponentRegistry {
    components: HashMap<String, Box<dyn PromptComponent>>,
}

impl ComponentRegistry {
    pub fn new() -> Self {
        Self {
            components: HashMap::new(),
        }
    }

    pub fn register_builtins(&mut self) {
        self.register(Box::new(PwdComponent::new()));
        self.register(Box::new(GitBranchComponent::new()));
        self.register(Box::new(GitStatusComponent::new()));
        self.register(Box::new(JobsComponent::new()));
        self.register(Box::new(ExitStatusComponent::new()));
        self.register(Box::new(ModelComponent::new()));
    }

    pub fn register(&mut self, component: Box<dyn PromptComponent>) {
        self.components.insert(component.id().to_string(), component);
    }

    pub fn get(&self, id: &str) -> Option<&dyn PromptComponent> {
        self.components.get(id).map(|c| c.as_ref())
    }

    pub fn all(&self) -> Vec<&dyn PromptComponent> {
        self.components.values().map(|c| c.as_ref()).collect()
    }

    pub fn contains(&self, id: &str) -> bool {
        self.components.contains_key(id)
    }
}

impl Default for ComponentRegistry {
    fn default() -> Self {
        let mut registry = Self::new();
        registry.register_builtins();
        registry
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_builtins() {
        let mut registry = ComponentRegistry::new();
        registry.register_builtins();

        assert!(registry.get("pwd").is_some());
        assert!(registry.get("git_branch").is_some());
        assert!(registry.get("exit_status").is_some());
        assert!(registry.get("jobs").is_some());
        assert!(registry.get("model").is_some());
        assert!(registry.get("git_status").is_some());
    }

    #[test]
    fn test_registry_get_unknown() {
        let registry = ComponentRegistry::new();
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_registry_all() {
        let mut registry = ComponentRegistry::new();
        registry.register_builtins();

        let all = registry.all();
        assert!(all.len() >= 6);
    }
}
