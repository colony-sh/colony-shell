# Recipe System Implementation Plan

**Date**: 2025-12-29  
**Source**: [Recipe System Design](./2025-12-29-recipe-system-design.md)  
**Status**: Ready for Implementation

## Executive Summary

The Recipe System provides LLM "modes" through declarative TOML configuration, optional Lua scripting, and a distribution system via Colony Cloud. This plan breaks down the 8-week implementation into atomic features with clear acceptance criteria.

---

## Features Overview

| ID | Feature | Phase | Priority | Effort | Dependencies |
|----|---------|-------|----------|--------|--------------|
| RS-001 | Recipe Crate Scaffolding | 1 | P0 | 1d | None |
| RS-002 | Recipe File Parsing | 1 | P0 | 2d | RS-001 |
| RS-003 | Skill File Parsing | 1 | P0 | 1d | RS-001 |
| RS-004 | Bundle File Parsing | 1 | P0 | 1d | RS-001 |
| RS-005 | Recipe File Discovery | 1 | P0 | 2d | RS-002 |
| RS-006 | Import Resolution (Local) | 1 | P0 | 2d | RS-005 |
| RS-007 | Recipe → Agent Integration | 1 | P0 | 3d | RS-006 |
| RS-008 | CLI: recipe list/use/show | 1 | P0 | 2d | RS-007 |
| RS-009 | Plugin File Parsing | 2 | P0 | 2d | RS-002 |
| RS-010 | Hook Registry & Execution | 2 | P0 | 3d | RS-009 |
| RS-011 | Subagent Spawning | 2 | P0 | 2d | RS-010, RS-007 |
| RS-012 | Slash Command Registration | 2 | P0 | 2d | RS-009 |
| RS-013 | Conditional Prompt Injection | 2 | P1 | 1d | RS-009 |
| RS-014 | CLI: plugin list/enable/disable | 2 | P1 | 1d | RS-009 |
| RS-015 | Lua Sandbox Environment | 3 | P0 | 2d | RS-001 |
| RS-016 | Colony Lua API (File/Shell) | 3 | P0 | 2d | RS-015 |
| RS-017 | Colony Lua API (Prompt/Context) | 3 | P0 | 2d | RS-015 |
| RS-018 | Lua Lifecycle Hooks | 3 | P0 | 2d | RS-017, RS-010 |
| RS-019 | Custom Commands in Lua | 3 | P1 | 2d | RS-018, RS-012 |
| RS-020 | Custom Tools in Lua | 3 | P2 | 2d | RS-018 |
| RS-021 | Registry Client (Read) | 4 | P0 | 2d | RS-001 |
| RS-022 | CLI: recipe install/update | 4 | P0 | 2d | RS-021 |
| RS-023 | Version Resolution | 4 | P0 | 2d | RS-021 |
| RS-024 | Lockfile Generation | 4 | P1 | 1d | RS-023 |
| RS-025 | CLI: recipe publish | 4 | P1 | 2d | RS-021 |
| RS-026 | Private Namespace Support | 4 | P2 | 2d | RS-021 |
| RS-027 | Built-in Recipes | 5 | P0 | 2d | RS-007 |
| RS-028 | Built-in Skills | 5 | P0 | 2d | RS-003 |
| RS-029 | Built-in Bundles | 5 | P0 | 2d | RS-004 |
| RS-030 | Project Type Auto-Detection | 5 | P1 | 2d | RS-005 |
| RS-031 | .colonyrc Recipe Override | 5 | P1 | 1d | RS-007, F020 |

---

## Phase 1: Core Recipe System (Days 1-14)

### RS-001: Recipe Crate Scaffolding

**Goal**: Create the `colony-recipe` crate with proper structure and dependencies.

**Tasks**:
1. Create `crates/colony-recipe/` directory
2. Initialize Cargo.toml with dependencies
3. Create module structure
4. Add to workspace Cargo.toml

**Acceptance Criteria**:
- [ ] Crate compiles
- [ ] All modules exist (empty stubs OK)
- [ ] Part of workspace build

**File Structure**:
```
crates/colony-recipe/
├── src/
│   ├── lib.rs              # Public API
│   ├── recipe.rs           # Recipe struct, parsing
│   ├── plugin.rs           # Plugin struct, hooks
│   ├── skill.rs            # Skill definitions
│   ├── bundle.rs           # Tool bundles
│   ├── loader.rs           # File discovery & loading
│   ├── resolver.rs         # Import/dependency resolution
│   ├── registry.rs         # Colony Cloud registry client
│   ├── lua/
│   │   ├── mod.rs          # Lua integration
│   │   ├── sandbox.rs      # Restricted Lua environment
│   │   └── api.rs          # Colony Lua API bindings
│   └── error.rs            # Recipe errors
└── Cargo.toml
```

**Cargo.toml**:
```toml
[package]
name = "colony-recipe"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { version = "1", features = ["derive"] }
toml = "0.8"
thiserror = "2"
tokio = { version = "1", features = ["fs"] }

[dependencies.mlua]
version = "0.10"
features = ["lua54", "vendored", "serialize"]
optional = true

[dependencies.reqwest]
version = "0.12"
features = ["json"]
optional = true

[features]
default = ["lua"]
lua = ["mlua"]
registry = ["reqwest"]
```

**Dependencies**: None

---

### RS-002: Recipe File Parsing

**Goal**: Parse recipe TOML files into strongly-typed Rust structs.

**Tasks**:
1. Define `Recipe` struct with all sections
2. Define `RecipeMeta`, `PromptConfig`, `ToolsConfig`, etc.
3. Implement `Recipe::from_toml()` parser
4. Add validation for required fields
5. Handle optional sections gracefully

