//! Recipe composition engine
//!
//! Handles importing, merging, and dependency resolution for recipe hierarchies.
//! Recipes can import other recipes via the `imports` field, forming a DAG.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::error::{RecipeError, Result};
use crate::recipe::{CommandDef, McpServerConfig, Recipe, SkillDef};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictKind {
    Skill(String),
    Command(String),
    McpServer(String),
}

impl std::fmt::Display for ConflictKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Skill(name) => write!(f, "skill '{name}'"),
            Self::Command(name) => write!(f, "command '{name}'"),
            Self::McpServer(name) => write!(f, "MCP server '{name}'"),
        }
    }
}

#[derive(Debug)]
pub struct MergeConflict {
    pub kind: ConflictKind,
    pub sources: Vec<String>,
}

impl std::fmt::Display for MergeConflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "conflict on {}: defined in [{}]",
            self.kind,
            self.sources.join(", ")
        )
    }
}

#[derive(Debug)]
pub struct ComposedRecipe {
    pub root: Recipe,
    pub merged_skills: HashMap<String, SkillDef>,
    pub merged_commands: Vec<CommandDef>,
    pub merged_mcp_servers: Vec<McpServerConfig>,
    pub resolution_order: Vec<String>,
}

pub fn topological_sort(recipes: &HashMap<String, Recipe>, root: &str) -> Result<Vec<String>> {
    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut all_names: HashSet<&str> = HashSet::new();

    let mut queue: VecDeque<&str> = VecDeque::new();
    queue.push_back(root);

    while let Some(name) = queue.pop_front() {
        if !all_names.insert(name) {
            continue;
        }

        let recipe = recipes
            .get(name)
            .ok_or_else(|| RecipeError::NotFound(name.to_string()))?;

        for import in &recipe.meta.imports {
            let import_str = import.as_str();
            adjacency.entry(import_str).or_default().push(name);
            *in_degree.entry(name).or_insert(0) += 1;

            if !all_names.contains(import_str) {
                queue.push_back(import_str);
            }
        }

        in_degree.entry(name).or_insert(0);
    }

    let mut order = Vec::new();
    let mut zero_degree: VecDeque<&str> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(&name, _)| name)
        .collect();

    zero_degree.make_contiguous().sort();

    while let Some(node) = zero_degree.pop_front() {
        order.push(node.to_string());

        if let Some(dependents) = adjacency.get(node) {
            for &dep in dependents {
                if let Some(deg) = in_degree.get_mut(dep) {
                    *deg -= 1;
                    if *deg == 0 {
                        zero_degree.push_back(dep);
                    }
                }
            }
        }
    }

    if order.len() != all_names.len() {
        let remaining: Vec<String> = all_names
            .iter()
            .filter(|name| !order.contains(&name.to_string()))
            .map(|s| s.to_string())
            .collect();
        return Err(RecipeError::Invalid(format!(
            "circular dependency detected among: [{}]",
            remaining.join(", ")
        )));
    }

    Ok(order)
}

pub fn detect_conflicts(recipes: &HashMap<String, Recipe>, order: &[String]) -> Vec<MergeConflict> {
    let mut skill_sources: HashMap<String, Vec<String>> = HashMap::new();
    let mut command_sources: HashMap<String, Vec<String>> = HashMap::new();
    let mut server_sources: HashMap<String, Vec<String>> = HashMap::new();

    for name in order {
        let Some(recipe) = recipes.get(name) else {
            continue;
        };

        for skill_name in recipe.skills.keys() {
            skill_sources
                .entry(skill_name.clone())
                .or_default()
                .push(name.clone());
        }

        for cmd in &recipe.commands {
            command_sources
                .entry(cmd.name.clone())
                .or_default()
                .push(name.clone());
        }

        for server in &recipe.tools.mcp_servers {
            server_sources
                .entry(server.name.clone())
                .or_default()
                .push(name.clone());
        }
    }

    let mut conflicts = Vec::new();

    for (name, sources) in &skill_sources {
        if sources.len() > 1 {
            conflicts.push(MergeConflict {
                kind: ConflictKind::Skill(name.clone()),
                sources: sources.clone(),
            });
        }
    }

    for (name, sources) in &command_sources {
        if sources.len() > 1 {
            conflicts.push(MergeConflict {
                kind: ConflictKind::Command(name.clone()),
                sources: sources.clone(),
            });
        }
    }

    for (name, sources) in &server_sources {
        if sources.len() > 1 {
            conflicts.push(MergeConflict {
                kind: ConflictKind::McpServer(name.clone()),
                sources: sources.clone(),
            });
        }
    }

    conflicts
}

