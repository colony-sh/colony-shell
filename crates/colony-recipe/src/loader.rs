//! Recipe file discovery and loading
//!
//! Discovers recipes from multiple locations in priority order:
//! 1. .colony/recipes/ (project-specific, highest priority)
//! 2. ~/.colony/recipes/ (user personal)
//! 3. ~/.config/colony/recipes/ (user config)
//! 4. ~/.cache/colony/registry/ (downloaded from registry)

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::{RecipeError, Result};
use crate::recipe::Recipe;

/// Information about a discovered recipe (without loading the full TOML).
#[derive(Debug, Clone)]
pub struct RecipeInfo {
    pub name: String,
    pub path: PathBuf,
    pub source: RecipeSource,
}

/// Where a recipe was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecipeSource {
    Project,
    User,
    Config,
    Registry,
}

impl std::fmt::Display for RecipeSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Project => "project",
            Self::User => "user",
            Self::Config => "config",
            Self::Registry => "registry",
        };
        f.write_str(s)
    }
}

/// Discovers and loads recipes from the filesystem.
pub struct RecipeLoader {
    search_paths: Vec<(PathBuf, RecipeSource)>,
    cache: HashMap<String, Recipe>,
}

impl RecipeLoader {
    /// Create a new loader that searches standard recipe locations.
    pub fn new() -> Self {
        let mut paths = Vec::new();

        if let Ok(cwd) = std::env::current_dir() {
            paths.push((cwd.join(".colony/recipes"), RecipeSource::Project));
        }

        if let Some(home) = dirs::home_dir() {
            paths.push((home.join(".colony/recipes"), RecipeSource::User));
        }

        if let Some(config) = dirs::config_dir() {
            paths.push((config.join("colony/recipes"), RecipeSource::Config));
        }

        if let Some(cache) = dirs::cache_dir() {
            paths.push((cache.join("colony/registry"), RecipeSource::Registry));
        }

        Self {
            search_paths: paths,
            cache: HashMap::new(),
        }
    }

    /// Discover all available recipes across search paths.
    ///
    /// Returns recipes in priority order (project > user > config > registry).
    /// Duplicate names are resolved by priority: first found wins.
    pub fn discover(&self) -> Vec<RecipeInfo> {
        let mut seen: HashMap<String, RecipeInfo> = HashMap::new();

        for (dir, source) in &self.search_paths {
            let entries = match std::fs::read_dir(dir) {
                Ok(e) => e,
                Err(_) => continue,
            };

            for entry in entries.flatten() {
                let path = entry.path();
                let name = self.extract_recipe_name(&path);
                let Some(name) = name else { continue };

                if seen.contains_key(&name) {
                    continue;
                }

                seen.insert(
                    name.clone(),
                    RecipeInfo {
                        name,
                        path: self.resolve_recipe_path(&path),
                        source: source.clone(),
                    },
                );
            }
        }

        let mut recipes: Vec<RecipeInfo> = seen.into_values().collect();
        recipes.sort_by(|a, b| a.name.cmp(&b.name));
        recipes
    }

    /// Load a recipe by name, searching all paths.
    pub fn load(&mut self, name: &str) -> Result<&Recipe> {
        if self.cache.contains_key(name) {
            return Ok(&self.cache[name]);
        }

        let path = self.find_recipe_path(name)?;
        let recipe = Recipe::from_file(&path)?;

        if recipe.meta.name != name {
            return Err(RecipeError::Invalid(format!(
                "recipe file name '{}' does not match recipe.name '{}'",
                name, recipe.meta.name
            )));
        }

        self.cache.insert(name.to_string(), recipe);
        Ok(&self.cache[name])
    }

    /// Load a recipe directly from a file path.
    pub fn load_from_path(&mut self, path: &Path) -> Result<&Recipe> {
        let recipe = Recipe::from_file(path)?;
        let name = recipe.meta.name.clone();
        self.cache.insert(name.clone(), recipe);
        Ok(&self.cache[&name])
    }

    /// Find the file path for a recipe by name.
    fn find_recipe_path(&self, name: &str) -> Result<PathBuf> {
        for (dir, _source) in &self.search_paths {
            let toml_path = dir.join(name).join("recipe.toml");
            if toml_path.exists() {
                return Ok(toml_path);
            }

            let toml_file = dir.join(format!("{name}.toml"));
            if toml_file.exists() {
                return Ok(toml_file);
            }

            let dir_path = dir.join(name);
            if dir_path.is_dir() {
                let recipe_toml = dir_path.join("recipe.toml");
                if recipe_toml.exists() {
                    return Ok(recipe_toml);
                }
            }
        }

        Err(RecipeError::NotFound(name.to_string()))
    }

