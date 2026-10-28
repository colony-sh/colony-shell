//! Recipe-driven agent configuration (P5-37)
//!
//! Maps recipe fields to `AgentConfig` so that switching recipes reconfigures
//! the agent. User overrides take priority over recipe settings.

#[cfg(feature = "recipe")]
mod inner {
    use crate::agent::AgentConfig;
    use crate::router::ProviderType;
    use colony_recipe::Recipe;
    use tracing::debug;

    /// Configuration derived from a recipe, ready to apply to an agent
    #[derive(Debug, Clone)]
    pub struct RecipeAgentConfig {
        pub system_prompt: Option<String>,
        pub model_provider: Option<ProviderType>,
        pub model_name: Option<String>,
        pub temperature: Option<f32>,
        pub max_turns: Option<u32>,
        pub auto_approve_patterns: Vec<String>,
        pub deny_patterns: Vec<String>,
        pub streaming: Option<bool>,
        pub auto_continue: Option<bool>,
    }

    impl RecipeAgentConfig {
        /// Extract agent configuration from a recipe
        pub fn from_recipe(recipe: &Recipe) -> Self {
            let model_provider = recipe
                .model
                .provider
                .as_deref()
                .and_then(ProviderType::parse);

            Self {
                system_prompt: recipe.prompt.system.clone(),
                model_provider,
                model_name: recipe.model.name.clone(),
                temperature: recipe.model.temperature,
                max_turns: recipe.behavior.max_turns,
                auto_approve_patterns: recipe.tools.approval.auto_approve.clone(),
                deny_patterns: recipe.tools.approval.deny.clone(),
                streaming: recipe.behavior.streaming,
                auto_continue: recipe.behavior.auto_continue,
            }
        }

        /// Merge this recipe config with user overrides into an `AgentConfig`
        ///
        /// User overrides take priority: any field set in `user_overrides` will
        /// win over the recipe value.
        pub fn merge_with_overrides(&self, user_overrides: &AgentConfigOverrides) -> AgentConfig {
            let mut config = AgentConfig::default();

            // Apply recipe values via the shared helper
            self.apply_to(&mut config);

            // User overrides win
            user_overrides.apply_to(&mut config);

            config
        }

        /// Apply this recipe config's values onto an existing `AgentConfig`
        fn apply_to(&self, config: &mut AgentConfig) {
            if let Some(ref prompt) = self.system_prompt {
                config.system_prompt = prompt.clone();
            }
            if let Some(temp) = self.temperature {
                config.temperature = Some(temp);
            }
            if !self.auto_approve_patterns.is_empty() {
                config.auto_approve_patterns = self.auto_approve_patterns.clone();
            }
        }

        /// Check if this recipe config specifies a model change
        pub fn has_model_change(&self) -> bool {
            self.model_provider.is_some() || self.model_name.is_some()
        }

        /// Get the provider and model name for switching, if specified
        pub fn model_spec(&self) -> Option<(ProviderType, String)> {
            let provider = self.model_provider?;
            let name = self
                .model_name
                .clone()
                .unwrap_or_else(|| provider.default_model().to_string());
            Some((provider, name))
        }
    }

    /// User-level overrides that take priority over recipe settings
    #[derive(Debug, Clone, Default)]
    pub struct AgentConfigOverrides {
        pub system_prompt: Option<String>,
        pub max_tokens: Option<u32>,
        pub temperature: Option<f32>,
        pub use_caching: Option<bool>,
        pub persistence_enabled: Option<bool>,
        pub memory_enabled: Option<bool>,
        pub auto_approve_patterns: Option<Vec<String>>,
    }

    impl AgentConfigOverrides {
        /// Apply overrides onto an existing `AgentConfig`
        fn apply_to(&self, config: &mut AgentConfig) {
            if let Some(ref prompt) = self.system_prompt {
                config.system_prompt = prompt.clone();
            }
            if let Some(max_tokens) = self.max_tokens {
                config.max_tokens = max_tokens;
            }
            if self.temperature.is_some() {
                config.temperature = self.temperature;
            }
            if let Some(caching) = self.use_caching {
                config.use_caching = caching;
            }
            if let Some(persistence) = self.persistence_enabled {
                config.persistence_enabled = persistence;
            }
            if let Some(memory) = self.memory_enabled {
                config.memory_enabled = memory;
            }
            if let Some(ref patterns) = self.auto_approve_patterns {
                config.auto_approve_patterns = patterns.clone();
            }
        }

        pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
            self.system_prompt = Some(prompt.into());
            self
        }

        pub fn with_max_tokens(mut self, tokens: u32) -> Self {
            self.max_tokens = Some(tokens);
            self
        }

