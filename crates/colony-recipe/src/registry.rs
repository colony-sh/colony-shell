//! Recipe registry for managing installed recipes
//!
//! Tracks installed recipes and provides name resolution.
//! The registry manifest is stored at ~/.colony/recipes/registry.toml.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::{RecipeError, Result};

/// Metadata about an installed recipe in the registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledRecipe {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub install_path: PathBuf,
    pub source_url: Option<String>,
    pub installed_at: String,
}

/// On-disk registry manifest (registry.toml).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RegistryManifest {
    #[serde(default)]
    pub recipes: HashMap<String, InstalledRecipe>,
}

/// Manages installed recipes.
pub struct RecipeRegistry {
    manifest_path: PathBuf,
    manifest: RegistryManifest,
}

impl RecipeRegistry {
    /// Open or create the registry at the default location (~/.colony/recipes/registry.toml).
    pub fn open() -> Result<Self> {
        let recipes_dir = Self::recipes_dir()?;
        let manifest_path = recipes_dir.join("registry.toml");
        Self::open_at(manifest_path)
    }

    /// Open or create the registry at a specific path.
    pub fn open_at(manifest_path: PathBuf) -> Result<Self> {
        let manifest = if manifest_path.exists() {
            let content = std::fs::read_to_string(&manifest_path).map_err(|e| RecipeError::Io {
                path: manifest_path.display().to_string(),
                source: e,
            })?;
            toml::from_str(&content).map_err(|e| RecipeError::TomlParse {
                path: manifest_path.display().to_string(),
                source: e,
            })?
        } else {
            RegistryManifest::default()
        };

        Ok(Self {
            manifest_path,
            manifest,
        })
    }

    /// Register an installed recipe.
    pub fn register(&mut self, entry: InstalledRecipe) -> Result<()> {
        self.manifest.recipes.insert(entry.name.clone(), entry);
        self.save()
    }

    /// Unregister a recipe by name.
    pub fn unregister(&mut self, name: &str) -> Result<Option<InstalledRecipe>> {
        let removed = self.manifest.recipes.remove(name);
        if removed.is_some() {
            self.save()?;
        }
        Ok(removed)
    }

    /// Look up a recipe by name.
    pub fn get(&self, name: &str) -> Option<&InstalledRecipe> {
        self.manifest.recipes.get(name)
    }

    /// List all installed recipes.
    pub fn list(&self) -> Vec<&InstalledRecipe> {
        let mut recipes: Vec<&InstalledRecipe> = self.manifest.recipes.values().collect();
        recipes.sort_by(|a, b| a.name.cmp(&b.name));
        recipes
    }

    /// Check if a recipe is installed.
    pub fn is_installed(&self, name: &str) -> bool {
        self.manifest.recipes.contains_key(name)
    }

    /// Save the manifest to disk.
    fn save(&self) -> Result<()> {
        if let Some(parent) = self.manifest_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| RecipeError::Io {
                path: parent.display().to_string(),
                source: e,
            })?;
        }

        let content = toml::to_string_pretty(&self.manifest).map_err(|e| {
            RecipeError::Invalid(format!("failed to serialize registry manifest: {e}"))
        })?;

        std::fs::write(&self.manifest_path, content).map_err(|e| RecipeError::Io {
            path: self.manifest_path.display().to_string(),
            source: e,
        })
    }

    /// Return the default recipes directory (~/.colony/recipes/).
    pub fn recipes_dir() -> Result<PathBuf> {
        let home = dirs::home_dir().ok_or_else(|| RecipeError::Io {
            path: "~".to_string(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "HOME directory not found"),
        })?;
        Ok(home.join(".colony/recipes"))
    }
}

/// Required files that must exist in a valid recipe package.
pub const RECIPE_REQUIRED_FILES: &[&str] = &["AGENT.md"];

/// Optional files that may exist in a recipe package.
pub const RECIPE_OPTIONAL_FILES: &[&str] = &["TOOLS.md", "CONTEXT.md", "PROMPT.md", "recipe.toml"];

/// Validate that a directory contains a valid recipe structure.
///
/// A valid recipe must have at least an AGENT.md file.
pub fn validate_recipe_dir(dir: &Path) -> Result<()> {
    if !dir.is_dir() {
        return Err(RecipeError::Invalid(format!(
            "not a directory: {}",
            dir.display()
        )));
    }

    let has_agent_md = dir.join("AGENT.md").exists();
    let has_recipe_toml = dir.join("recipe.toml").exists();

    if !has_agent_md && !has_recipe_toml {
        return Err(RecipeError::MissingFile(
            "recipe must contain at least AGENT.md or recipe.toml".to_string(),
        ));
    }

    Ok(())
}