    /// Extract recipe name from a path entry.
    fn extract_recipe_name(&self, path: &Path) -> Option<String> {
        if path.is_dir() {
            let recipe_toml = path.join("recipe.toml");
            if recipe_toml.exists() {
                return path.file_name()?.to_str().map(String::from);
            }
        } else if path.extension().and_then(|e| e.to_str()) == Some("toml") {
            return path.file_stem()?.to_str().map(String::from);
        }
        None
    }

    /// Resolve the actual recipe.toml path for a directory or file entry.
    fn resolve_recipe_path(&self, path: &Path) -> PathBuf {
        if path.is_dir() {
            path.join("recipe.toml")
        } else {
            path.to_path_buf()
        }
    }

    /// Return the user recipes directory (~/.colony/recipes/).
    pub fn user_recipes_dir() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".colony/recipes"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discover_empty() {
        let loader = RecipeLoader {
            search_paths: vec![],
            cache: HashMap::new(),
        };
        let recipes = loader.discover();
        assert!(recipes.is_empty());
    }

    #[test]
    fn test_discover_with_toml_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("test-recipe.toml"),
            r#"
[recipe]
name = "test-recipe"
"#,
        )
        .unwrap();

        let loader = RecipeLoader {
            search_paths: vec![(dir.path().to_path_buf(), RecipeSource::User)],
            cache: HashMap::new(),
        };
        let recipes = loader.discover();
        assert_eq!(recipes.len(), 1);
        assert_eq!(recipes[0].name, "test-recipe");
        assert_eq!(recipes[0].source, RecipeSource::User);
    }

    #[test]
    fn test_discover_with_directory() {
        let dir = tempfile::tempdir().unwrap();
        let recipe_dir = dir.path().join("my-recipe");
        std::fs::create_dir(&recipe_dir).unwrap();
        std::fs::write(
            recipe_dir.join("recipe.toml"),
            r#"
[recipe]
name = "my-recipe"
"#,
        )
        .unwrap();

        let loader = RecipeLoader {
            search_paths: vec![(dir.path().to_path_buf(), RecipeSource::User)],
            cache: HashMap::new(),
        };
        let recipes = loader.discover();
        assert_eq!(recipes.len(), 1);
        assert_eq!(recipes[0].name, "my-recipe");
    }

    #[test]
    fn test_load_recipe_from_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("test-recipe.toml"),
            r#"
[recipe]
name = "test-recipe"
description = "A test recipe"
version = "1.0.0"

[prompt]
system = "You are a test assistant."
"#,
        )
        .unwrap();

        let mut loader = RecipeLoader {
            search_paths: vec![(dir.path().to_path_buf(), RecipeSource::User)],
            cache: HashMap::new(),
        };
        let recipe = loader.load("test-recipe").unwrap();
        assert_eq!(recipe.meta.name, "test-recipe");
        assert_eq!(recipe.meta.version, "1.0.0");
    }

    #[test]
    fn test_load_recipe_not_found() {
        let mut loader = RecipeLoader {
            search_paths: vec![],
            cache: HashMap::new(),
        };
        let result = loader.load("nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_priority_ordering() {
        let project_dir = tempfile::tempdir().unwrap();
        let user_dir = tempfile::tempdir().unwrap();

        std::fs::write(
            project_dir.path().join("shared.toml"),
            r#"
[recipe]
name = "shared"
description = "project version"
"#,
        )
        .unwrap();

        std::fs::write(
            user_dir.path().join("shared.toml"),
            r#"
[recipe]
name = "shared"
description = "user version"
"#,
        )
        .unwrap();

        let loader = RecipeLoader {
            search_paths: vec![
                (project_dir.path().to_path_buf(), RecipeSource::Project),
                (user_dir.path().to_path_buf(), RecipeSource::User),
            ],
            cache: HashMap::new(),
        };
        let recipes = loader.discover();
        assert_eq!(recipes.len(), 1);
        assert_eq!(recipes[0].source, RecipeSource::Project);
    }
}