pub fn compose(recipes: &HashMap<String, Recipe>, root_name: &str) -> Result<ComposedRecipe> {
    let order = topological_sort(recipes, root_name)?;
    let conflicts = detect_conflicts(recipes, &order);

    if !conflicts.is_empty() {
        let msgs: Vec<String> = conflicts.iter().map(|c| c.to_string()).collect();
        return Err(RecipeError::Invalid(format!(
            "merge conflicts:\n{}",
            msgs.join("\n")
        )));
    }

    let mut merged_skills: HashMap<String, SkillDef> = HashMap::new();
    let mut merged_commands: Vec<CommandDef> = Vec::new();
    let mut merged_mcp_servers: Vec<McpServerConfig> = Vec::new();
    let mut seen_commands: HashSet<String> = HashSet::new();
    let mut seen_servers: HashSet<String> = HashSet::new();

    for name in &order {
        let Some(recipe) = recipes.get(name) else {
            return Err(RecipeError::NotFound(name.clone()));
        };

        for (skill_name, skill_def) in &recipe.skills {
            merged_skills
                .entry(skill_name.clone())
                .or_insert_with(|| skill_def.clone());
        }

        for cmd in &recipe.commands {
            if seen_commands.insert(cmd.name.clone()) {
                merged_commands.push(cmd.clone());
            }
        }

        for server in &recipe.tools.mcp_servers {
            if seen_servers.insert(server.name.clone()) {
                merged_mcp_servers.push(server.clone());
            }
        }
    }

    let root = recipes
        .get(root_name)
        .ok_or_else(|| RecipeError::NotFound(root_name.to_string()))?
        .clone();

    Ok(ComposedRecipe {
        root,
        merged_skills,
        merged_commands,
        merged_mcp_servers,
        resolution_order: order,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipe::{BehaviorConfig, ModelConfig, PromptConfig, RecipeMeta, ToolsConfig};

    fn make_recipe(name: &str, imports: Vec<&str>) -> Recipe {
        Recipe {
            meta: RecipeMeta {
                name: name.to_string(),
                description: None,
                version: "1.0.0".to_string(),
                author: None,
                imports: imports.into_iter().map(String::from).collect(),
            },
            prompt: PromptConfig::default(),
            tools: ToolsConfig::default(),
            skills: HashMap::new(),
            commands: Vec::new(),
            model: ModelConfig::default(),
            behavior: BehaviorConfig::default(),
        }
    }

    #[test]
    fn test_topological_sort_single() {
        let mut recipes = HashMap::new();
        recipes.insert("root".to_string(), make_recipe("root", vec![]));

        let order = topological_sort(&recipes, "root").unwrap();
        assert_eq!(order, vec!["root"]);
    }

    #[test]
    fn test_topological_sort_chain() {
        let mut recipes = HashMap::new();
        recipes.insert("a".to_string(), make_recipe("a", vec![]));
        recipes.insert("b".to_string(), make_recipe("b", vec!["a"]));
        recipes.insert("c".to_string(), make_recipe("c", vec!["b"]));

        let order = topological_sort(&recipes, "c").unwrap();
        assert_eq!(order, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_topological_sort_diamond() {
        let mut recipes = HashMap::new();
        recipes.insert("base".to_string(), make_recipe("base", vec![]));
        recipes.insert("left".to_string(), make_recipe("left", vec!["base"]));
        recipes.insert("right".to_string(), make_recipe("right", vec!["base"]));
        recipes.insert("top".to_string(), make_recipe("top", vec!["left", "right"]));

        let order = topological_sort(&recipes, "top").unwrap();
        assert_eq!(order[0], "base");
        assert!(order.contains(&"left".to_string()));
        assert!(order.contains(&"right".to_string()));
        assert_eq!(order.last().unwrap(), "top");
    }

    #[test]
    fn test_topological_sort_cycle() {
        let mut recipes = HashMap::new();
        recipes.insert("a".to_string(), make_recipe("a", vec!["b"]));
        recipes.insert("b".to_string(), make_recipe("b", vec!["a"]));

        let result = topological_sort(&recipes, "a");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("circular dependency"));
    }

    #[test]
    fn test_topological_sort_missing_import() {
        let mut recipes = HashMap::new();
        recipes.insert("a".to_string(), make_recipe("a", vec!["missing"]));

        let result = topological_sort(&recipes, "a");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not found"));
    }

    #[test]
    fn test_detect_conflicts_no_conflicts() {
        let mut recipes = HashMap::new();

        let mut r1 = make_recipe("base", vec![]);
        r1.skills.insert(
            "lint".to_string(),
            SkillDef {
                trigger: Some("/lint".to_string()),
                description: None,
                prompt: None,
            },
        );

        let mut r2 = make_recipe("top", vec!["base"]);
        r2.skills.insert(
            "test".to_string(),
            SkillDef {
                trigger: Some("/test".to_string()),
                description: None,
                prompt: None,
            },
        );

        recipes.insert("base".to_string(), r1);
        recipes.insert("top".to_string(), r2);

        let conflicts = detect_conflicts(&recipes, &["base".to_string(), "top".to_string()]);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn test_detect_conflicts_skill_conflict() {
        let mut recipes = HashMap::new();

        let mut r1 = make_recipe("a", vec![]);
        r1.skills.insert(
            "review".to_string(),
            SkillDef {
                trigger: Some("/review".to_string()),
                description: None,
                prompt: None,
            },
        );

        let mut r2 = make_recipe("b", vec![]);
        r2.skills.insert(
            "review".to_string(),
            SkillDef {
                trigger: Some("/review".to_string()),
                description: Some("different".to_string()),
                prompt: None,
            },
        );

        recipes.insert("a".to_string(), r1);
        recipes.insert("b".to_string(), r2);

        let conflicts = detect_conflicts(&recipes, &["a".to_string(), "b".to_string()]);
        assert_eq!(conflicts.len(), 1);
        assert!(matches!(conflicts[0].kind, ConflictKind::Skill(ref s) if s == "review"));
    }

    #[test]
    fn test_compose_simple() {
        let mut recipes = HashMap::new();

        let mut base = make_recipe("base", vec![]);
        base.skills.insert(
            "lint".to_string(),
            SkillDef {
                trigger: Some("/lint".to_string()),
                description: None,
                prompt: Some("Run lint checks.".to_string()),
            },
        );
        base.commands.push(CommandDef {
            name: "build".to_string(),
            description: Some("Build project".to_string()),
            action: crate::recipe::CommandAction::Shell,
            command: Some("cargo build".to_string()),
            prompt: None,
        });

        let mut top = make_recipe("top", vec!["base"]);
        top.skills.insert(
            "test".to_string(),
            SkillDef {
                trigger: Some("/test".to_string()),
                description: None,
                prompt: Some("Run tests.".to_string()),
            },
        );

        recipes.insert("base".to_string(), base);
        recipes.insert("top".to_string(), top);

        let composed = compose(&recipes, "top").unwrap();
        assert_eq!(composed.resolution_order, vec!["base", "top"]);
        assert_eq!(composed.merged_skills.len(), 2);
        assert!(composed.merged_skills.contains_key("lint"));
        assert!(composed.merged_skills.contains_key("test"));
        assert_eq!(composed.merged_commands.len(), 1);
        assert_eq!(composed.root.meta.name, "top");
    }

    #[test]
    fn test_compose_conflict_errors() {
        let mut recipes = HashMap::new();

        let mut base = make_recipe("base", vec![]);
        base.skills.insert(
            "review".to_string(),
            SkillDef {
                trigger: Some("/review".to_string()),
                description: None,
                prompt: None,
            },
        );

        let mut top = make_recipe("top", vec!["base"]);
        top.skills.insert(
            "review".to_string(),
            SkillDef {
                trigger: Some("/review".to_string()),
                description: Some("conflict".to_string()),
                prompt: None,
            },
        );

        recipes.insert("base".to_string(), base);
        recipes.insert("top".to_string(), top);

        let result = compose(&recipes, "top");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("merge conflicts"));
    }

    #[test]
    fn test_compose_mcp_servers() {
        let mut recipes = HashMap::new();

        let mut base = make_recipe("base", vec![]);
        base.tools.mcp_servers.push(McpServerConfig {
            name: "filesystem".to_string(),
            command: "npx".to_string(),
            args: vec!["-y".to_string(), "@anthropic/mcp-filesystem".to_string()],
            env: HashMap::new(),
        });

        let mut top = make_recipe("top", vec!["base"]);
        top.tools.mcp_servers.push(McpServerConfig {
            name: "github".to_string(),
            command: "npx".to_string(),
            args: vec!["-y".to_string(), "@anthropic/mcp-github".to_string()],
            env: HashMap::new(),
        });

        recipes.insert("base".to_string(), base);
        recipes.insert("top".to_string(), top);

        let composed = compose(&recipes, "top").unwrap();
        assert_eq!(composed.merged_mcp_servers.len(), 2);
    }
}