/// List all files in a recipe package directory that are part of the recipe.
pub fn recipe_package_files(dir: &Path) -> Vec<PathBuf> {
    let special_files = [
        "AGENT.md",
        "TOOLS.md",
        "CONTEXT.md",
        "PROMPT.md",
        "HEARTBEAT.md",
        "recipe.toml",
    ];

    let mut files = Vec::new();
    for name in &special_files {
        let path = dir.join(name);
        if path.exists() {
            files.push(path);
        }
    }

    let scripts_dir = dir.join("scripts");
    if scripts_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&scripts_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    files.push(path);
                }
            }
        }
    }

    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_open_create() {
        let dir = tempfile::tempdir().unwrap();
        let manifest_path = dir.path().join("registry.toml");
        let registry = RecipeRegistry::open_at(manifest_path).unwrap();
        assert!(registry.list().is_empty());
    }

    #[test]
    fn test_registry_register_and_list() {
        let dir = tempfile::tempdir().unwrap();
        let manifest_path = dir.path().join("registry.toml");
        let mut registry = RecipeRegistry::open_at(manifest_path).unwrap();

        registry
            .register(InstalledRecipe {
                name: "test-recipe".to_string(),
                version: "1.0.0".to_string(),
                description: Some("A test recipe".to_string()),
                install_path: dir.path().join("test-recipe"),
                source_url: None,
                installed_at: "2026-02-10T00:00:00Z".to_string(),
            })
            .unwrap();

        assert_eq!(registry.list().len(), 1);
        assert!(registry.is_installed("test-recipe"));
        assert!(!registry.is_installed("other"));
    }

    #[test]
    fn test_registry_unregister() {
        let dir = tempfile::tempdir().unwrap();
        let manifest_path = dir.path().join("registry.toml");
        let mut registry = RecipeRegistry::open_at(manifest_path).unwrap();

        registry
            .register(InstalledRecipe {
                name: "test".to_string(),
                version: "1.0.0".to_string(),
                description: None,
                install_path: dir.path().join("test"),
                source_url: None,
                installed_at: "2026-02-10T00:00:00Z".to_string(),
            })
            .unwrap();

        let removed = registry.unregister("test").unwrap();
        assert!(removed.is_some());
        assert!(!registry.is_installed("test"));
    }

    #[test]
    fn test_registry_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let manifest_path = dir.path().join("registry.toml");

        {
            let mut registry = RecipeRegistry::open_at(manifest_path.clone()).unwrap();
            registry
                .register(InstalledRecipe {
                    name: "persisted".to_string(),
                    version: "2.0.0".to_string(),
                    description: None,
                    install_path: dir.path().join("persisted"),
                    source_url: Some("https://example.com/persisted".to_string()),
                    installed_at: "2026-02-10T00:00:00Z".to_string(),
                })
                .unwrap();
        }

        let registry = RecipeRegistry::open_at(manifest_path).unwrap();
        assert!(registry.is_installed("persisted"));
        let entry = registry.get("persisted").unwrap();
        assert_eq!(entry.version, "2.0.0");
    }

    #[test]
    fn test_validate_recipe_dir_with_agent_md() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("AGENT.md"), "# Agent\n").unwrap();
        assert!(validate_recipe_dir(dir.path()).is_ok());
    }

    #[test]
    fn test_validate_recipe_dir_with_recipe_toml() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("recipe.toml"), "[recipe]\nname = \"t\"\n").unwrap();
        assert!(validate_recipe_dir(dir.path()).is_ok());
    }

    #[test]
    fn test_validate_recipe_dir_empty() {
        let dir = tempfile::tempdir().unwrap();
        let result = validate_recipe_dir(dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_recipe_package_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("AGENT.md"), "agent").unwrap();
        std::fs::write(dir.path().join("TOOLS.md"), "tools").unwrap();
        std::fs::write(dir.path().join("recipe.toml"), "toml").unwrap();
        std::fs::write(dir.path().join("unrelated.txt"), "other").unwrap();

        let files = recipe_package_files(dir.path());
        assert_eq!(files.len(), 3);
    }
}