        pub fn with_temperature(mut self, temp: f32) -> Self {
            self.temperature = Some(temp);
            self
        }
    }

    /// Apply a recipe's configuration to an existing `AgentConfig`
    ///
    /// Only sets fields that the recipe explicitly specifies.
    /// Does not clear fields that the recipe omits.
    pub fn apply_recipe_to_config(recipe: &Recipe, config: &mut AgentConfig) {
        if let Some(ref prompt) = recipe.prompt.system {
            config.system_prompt = prompt.clone();
        }

        if let Some(temp) = recipe.model.temperature {
            config.temperature = Some(temp);
        }

        if !recipe.tools.approval.auto_approve.is_empty() {
            config.auto_approve_patterns = recipe.tools.approval.auto_approve.clone();
        }

        debug!(
            recipe = %recipe.meta.name,
            "Applied recipe configuration to agent"
        );
    }

    /// Diff two recipe configs to detect what changed
    #[derive(Debug, Clone)]
    pub struct RecipeConfigDiff {
        pub system_prompt_changed: bool,
        pub model_changed: bool,
        pub temperature_changed: bool,
        pub approval_changed: bool,
    }

    impl RecipeConfigDiff {
        pub fn diff(old: &RecipeAgentConfig, new: &RecipeAgentConfig) -> Self {
            Self {
                system_prompt_changed: old.system_prompt != new.system_prompt,
                model_changed: old.model_provider != new.model_provider
                    || old.model_name != new.model_name,
                temperature_changed: old.temperature != new.temperature,
                approval_changed: old.auto_approve_patterns != new.auto_approve_patterns
                    || old.deny_patterns != new.deny_patterns,
            }
        }

        pub fn has_changes(&self) -> bool {
            self.system_prompt_changed
                || self.model_changed
                || self.temperature_changed
                || self.approval_changed
        }
    }
}

#[cfg(feature = "recipe")]
pub use inner::*;

#[cfg(not(feature = "recipe"))]
pub mod stub {
    #[derive(Debug, Clone, Default)]
    pub struct RecipeConfigStub;
}

#[cfg(not(feature = "recipe"))]
pub use stub::*;

#[cfg(test)]
#[cfg(feature = "recipe")]
mod tests {
    use super::*;
    use colony_recipe::Recipe;
    use std::path::PathBuf;

    fn test_recipe(toml: &str) -> Recipe {
        Recipe::from_toml(toml, &PathBuf::from("test.toml")).expect("valid recipe TOML")
    }

    fn minimal_recipe() -> Recipe {
        test_recipe(
            r#"
[recipe]
name = "test"
"#,
        )
    }

    fn full_recipe() -> Recipe {
        test_recipe(
            r#"
[recipe]
name = "coding-agent"
version = "1.0.0"

[prompt]
system = "You are an expert coder."

[model]
provider = "anthropic"
name = "claude-3-5-sonnet-20241022"
temperature = 0.3

[tools.approval]
auto_approve = ["file_read", "search"]
deny = ["rm_rf"]

[behavior]
streaming = true
auto_continue = true
max_turns = 25
"#,
        )
    }

    #[test]
    fn test_from_recipe_minimal() {
        let recipe = minimal_recipe();
        let config = RecipeAgentConfig::from_recipe(&recipe);

        assert!(config.system_prompt.is_none());
        assert!(config.model_provider.is_none());
        assert!(config.model_name.is_none());
        assert!(config.temperature.is_none());
        assert!(config.auto_approve_patterns.is_empty());
        assert!(config.deny_patterns.is_empty());
    }

    #[test]
    fn test_from_recipe_full() {
        let recipe = full_recipe();
        let config = RecipeAgentConfig::from_recipe(&recipe);

        assert_eq!(
            config.system_prompt.as_deref(),
            Some("You are an expert coder.")
        );
        assert_eq!(
            config.model_provider,
            Some(crate::router::ProviderType::Anthropic)
        );
        assert_eq!(
            config.model_name.as_deref(),
            Some("claude-3-5-sonnet-20241022")
        );
        assert_eq!(config.temperature, Some(0.3));
        assert_eq!(config.max_turns, Some(25));
        assert_eq!(config.auto_approve_patterns, vec!["file_read", "search"]);
        assert_eq!(config.deny_patterns, vec!["rm_rf"]);
        assert_eq!(config.streaming, Some(true));
        assert_eq!(config.auto_continue, Some(true));
    }

    #[test]
    fn test_merge_recipe_only() {
        let recipe = full_recipe();
        let recipe_config = RecipeAgentConfig::from_recipe(&recipe);
        let overrides = AgentConfigOverrides::default();

        let agent_config = recipe_config.merge_with_overrides(&overrides);

        assert_eq!(agent_config.system_prompt, "You are an expert coder.");
        assert_eq!(agent_config.temperature, Some(0.3));
        assert_eq!(
            agent_config.auto_approve_patterns,
            vec!["file_read", "search"]
        );
    }

