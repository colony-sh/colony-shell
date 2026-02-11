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
//! - [`composer`] — Recipe composition with import merging and dependency resolution
//! - [`skill`] — Skill trigger matching and prompt expansion
//! - [`executor`] — Command action dispatch (shell, agent, skill)
//! - [`mcp_lifecycle`] — MCP server lifecycle management
//! - [`approval`] — Tool approval policy enforcement
//! - [`version`] — SemVer constraint checking and conflict resolution
//!
//! # File Locations (Priority Order)
//!
//! 1. `.colony/recipes/` — Project-specific (highest priority)
//! 2. `~/.colony/recipes/` — User personal
//! 3. `~/.config/colony/recipes/` — User config
//! 4. `~/.cache/colony/registry/` — Downloaded from Colony Cloud

pub mod approval;
pub mod composer;
pub mod error;
pub mod executor;
pub mod loader;
pub mod mcp_lifecycle;
pub mod recipe;
pub mod registry;
pub mod skill;
pub mod version;

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

pub use approval::{ApprovalDecision, ApprovalEnforcer};
pub use composer::{compose, topological_sort, ComposedRecipe, ConflictKind, MergeConflict};
pub use executor::{
    find_command, resolve_command, AgentAction, ExecutionResult, ExecutionStatus, ResolvedAction,
    ShellAction, SkillAction,
};
pub use mcp_lifecycle::{
    build_server_command, validate_server_config, McpLifecycleManager, McpToolInfo, ManagedServer,
    ServerStatus,
};
pub use skill::{ResolvedSkill, SkillEngine, SkillMatch, TriggerKind};
pub use version::{
    detect_version_conflicts, parse_version_import, resolve_version, ResolutionStrategy, SemVer,
    VersionConflict, VersionConstraint, VersionRequest,
};
