//! Colony Recipe System
//!
//! Recipes are declarative configuration units that define an LLM "mode",
//! combining system prompts, tools, skills, and behaviors into a switchable
//! profile.
//!
//! # Architecture
//!
//! - [`Recipe`] — Parsed recipe.toml configuration
//! - [`RecipeLoader`] — File discovery and loading across search paths
//! - [`RecipeRegistry`] — Manages installed recipes and name resolution
//!
//! # File Locations (Priority Order)
//!
//! 1. `.colony/recipes/` — Project-specific (highest priority)
//! 2. `~/.colony/recipes/` — User personal
//! 3. `~/.config/colony/recipes/` — User config
//! 4. `~/.cache/colony/registry/` — Downloaded from Colony Cloud

pub mod error;
pub mod loader;
pub mod recipe;
pub mod registry;

pub use error::{RecipeError, Result};
pub use loader::{RecipeInfo, RecipeLoader, RecipeSource};
pub use recipe::{
    BehaviorConfig, CommandAction, CommandDef, McpServerConfig, ModelConfig, PromptConfig, Recipe,
    RecipeMeta, SkillDef, ToolApprovalConfig, ToolsConfig,
};
pub use registry::{
    validate_recipe_dir, recipe_package_files, InstalledRecipe, RecipeRegistry,
    RegistryManifest, RECIPE_OPTIONAL_FILES, RECIPE_REQUIRED_FILES,
};