    #[test]
    fn test_merge_user_overrides_win() {
        let recipe = full_recipe();
        let recipe_config = RecipeAgentConfig::from_recipe(&recipe);

        let overrides = AgentConfigOverrides::default()
            .with_system_prompt("My custom prompt")
            .with_temperature(0.9)
            .with_max_tokens(8192);

        let agent_config = recipe_config.merge_with_overrides(&overrides);

        assert_eq!(agent_config.system_prompt, "My custom prompt");
        assert_eq!(agent_config.temperature, Some(0.9));
        assert_eq!(agent_config.max_tokens, 8192);
        // Recipe patterns still apply since user didn't override them
        assert_eq!(
            agent_config.auto_approve_patterns,
            vec!["file_read", "search"]
        );
    }

    #[test]
    fn test_merge_user_overrides_approval() {
        let recipe = full_recipe();
        let recipe_config = RecipeAgentConfig::from_recipe(&recipe);

        let mut overrides = AgentConfigOverrides::default();
        overrides.auto_approve_patterns = Some(vec!["*".to_string()]);

        let agent_config = recipe_config.merge_with_overrides(&overrides);
        assert_eq!(agent_config.auto_approve_patterns, vec!["*"]);
    }

    #[test]
    fn test_has_model_change() {
        let minimal = RecipeAgentConfig::from_recipe(&minimal_recipe());
        assert!(!minimal.has_model_change());

        let full = RecipeAgentConfig::from_recipe(&full_recipe());
        assert!(full.has_model_change());
    }

    #[test]
    fn test_model_spec() {
        let full = RecipeAgentConfig::from_recipe(&full_recipe());
        let (provider, name) = full.model_spec().expect("model spec should exist");
        assert_eq!(provider, crate::router::ProviderType::Anthropic);
        assert_eq!(name, "claude-3-5-sonnet-20241022");
    }

    #[test]
    fn test_model_spec_default_model() {
        let recipe = test_recipe(
            r#"
[recipe]
name = "test"

[model]
provider = "ollama"
"#,
        );
        let config = RecipeAgentConfig::from_recipe(&recipe);
        let (provider, name) = config.model_spec().expect("spec");
        assert_eq!(provider, crate::router::ProviderType::Ollama);
        assert_eq!(name, "llama3");
    }

    #[test]
    fn test_apply_recipe_to_config() {
        let recipe = full_recipe();
        let mut config = crate::agent::AgentConfig::default();

        apply_recipe_to_config(&recipe, &mut config);

        assert_eq!(config.system_prompt, "You are an expert coder.");
        assert_eq!(config.temperature, Some(0.3));
        assert_eq!(config.auto_approve_patterns, vec!["file_read", "search"]);
    }

    #[test]
    fn test_apply_recipe_preserves_unset() {
        let recipe = minimal_recipe();
        let mut config = crate::agent::AgentConfig {
            max_tokens: 8192,
            system_prompt: "original prompt".to_string(),
            temperature: Some(0.5),
            ..Default::default()
        };

        apply_recipe_to_config(&recipe, &mut config);

        // Recipe didn't set these, so they're preserved
        assert_eq!(config.max_tokens, 8192);
        assert_eq!(config.system_prompt, "original prompt");
        assert_eq!(config.temperature, Some(0.5));
    }

    #[test]
    fn test_recipe_config_diff_no_changes() {
        let recipe = full_recipe();
        let config1 = RecipeAgentConfig::from_recipe(&recipe);
        let config2 = RecipeAgentConfig::from_recipe(&recipe);

        let diff = RecipeConfigDiff::diff(&config1, &config2);
        assert!(!diff.has_changes());
    }

    #[test]
    fn test_recipe_config_diff_with_changes() {
        let config1 = RecipeAgentConfig::from_recipe(&full_recipe());
        let config2 = RecipeAgentConfig::from_recipe(&test_recipe(
            r#"
[recipe]
name = "different"

[prompt]
system = "Different prompt."

[model]
provider = "openai"
name = "gpt-4-turbo"
temperature = 0.8
"#,
        ));

        let diff = RecipeConfigDiff::diff(&config1, &config2);
        assert!(diff.has_changes());
        assert!(diff.system_prompt_changed);
        assert!(diff.model_changed);
        assert!(diff.temperature_changed);
        assert!(diff.approval_changed);
    }

    #[test]
    fn test_recipe_config_diff_partial() {
        let config1 = RecipeAgentConfig::from_recipe(&full_recipe());
        let mut config2 = config1.clone();
        config2.temperature = Some(0.99);

        let diff = RecipeConfigDiff::diff(&config1, &config2);
        assert!(diff.has_changes());
        assert!(diff.temperature_changed);
        assert!(!diff.system_prompt_changed);
        assert!(!diff.model_changed);
    }

    #[test]
    fn test_overrides_builder() {
        let overrides = AgentConfigOverrides::default()
            .with_system_prompt("custom")
            .with_max_tokens(2048)
            .with_temperature(0.1);

        assert_eq!(overrides.system_prompt.as_deref(), Some("custom"));
        assert_eq!(overrides.max_tokens, Some(2048));
        assert_eq!(overrides.temperature, Some(0.1));
    }
}