**Acceptance Criteria**:
- [ ] Valid TOML parses without error
- [ ] Missing optional sections default correctly
- [ ] Invalid TOML returns clear error
- [ ] All config sections represented

**Core Types**:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recipe {
    pub meta: RecipeMeta,
    #[serde(default)]
    pub imports: Vec<String>,
    pub prompt: PromptConfig,
    #[serde(default)]
    pub tools: ToolsConfig,
    #[serde(default)]
    pub skills: HashMap<String, Skill>,
    #[serde(default)]
    pub commands: Vec<Command>,
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub keybindings: HashMap<String, String>,
    #[serde(default)]
    pub model: ModelConfig,
    #[serde(default)]
    pub behavior: BehaviorConfig,
    #[serde(default)]
    pub lua: Option<LuaConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeMeta {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_version")]
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptConfig {
    pub system: String,
    #[serde(default)]
    pub context_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsConfig {
    #[serde(default)]
    pub mcp_servers: Vec<McpServerConfig>,
    #[serde(default)]
    pub approval: ToolApprovalConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolApprovalConfig {
    #[serde(default)]
    pub auto_approve: Vec<String>,
    #[serde(default)]
    pub require_approval: Vec<String>,
    #[serde(default)]
    pub deny: Vec<String>,
}
```

**Dependencies**: RS-001

---

### RS-003: Skill File Parsing

**Goal**: Parse standalone skill TOML files.

**Tasks**:
1. Define `Skill` struct
2. Define `SkillParameter` for typed params
3. Define `SkillPrompt` with template support
4. Implement `Skill::from_toml()` parser
5. Add parameter validation

**Acceptance Criteria**:
- [ ] Skill TOML parses correctly
- [ ] Parameters have types and defaults
- [ ] Template placeholders preserved
- [ ] Required vs optional params enforced

**Core Types**:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub trigger: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub parameters: HashMap<String, SkillParameter>,
    pub prompt: SkillPrompt,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillParameter {
    #[serde(rename = "type")]
    pub param_type: String, // "string", "bool", "int"
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "enum", default)]
    pub allowed_values: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillPrompt {
    pub template: String,
}
```

**Dependencies**: RS-001

---

### RS-004: Bundle File Parsing

**Goal**: Parse bundle TOML files for tool collections.

**Tasks**:
1. Define `Bundle` struct
2. Define `BundleMeta` and `McpConfig`
3. Implement `Bundle::from_toml()` parser
4. Handle MCP server configurations

**Acceptance Criteria**:
- [ ] Bundle TOML parses correctly
- [ ] MCP servers have command and args
- [ ] Tool lists parsed
- [ ] Approval policies captured

**Core Types**:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bundle {
    pub meta: BundleMeta,
    #[serde(default)]
    pub mcp: McpConfig,
    #[serde(default)]
    pub tools: BundleToolsConfig,
    #[serde(default)]
    pub approval: ToolApprovalConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleMeta {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_version")]
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfig {
    #[serde(default)]
    pub servers: Vec<McpServerConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}
```

**Dependencies**: RS-001

---

### RS-005: Recipe File Discovery

**Goal**: Implement RecipeLoader that discovers recipes from multiple locations.

**Tasks**:
1. Define search paths in priority order:
   - `.colony/recipes/` (project-specific)
   - `~/.config/colony/recipes/` (user personal)
   - `~/.cache/colony/registry/` (downloaded)
   - Built-in (compiled into binary)
2. Implement `RecipeLoader::discover()` that walks all paths
3. Implement `RecipeLoader::load_recipe(name)` with namespace handling
4. Handle namespace prefixes (`official/`, `community/`, `local/`)

**Acceptance Criteria**:
- [ ] Discovers recipes in all locations
- [ ] Priority order respected (project > user > cache > builtin)
- [ ] Namespaces resolved correctly
- [ ] Missing recipe returns clear error

**Key Code**:
```rust
pub struct RecipeLoader {
    search_paths: Vec<PathBuf>,
    cache: HashMap<String, Recipe>,
}

impl RecipeLoader {
    pub fn new() -> Self {
        let mut paths = vec![];
        
        // Project-specific (highest priority)
        if let Ok(cwd) = std::env::current_dir() {
            paths.push(cwd.join(".colony/recipes"));
        }
        
        // User personal
        if let Some(config) = dirs::config_dir() {
            paths.push(config.join("colony/recipes"));
        }
        
        // Downloaded from registry
        if let Some(cache) = dirs::cache_dir() {
            paths.push(cache.join("colony/registry"));
        }
        
        Self { search_paths: paths, cache: HashMap::new() }
    }
    
    pub fn discover(&self) -> Vec<RecipeInfo> {
        // Walk all paths, return metadata about available recipes
    }
    
    pub fn load_recipe(&mut self, name: &str) -> Result<&Recipe, Error> {
        // Parse namespace, search paths, load and cache
    }
}
```

**Dependencies**: RS-002

---

### RS-006: Import Resolution (Local)

**Goal**: Resolve recipe imports and merge configurations.

**Tasks**:
1. Implement `RecipeResolver` that processes imports
2. Recursively load imported recipes/skills/bundles
3. Merge configurations in proper order
4. Detect and report circular dependencies
5. Build `ResolvedRecipe` with all imports flattened

**Acceptance Criteria**:
- [ ] Single-level imports work
- [ ] Nested imports work
- [ ] Circular imports detected and error
- [ ] Skills from imports available
- [ ] Bundles from imports merged

**Key Code**:
```rust
pub struct RecipeResolver<'a> {
    loader: &'a mut RecipeLoader,
    visited: HashSet<String>,
}

impl<'a> RecipeResolver<'a> {
    pub async fn resolve(&mut self, recipe: &Recipe) -> Result<ResolvedRecipe, Error> {
        let mut resolved = ResolvedRecipe::from(recipe);
        
        for import in &recipe.imports {
            if self.visited.contains(import) {
                return Err(Error::CircularImport(import.clone()));
            }
            self.visited.insert(import.clone());
            
            let imported = self.loader.load_recipe(import)?;
            let resolved_import = Box::pin(self.resolve(imported)).await?;
            resolved.merge(resolved_import);
        }
        
        Ok(resolved)
    }
}

pub struct ResolvedRecipe {
    pub system_prompt: String,
    pub context_files: Vec<String>,
    pub skills: HashMap<String, Skill>,
    pub commands: Vec<Command>,
    pub mcp_servers: Vec<McpServerConfig>,
    pub tool_approval: ToolApprovalConfig,
    pub model: ModelConfig,
    pub behavior: BehaviorConfig,
}
```

**Dependencies**: RS-005

---

### RS-007: Recipe → Agent Integration

**Goal**: Create agents from resolved recipes.

**Tasks**:
1. Add `Agent::with_recipe(name)` constructor
2. Build system prompt from recipe + context files
3. Configure tools from recipe's MCP servers
4. Set model provider and settings from recipe
5. Store skills for later execution
6. Wire up to existing Agent infrastructure

**Acceptance Criteria**:
- [ ] Agent created with recipe's system prompt
- [ ] Context files loaded if they exist
- [ ] MCP servers started
- [ ] Model configured per recipe
- [ ] Skills accessible via Agent

**Integration Points**:
```rust
impl Agent {
    pub async fn with_recipe(recipe_name: &str) -> Result<Self> {
        let mut loader = RecipeLoader::new();
        let recipe = loader.load_recipe(recipe_name)?;
        let resolved = RecipeResolver::new(&mut loader).resolve(recipe).await?;
        
        // Build system prompt
        let mut prompt = resolved.system_prompt.clone();
        for context_file in &resolved.context_files {
            if let Ok(content) = tokio::fs::read_to_string(context_file).await {
                prompt.push_str("\n\n");
                prompt.push_str(&content);
            }
        }
        
        // Configure model
        let provider = resolved.model.provider.as_deref().unwrap_or("colony");
        let model_name = resolved.model.name.as_deref().unwrap_or("claude-3.5-sonnet");
        
        // Create agent with recipe config
        let mut agent = Agent::new(provider, model_name).await?;
        agent.set_system_prompt(&prompt);
        agent.set_skills(resolved.skills);
        
        // Start MCP servers
        for server in &resolved.mcp_servers {
            agent.add_mcp_server(server).await?;
        }
        
        Ok(agent)
    }
}
```

**Dependencies**: RS-006

---

### RS-008: CLI: recipe list/use/show

**Goal**: Implement basic recipe management CLI commands.

**Tasks**:
1. Add `recipe` subcommand to colony-shell
2. Implement `recipe list` - shows all available recipes
3. Implement `recipe use <name>` - activates a recipe
4. Implement `recipe show <name>` - displays recipe details
5. Implement `recipe current` - shows active recipe
6. Store active recipe in shell state

**Acceptance Criteria**:
- [ ] `colony recipe list` shows discovered recipes
- [ ] `colony recipe use coding-agent` activates recipe
- [ ] `colony recipe show coding-agent` displays details
- [ ] `colony recipe current` shows active recipe name
- [ ] Active recipe persists across commands

**CLI Output Examples**:
```
$ colony recipe list
Available recipes:
  coding-agent      Claude Code-style coding assistant
  chat              Simple chat mode without tools
  research          Research assistant with web search
  [local] my-recipe Custom project recipe

$ colony recipe show coding-agent
Recipe: coding-agent v1.0.0
Description: Claude Code-style coding assistant

Imports:
  - official/bundles/file-ops
  - official/bundles/git-ops
  - official/skills/brainstorming

Skills:
  /review   - Review code for issues
  /explain  - Explain code in detail

Commands:
  /test     - Run project tests
  /build    - Build the project
```

**Dependencies**: RS-007

---

## Phase 2: Plugins & Hooks (Days 15-28)

### RS-009: Plugin File Parsing

**Goal**: Parse plugin TOML files with all their components.

**Tasks**:
1. Define `Plugin` struct
2. Define `PluginPrompt` with inject and conditional
3. Define `HooksConfig` with all hook types
4. Define `Subagent` configuration
5. Implement `Plugin::from_toml()` parser

**Acceptance Criteria**:
- [ ] Plugin TOML parses all sections
- [ ] Hooks have proper structure
- [ ] Subagents configured with prompts and tools
- [ ] Commands include action types

**Core Types**:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plugin {
    pub meta: PluginMeta,
    #[serde(default)]
    pub prompt: PluginPrompt,
    #[serde(default)]
    pub commands: Vec<Command>,
    #[serde(default)]
    pub hooks: HooksConfig,
    #[serde(default)]
    pub agents: HashMap<String, Subagent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginPrompt {
    #[serde(default)]
    pub inject: Option<String>,
    #[serde(default)]
    pub conditional: Vec<ConditionalPrompt>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionalPrompt {
    pub when: String,  // Simple condition expression
    pub inject: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HooksConfig {
    #[serde(default)]
    pub before_tool: Option<HookDef>,
    #[serde(default)]
    pub after_tool: Option<HookDef>,
    #[serde(default)]
    pub on_session_start: Option<HookDef>,
    #[serde(default)]
    pub on_session_end: Option<HookDef>,
    #[serde(default)]
    pub on_error: Option<HookDef>,
    #[serde(default)]
    pub on_message: Option<HookDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookDef {
    #[serde(rename = "match", default)]
    pub match_patterns: Vec<String>,
    pub action: HookAction,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub suggestion: Option<String>,
    #[serde(default)]
    pub agent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HookAction {
    Prompt,
    Suggest,
    Confirm,
    Block,
    Modify,
    Agent,
    Shell,
    Transform,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subagent {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub system_prompt: String,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub model: Option<String>,
}
```

**Dependencies**: RS-002

---

### RS-010: Hook Registry & Execution

**Goal**: Implement hook registration and execution system.

**Tasks**:
1. Create `HookRegistry` struct
2. Register hooks from loaded plugins
3. Implement hook matching (tool name, event type, patterns)
4. Implement hook execution with action handling
5. Wire hooks into agent execution flow

**Acceptance Criteria**:
- [ ] Hooks registered from plugins
- [ ] before_tool hooks fire before tool calls
- [ ] after_tool hooks fire after tool completion
- [ ] on_error hooks fire on errors
- [ ] Hook actions execute correctly

**Key Code**:
```rust
pub struct HookRegistry {
    before_tool: Vec<RegisteredHook>,
    after_tool: Vec<RegisteredHook>,
    on_session_start: Vec<RegisteredHook>,
    on_error: Vec<RegisteredHook>,
}

pub struct RegisteredHook {
    pub source: String,  // Plugin name
    pub def: HookDef,
}

impl HookRegistry {
    pub fn register_plugin(&mut self, plugin: &Plugin) {
        if let Some(hook) = &plugin.hooks.before_tool {
            self.before_tool.push(RegisteredHook {
                source: plugin.meta.name.clone(),
                def: hook.clone(),
            });
        }
        // ... other hooks
    }
    
    pub async fn fire_before_tool(
        &self, 
        tool_name: &str, 
        input: &serde_json::Value,
        ctx: &mut HookContext,
    ) -> Result<HookResult, Error> {
        for hook in &self.before_tool {
            if hook.def.matches(tool_name) {
                match hook.def.action {
                    HookAction::Block => return Ok(HookResult::Block),
                    HookAction::Prompt => {
                        if let Some(prompt) = &hook.def.prompt {
                            ctx.inject_prompt(prompt);
                        }
                    }
                    HookAction::Confirm => {
                        if !ctx.user_confirmed().await? {
                            return Ok(HookResult::Block);
                        }
                    }
                    // ... other actions
                }
            }
        }
        Ok(HookResult::Continue)
    }
}

pub enum HookResult {
    Continue,
    Block,
    Modified(serde_json::Value),
}
```

**Dependencies**: RS-009

---

### RS-011: Subagent Spawning

**Goal**: Implement subagent creation and execution from plugins.

**Tasks**:
1. Create `SubagentManager` struct
2. Implement `spawn_subagent(name, prompt)` method
3. Configure subagent with plugin-defined settings
4. Handle subagent tool restrictions
5. Return subagent results to parent agent

**Acceptance Criteria**:
- [ ] Subagents spawn with correct config
- [ ] Subagent has its own system prompt
- [ ] Subagent tool list restricted per config
- [ ] Results returned to caller
- [ ] Subagent lifecycle managed properly

**Key Code**:
```rust
impl Agent {
    pub async fn run_subagent(
        &self,
        name: &str,
        prompt: &str,
    ) -> Result<String, Error> {
        let subagent_def = self.subagents.get(name)
            .ok_or(Error::UnknownSubagent(name.to_string()))?;
        
        // Create temporary agent with subagent config
        let model = subagent_def.model.as_deref()
            .unwrap_or(&self.model_name);
        
        let mut subagent = Agent::new(&self.provider_name, model).await?;
        subagent.set_system_prompt(&subagent_def.system_prompt);
        
        // Restrict tools
        for tool_name in &subagent_def.tools {
            if let Some(tool) = self.tools.get(tool_name) {
                subagent.register_tool(tool.clone());
            }
        }
        
        // Run with user prompt
        let response = subagent.chat(prompt).await?;
        Ok(response.text)
    }
}
```

**Dependencies**: RS-010, RS-007

---

### RS-012: Slash Command Registration

**Goal**: Implement slash command registration and dispatch.

**Tasks**:
1. Create `CommandRegistry` struct
2. Register commands from recipes and plugins
3. Implement command parsing (`/test`, `/review file.rs`)
4. Dispatch to appropriate handler (shell, agent, skill)
5. Handle command parameters

**Acceptance Criteria**:
- [ ] Commands registered from recipes
- [ ] `/test` executes shell command
- [ ] `/commit` triggers agent prompt
- [ ] `/review` runs skill with params
- [ ] Unknown command shows help

**Command Types**:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub action: CommandAction,
    #[serde(default)]
    pub command: Option<String>,  // For shell action
    #[serde(default)]
    pub prompt: Option<String>,   // For agent action
    #[serde(default)]
    pub agent: Option<String>,    // Subagent to use
    #[serde(default)]
    pub parameters: Vec<CommandParameter>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CommandAction {
    Shell,
    Agent,
    Skill,
    Lua,
}

impl Agent {
    pub async fn execute_command(
        &mut self,
        name: &str,
        args: HashMap<String, String>,
    ) -> Result<String, Error> {
        let cmd = self.commands.get(name)
            .ok_or(Error::UnknownCommand(name.to_string()))?;
        
        match cmd.action {
            CommandAction::Shell => {
                let command = cmd.command.as_ref().unwrap();
                execute_shell_command(command).await
            }
            CommandAction::Agent => {
                let prompt = cmd.prompt.as_ref().unwrap();
                if let Some(agent_name) = &cmd.agent {
                    self.run_subagent(agent_name, prompt).await
                } else {
                    self.chat(prompt).await.map(|r| r.text)
                }
            }
            CommandAction::Skill => {
                self.run_skill(name, args).await
            }
            CommandAction::Lua => {
                // Phase 3
                unimplemented!()
            }
        }
    }
}
```

**Dependencies**: RS-009

---

### RS-013: Conditional Prompt Injection

**Goal**: Implement conditional prompt injection based on context.

**Tasks**:
1. Parse condition expressions (simple equality for now)
2. Evaluate conditions against current context
3. Inject matching prompts into system prompt
4. Support file extension, project type, etc. conditions

**Acceptance Criteria**:
- [ ] `file_extension == 'rs'` condition works
- [ ] Multiple conditions evaluated
- [ ] Matching prompts injected
- [ ] Non-matching conditions skipped

**Key Code**:
```rust
pub struct ConditionContext {
    pub file_extension: Option<String>,
    pub project_type: Option<String>,
    pub git_branch: Option<String>,
}

impl ConditionalPrompt {
    pub fn evaluate(&self, ctx: &ConditionContext) -> bool {
        // Simple expression parser: "field == 'value'"
        let parts: Vec<&str> = self.when.split_whitespace().collect();
        if parts.len() != 3 {
            return false;
        }
        
        let (field, op, value) = (parts[0], parts[1], parts[2].trim_matches('\''));
        
        if op != "==" {
            return false;
        }
        
        match field {
            "file_extension" => ctx.file_extension.as_deref() == Some(value),
            "project_type" => ctx.project_type.as_deref() == Some(value),
            "git_branch" => ctx.git_branch.as_deref() == Some(value),
            _ => false,
        }
    }
}
```

**Dependencies**: RS-009

---

### RS-014: CLI: plugin list/enable/disable

**Goal**: Implement plugin management CLI commands.

**Tasks**:
1. Add `plugin` subcommand
2. Implement `plugin list` - shows available plugins
3. Implement `plugin enable <name>` - enables plugin in current recipe
4. Implement `plugin disable <name>` - disables plugin
5. Implement `plugin show <name>` - displays plugin details
6. Store enabled/disabled state

**Acceptance Criteria**:
- [ ] `colony plugin list` shows plugins
- [ ] `colony plugin enable pr-review-toolkit` enables plugin
- [ ] `colony plugin disable pr-review-toolkit` disables
- [ ] Plugin state persists
- [ ] `colony plugin show` displays hooks and commands

**Dependencies**: RS-009

---

## Phase 3: Lua Scripting (Days 29-38)

### RS-015: Lua Sandbox Environment

**Goal**: Create restricted Lua environment using mlua.

**Tasks**:
1. Create `LuaSandbox` struct wrapping mlua::Lua
2. Configure safe global environment
3. Block dangerous functions (os.execute, io.*, etc.)
4. Allow safe functions (string.*, table.*, math.*)
5. Set up module loading restrictions

**Acceptance Criteria**:
- [ ] Lua code executes in sandbox
- [ ] `os.execute` blocked
- [ ] `io.open` blocked
- [ ] `string.*` available
- [ ] `table.*` available

**Key Code**:
```rust
pub struct LuaSandbox {
    lua: Lua,
}

impl LuaSandbox {
    pub fn new() -> Result<Self, Error> {
        let lua = Lua::new();
        
        // Create restricted environment
        lua.scope(|scope| {
            let globals = lua.globals();
            
            // Remove dangerous globals
            globals.set("os", mlua::Value::Nil)?;
            globals.set("io", mlua::Value::Nil)?;
            globals.set("loadfile", mlua::Value::Nil)?;
            globals.set("dofile", mlua::Value::Nil)?;
            globals.set("require", mlua::Value::Nil)?;
            globals.set("debug", mlua::Value::Nil)?;
            
            Ok(())
        })?;
        
        Ok(Self { lua })
    }
    
    pub fn execute(&self, code: &str) -> Result<(), Error> {
        self.lua.load(code).exec()?;
        Ok(())
    }
}
```

**Dependencies**: RS-001

---

### RS-016: Colony Lua API (File/Shell)

**Goal**: Implement file and shell operations in Lua API.

**Tasks**:
1. Create `colony` global table
2. Implement `colony.read_file(path)` → string
3. Implement `colony.write_file(path, content)`
4. Implement `colony.file_exists(path)` → bool
5. Implement `colony.list_dir(path)` → table
6. Implement `colony.shell(command)` → string
7. Implement `colony.shell_async(command)` → handle

**Acceptance Criteria**:
- [ ] File operations work correctly
- [ ] Shell commands execute and return stdout
- [ ] Errors propagate as Lua errors
- [ ] Paths resolved relative to project root

**Key Code**:
```rust
impl LuaSandbox {
    fn register_colony_api(&self, project_root: &Path) -> Result<(), Error> {
        let globals = self.lua.globals();
        let colony = self.lua.create_table()?;
        
        // colony.read_file(path) -> string
        let root = project_root.to_path_buf();
        colony.set("read_file", self.lua.create_function(move |_, path: String| {
            let full_path = root.join(&path);
            std::fs::read_to_string(&full_path)
                .map_err(|e| mlua::Error::RuntimeError(e.to_string()))
        })?)?;
        
        // colony.shell(command) -> string
        colony.set("shell", self.lua.create_function(|_, cmd: String| {
            let output = std::process::Command::new("sh")
                .arg("-c")
                .arg(&cmd)
                .output()
                .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
            
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        })?)?;
        
        globals.set("colony", colony)?;
        Ok(())
    }
}
```

**Dependencies**: RS-015

---

### RS-017: Colony Lua API (Prompt/Context)

**Goal**: Implement prompt and context manipulation in Lua API.

**Tasks**:
1. Implement `ctx.prompt.append(text)`
2. Implement `ctx.prompt.prepend(text)`
3. Implement `ctx.suggest(message)`
4. Implement `ctx.notify(message, level)`
5. Implement `ctx.project.root`, `ctx.project.language`
6. Implement `ctx.session.get/set(key, value)`
7. Implement `ctx.import(recipe_or_bundle)`

**Acceptance Criteria**:
- [ ] Prompt manipulation works
- [ ] Suggestions shown to user
- [ ] Project info accessible
- [ ] Session state persists
- [ ] Dynamic imports work

**Key Code**:
```rust
pub struct LuaContext {
    prompt_additions: Vec<PromptAddition>,
    suggestions: Vec<String>,
    session_data: HashMap<String, String>,
    imports_requested: Vec<String>,
}

enum PromptAddition {
    Prepend(String),
    Append(String),
}

impl LuaSandbox {
    fn create_context_table(&self, ctx: &mut LuaContext) -> Result<mlua::Table, Error> {
        let context = self.lua.create_table()?;
        let prompt = self.lua.create_table()?;
        
        // ctx.prompt.append(text)
        let ctx_ref = Rc::new(RefCell::new(ctx));
        let ctx_clone = ctx_ref.clone();
        prompt.set("append", self.lua.create_function(move |_, text: String| {
            ctx_clone.borrow_mut().prompt_additions.push(PromptAddition::Append(text));
            Ok(())
        })?)?;
        
        context.set("prompt", prompt)?;
        
        // ctx.suggest(message)
        let ctx_clone = ctx_ref.clone();
        context.set("suggest", self.lua.create_function(move |_, msg: String| {
            ctx_clone.borrow_mut().suggestions.push(msg);
            Ok(())
        })?)?;
        
        Ok(context)
    }
}
```

**Dependencies**: RS-015

---

### RS-018: Lua Lifecycle Hooks

**Goal**: Call Lua functions at recipe lifecycle events.

**Tasks**:
1. Call `on_activate(ctx)` when recipe activated
2. Call `on_deactivate(ctx)` when switching recipes
3. Call `on_session_start(ctx)` at session begin
4. Call `build_system_prompt(ctx)` for dynamic prompts
5. Integrate with existing hook system from RS-010

**Acceptance Criteria**:
- [ ] Lifecycle functions called at right times
- [ ] Context passed correctly
- [ ] Errors handled gracefully
- [ ] Dynamic prompts integrated

**Key Code**:
```rust
impl LuaSandbox {
    pub fn call_on_activate(&self, ctx: &mut LuaContext) -> Result<(), Error> {
        let globals = self.lua.globals();
        if let Ok(func) = globals.get::<mlua::Function>("on_activate") {
            let ctx_table = self.create_context_table(ctx)?;
            func.call::<()>(ctx_table)?;
        }
        Ok(())
    }
    
    pub fn call_build_system_prompt(&self, ctx: &mut LuaContext) -> Result<Option<String>, Error> {
        let globals = self.lua.globals();
        if let Ok(func) = globals.get::<mlua::Function>("build_system_prompt") {
            let ctx_table = self.create_context_table(ctx)?;
            let result: String = func.call(ctx_table)?;
            return Ok(Some(result));
        }
        Ok(None)
    }
}
```

**Dependencies**: RS-017, RS-010

---

### RS-019: Custom Commands in Lua

**Goal**: Allow defining slash commands in Lua.

**Tasks**:
1. Implement `colony.command(name, config)` registration
2. Store Lua function reference for command
3. Dispatch command execution to Lua function
4. Pass arguments to Lua function
5. Return result to user

**Acceptance Criteria**:
- [ ] `colony.command("smart-test", {...})` registers command
- [ ] Command appears in `/help`
- [ ] Command execution calls Lua function
- [ ] Arguments passed correctly
- [ ] Result displayed to user

**Key Code**:
```rust
colony.set("command", self.lua.create_function(move |lua, args: (String, mlua::Table)| {
    let (name, config) = args;
    let description: String = config.get("description")?;
    let action: mlua::Function = config.get("action")?;
    
    // Store function reference in registry
    let key = lua.create_registry_value(action)?;
    COMMAND_REGISTRY.lock().insert(name.clone(), RegisteredLuaCommand {
        description,
        function_key: key,
    });
    
    Ok(())
})?)?;
```

**Dependencies**: RS-018, RS-012

---

### RS-020: Custom Tools in Lua

**Goal**: Allow defining agent tools in Lua.

**Tasks**:
1. Implement `colony.tool(name, config)` registration
2. Generate JSON schema from Lua table
3. Register tool with agent
4. Dispatch tool execution to Lua function
5. Return structured result

**Acceptance Criteria**:
- [ ] `colony.tool("project_summary", {...})` registers tool
- [ ] Tool appears in agent's available tools
- [ ] LLM can call the tool
- [ ] Lua function executes
- [ ] Result returned to LLM

**Dependencies**: RS-018

---

## Phase 4: Registry Integration (Days 39-50)

### RS-021: Registry Client (Read)

**Goal**: Implement client to read from Colony Cloud registry.

**Tasks**:
1. Create `RegistryClient` struct
2. Implement `search(query, tags)` method
3. Implement `get_package(namespace, name)` method
4. Implement `download(namespace, name, version)` method
5. Cache downloaded packages locally

**Acceptance Criteria**:
- [ ] Search returns matching packages
- [ ] Package metadata fetched
- [ ] Download saves to cache directory
- [ ] Caching prevents re-downloads

**Key Code**:
```rust
pub struct RegistryClient {
    base_url: String,
    api_key: Option<String>,
    http: reqwest::Client,
}

impl RegistryClient {
    pub fn new() -> Self {
        Self {
            base_url: "https://api.colony.sh".into(),
            api_key: std::env::var("COLONY_API_KEY").ok(),
            http: reqwest::Client::new(),
        }
    }
    
    pub async fn search(&self, query: &str, tags: &[&str]) -> Result<Vec<PackageInfo>, Error> {
        let url = format!("{}/v1/registry/search", self.base_url);
        let response = self.http.get(&url)
            .query(&[("q", query)])
            .query(&tags.iter().map(|t| ("tags", *t)).collect::<Vec<_>>())
            .send()
            .await?;
        
        Ok(response.json().await?)
    }
    
    pub async fn download(&self, namespace: &str, name: &str, version: &str) -> Result<PathBuf, Error> {
        let cache_dir = dirs::cache_dir()
            .unwrap()
            .join("colony/registry")
            .join(namespace)
            .join(name)
            .join(version);
        
        if cache_dir.exists() {
            return Ok(cache_dir);
        }
        
        let url = format!("{}/v1/registry/recipes/{}/{}/{}/download", 
            self.base_url, namespace, name, version);
        let content = self.http.get(&url).send().await?.bytes().await?;
        
        tokio::fs::create_dir_all(&cache_dir).await?;
        tokio::fs::write(cache_dir.join("recipe.toml"), &content).await?;
        
        Ok(cache_dir)
    }
}
```

**Dependencies**: RS-001

---

### RS-022: CLI: recipe install/update

**Goal**: Implement recipe installation from registry.

**Tasks**:
1. Implement `recipe install <name>` command
2. Implement `recipe install <name>@<version>` with version
3. Implement `recipe update` to update all
4. Implement `recipe update <name>` to update specific
5. Show download progress

**Acceptance Criteria**:
- [ ] `colony recipe install official/coding-agent` downloads recipe
- [ ] Version specifier works
- [ ] Update checks for new versions
- [ ] Progress shown during download

**Dependencies**: RS-021

---

### RS-023: Version Resolution

**Goal**: Implement semver-compatible version resolution.

**Tasks**:
1. Parse version constraints (`^1.0.0`, `~1.2.0`, `1.2.3`)
2. Fetch available versions from registry
3. Select best matching version
4. Handle transitive dependency versions
5. Report conflicts clearly

**Acceptance Criteria**:
- [ ] `^1.0.0` resolves to latest 1.x
- [ ] `~1.2.0` resolves to latest 1.2.x
- [ ] Exact version works
- [ ] Conflicts reported
- [ ] Dependencies resolved transitively

**Key Code**:
```rust
pub struct VersionResolver<'a> {
    registry: &'a RegistryClient,
}

impl<'a> VersionResolver<'a> {
    pub async fn resolve(&self, constraint: &str) -> Result<String, Error> {
        let versions = self.registry.get_versions(&package).await?;
        
        let req = semver::VersionReq::parse(constraint)?;
        
        let matching: Vec<_> = versions.iter()
            .filter(|v| req.matches(&semver::Version::parse(v).unwrap()))
            .collect();
        
        matching.last()
            .cloned()
            .ok_or(Error::NoMatchingVersion(constraint.to_string()))
    }
}
```

**Dependencies**: RS-021

---

### RS-024: Lockfile Generation

**Goal**: Generate and respect lockfile for reproducible builds.

**Tasks**:
1. Define lockfile format (`.colony/recipe.lock`)
2. Generate lockfile after resolution
3. Respect lockfile when installing
4. Implement `recipe update` to regenerate lockfile
5. Commit lockfile recommendation

**Acceptance Criteria**:
- [ ] Lockfile generated on install
- [ ] Same lockfile = same versions
- [ ] `recipe update` regenerates
- [ ] Lockfile format documented

**Lockfile Format**:
```toml
# .colony/recipe.lock
# Generated by Colony Shell - do not edit manually

[[package]]
name = "official/coding-agent"
version = "1.2.0"
checksum = "sha256:abc123..."

[[package]]
name = "official/bundles/file-ops"
version = "1.0.3"
checksum = "sha256:def456..."
```

**Dependencies**: RS-023

---

### RS-025: CLI: recipe publish

**Goal**: Implement recipe publishing to registry.

**Tasks**:
1. Implement `recipe publish <path>` command
2. Validate recipe before publishing
3. Require authentication (COLONY_API_KEY)
4. Upload to community namespace
5. Show published URL

**Acceptance Criteria**:
- [ ] Valid recipe publishes
- [ ] Invalid recipe shows errors
- [ ] Auth required
- [ ] URL shown on success

**Dependencies**: RS-021

---

### RS-026: Private Namespace Support

**Goal**: Support private team/org namespaces.

**Tasks**:
1. Handle `private/` namespace prefix
2. Require Pro/Enterprise subscription
3. Implement org membership check
4. Download private packages with auth
5. Clear error for unauthorized access

**Acceptance Criteria**:
- [ ] `private/myorg/tool` works for members
- [ ] Non-members get 403
- [ ] Free tier gets subscription prompt

**Dependencies**: RS-021

---

## Phase 5: Built-in Recipes (Days 51-56)

### RS-027: Built-in Recipes

**Goal**: Ship default recipes with Colony Shell.

**Tasks**:
1. Create `recipes/` directory in colony-shell crate
2. Embed recipes using `include_str!`
3. Register built-in recipes in loader
4. Create: coding-agent, chat, research, minimal

**Recipes to Create**:
```
crates/colony-shell/recipes/
├── coding-agent.toml   # Full Claude Code-like experience
├── chat.toml           # Simple chat, no tools
├── research.toml       # Web search + summarization
└── minimal.toml        # Bare minimum for testing
```

**Dependencies**: RS-007

---

### RS-028: Built-in Skills

**Goal**: Ship default skills with Colony Shell.

**Tasks**:
1. Create `skills/` directory
2. Embed skills using `include_str!`
3. Create: brainstorming, tdd, debugging, code-review, explain

**Skills to Create**:
```
crates/colony-shell/skills/
├── brainstorming.toml
├── tdd.toml
├── debugging.toml
├── code-review.toml
└── explain.toml
```

**Dependencies**: RS-003

---

### RS-029: Built-in Bundles

**Goal**: Ship default tool bundles with Colony Shell.

**Tasks**:
1. Create `bundles/` directory
2. Embed bundles using `include_str!`
3. Create: file-ops, git-ops, shell-exec, web-search

**Bundles to Create**:
```
crates/colony-shell/bundles/
├── file-ops.toml      # read_file, write_file, list_directory
├── git-ops.toml       # git status, diff, commit, push
├── shell-exec.toml    # execute_command
└── web-search.toml    # web_search (requires MCP server)
```

**Dependencies**: RS-004

---

### RS-030: Project Type Auto-Detection

**Goal**: Detect project type and suggest appropriate recipe.

**Tasks**:
1. Check for marker files (Cargo.toml, package.json, etc.)
2. Determine project language/framework
3. Suggest matching recipe if available
4. Prompt user for confirmation
5. Remember choice for project

**Acceptance Criteria**:
- [ ] Rust project detected (Cargo.toml)
- [ ] Node project detected (package.json)
- [ ] Python project detected (pyproject.toml)
- [ ] Suggestion shown on first visit
- [ ] Choice persisted

**Detection Logic**:
```rust
pub fn detect_project_type(root: &Path) -> Option<ProjectType> {
    if root.join("Cargo.toml").exists() {
        Some(ProjectType::Rust)
    } else if root.join("package.json").exists() {
        Some(ProjectType::Node)
    } else if root.join("pyproject.toml").exists() {
        Some(ProjectType::Python)
    } else if root.join("go.mod").exists() {
        Some(ProjectType::Go)
    } else {
        None
    }
}

pub fn suggest_recipe(project_type: &ProjectType) -> Option<&'static str> {
    match project_type {
        ProjectType::Rust => Some("official/rust-workspace"),
        ProjectType::Node => Some("official/node-project"),
        ProjectType::Python => Some("official/python-project"),
        ProjectType::Go => Some("official/go-project"),
    }
}
```

**Dependencies**: RS-005

---

### RS-031: .colonyrc Recipe Override

**Goal**: Allow project-specific recipe overrides in .colonyrc.

**Tasks**:
1. Add `[recipe]` section to .colonyrc schema
2. Support `use = "recipe-name"` to specify default recipe
3. Support `[recipe.override.*]` to override recipe settings
4. Merge overrides with loaded recipe
5. Document override options

**Acceptance Criteria**:
- [ ] `.colonyrc` can specify default recipe
- [ ] Override sections applied to recipe
- [ ] Prompt additions work
- [ ] Tool approval overrides work

**Example**:
```toml
# .colonyrc
version = 1

[recipe]
use = "official/coding-agent"

[recipe.override.prompt]
context_files = [
    "CLAUDE.md",
    "docs/CONTRIBUTING.md",
]

[recipe.override.tools.approval]
auto_approve = ["write_file", "execute_command"]

[recipe.override.model]
provider = "anthropic"
name = "claude-3-opus"
```

**Dependencies**: RS-007, F020

---

## Risk Mitigation

| Risk | Impact | Probability | Mitigation |
|------|--------|-------------|------------|
| Lua sandbox escape | High | Low | Extensive testing, block all dangerous APIs |
| Registry downtime | Medium | Low | Graceful fallback to cached packages |
| Import cycles | Medium | Medium | Cycle detection in resolver |
| Large recipe files | Low | Low | Size limits on registry, streaming parse |
| Plugin conflicts | Medium | Medium | Namespace isolation, priority ordering |

---

## Success Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| Recipe load time | < 100ms | Benchmark test |
| Import resolution | < 500ms | Benchmark test |
| Built-in recipe coverage | 4+ recipes | Manual count |
| Built-in skill coverage | 5+ skills | Manual count |
| Registry latency | < 1s | CloudWatch |

---

## Open Questions

1. **Recipe inheritance**: Should recipes be able to inherit from other recipes?
   - Current: No inheritance, only imports
   - Recommendation: Keep simple, imports are sufficient

2. **Lua version**: Which Lua version to support?
   - Current: Lua 5.4 via mlua
   - Recommendation: Stick with 5.4, most modern

3. **Hot reload**: Should recipe changes hot-reload?
   - Current: Requires `recipe use` to reload
   - Recommendation: Add file watcher in future version

---

## Appendix: Feature Checklist Format

Each feature when implemented should have:

```markdown
## RS-XXX: [Feature Name]

### Status: [Not Started | In Progress | Complete]

### Implementation Notes
- [Any deviations from plan]
- [Technical decisions made]

### Test Coverage
- [ ] Unit tests
- [ ] Integration tests
- [ ] BATS tests (if CLI)

### Review Checklist
- [ ] Code reviewed
- [ ] Documentation updated
- [ ] Examples added
- [ ] Manually verified
```
