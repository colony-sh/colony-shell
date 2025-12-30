use crate::component::{ComponentOutput, PromptComponent};
use crate::context::ComponentContext;
use std::collections::{HashMap, HashSet};

pub struct SubscriptionManager {
    subscriptions: HashMap<String, HashSet<String>>,
    cache: HashMap<String, ComponentOutput>,
}

impl SubscriptionManager {
    pub fn new() -> Self {
        Self {
            subscriptions: HashMap::new(),
            cache: HashMap::new(),
        }
    }

    pub fn register<C: PromptComponent>(&mut self, component: &C) {
        let component_id = component.id().to_string();

        for pattern in component.subscriptions() {
            if let Some(first) = pattern.first() {
                if let colony_tuple::Value::Str(event_type) = first {
                    self.subscriptions
                        .entry(event_type.clone())
                        .or_default()
                        .insert(component_id.clone());
                }
            }
        }
    }

    pub fn on_event(&mut self, event_type: &str) {
        if let Some(component_ids) = self.subscriptions.get(event_type) {
            for id in component_ids.clone() {
                self.cache.remove(&id);
            }
        }
    }

    pub fn get_output<C: PromptComponent>(
        &mut self,
        component: &C,
        ctx: &ComponentContext,
    ) -> ComponentOutput {
        let id = component.id().to_string();

        if let Some(cached) = self.cache.get(&id) {
            return cached.clone();
        }

        let output = component.render(ctx);
        self.cache.insert(id, output.clone());
        output
    }

    pub fn invalidate_all(&mut self) {
        self.cache.clear();
    }
}

impl Default for SubscriptionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PwdComponent;
    use std::collections::HashMap as StdHashMap;

    fn test_context() -> ComponentContext {
        ComponentContext {
            cwd: std::path::PathBuf::from("/home/test"),
            env: {
                let mut env = StdHashMap::new();
                env.insert("HOME".to_string(), "/home/test".to_string());
                env
            },
            last_exit_code: 0,
            running_jobs: 0,
            stopped_jobs: 0,
            model: None,
        }
    }

    #[test]
    fn test_register_component() {
        let mut manager = SubscriptionManager::new();
        let pwd = PwdComponent::new();

        manager.register(&pwd);

        assert!(manager.subscriptions.contains_key("dir_change"));
    }

    #[test]
    fn test_cache_invalidation() {
        let mut manager = SubscriptionManager::new();
        let pwd = PwdComponent::new();

        manager.register(&pwd);

        let ctx = test_context();
        let _ = manager.get_output(&pwd, &ctx);

        assert!(manager.cache.contains_key("pwd"));

        manager.on_event("dir_change");

        assert!(!manager.cache.contains_key("pwd"));
    }

    #[test]
    fn test_cache_hit() {
        let mut manager = SubscriptionManager::new();
        let pwd = PwdComponent::new();

        manager.register(&pwd);

        let ctx = test_context();

        let out1 = manager.get_output(&pwd, &ctx);
        let out2 = manager.get_output(&pwd, &ctx);

        assert_eq!(out1.spans.len(), out2.spans.len());
    }
}
