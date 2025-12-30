# Recipe System Design

**Date**: 2025-12-29  
**Status**: Draft  
**Author**: Brainstorming session

## Overview

**Recipes** are declarative configuration units that define an LLM "mode" - combining system prompts, tools, skills, UI layout, and behaviors into a switchable profile.

The recipe system serves three purposes:
1. **Configuration portability** - Share/distribute agent configurations as a unit
2. **Mode switching** - Rapidly switch between different agent personas/toolsets
3. **Extensibility platform** - Enable third-party plugins that bundle prompts + tools + UI

## Core Concepts

| Concept | Description |
|---------|-------------|
| **Recipe** | Complete configuration for an agent mode. TOML file with optional Lua. |
| **Plugin** | A recipe that provides prompts, commands, hooks, and/or subagents for other recipes to import. |
| **Skill** | A reusable prompt template/workflow. Can be standalone file or inline in recipe. |
| **Bundle** | A collection of related tools/MCP servers packaged together. |

### Hierarchy

```
Recipe (top-level configuration)
├── imports plugins, skills, bundles
├── defines system prompt + context files
├── configures tools + approval policies
├── defines inline skills + commands
├── specifies UI layout + keybindings
└── sets model + behavior preferences
```

### File Locations (Priority Order)

```
1. .colony/recipes/          # Project-specific (highest priority)
2. ~/.config/colony/recipes/ # User personal
3. ~/.cache/colony/registry/ # Downloaded from Colony Cloud
4. (built-in)                # Ships with Colony Shell (lowest priority)
```

### Namespaces

| Namespace | Source | Example |
|-----------|--------|---------|
| `official/` | Colony Cloud curated | `official/coding-agent` |
| `community/` | Public community | `community/mcfearsome/rust-dev` |
| `private/` | Team/org (Pro+) | `private/myorg/internal-tools` |
| `local/` | User's local files | `local/my-custom-recipe` |
| (none) | Project `.colony/` | `my-project-recipe` |

## Recipe File Format

### Basic Recipe (TOML)

```toml
# ~/.config/colony/recipes/coding-agent.toml

[recipe]
name = "coding-agent"
description = "Claude Code-style coding assistant"
version = "1.0.0"

# Import from other sources
imports = [
    "official/bundles/file-ops",      # Tool bundle
    "official/bundles/git-ops",       # Tool bundle
    "official/skills/brainstorming",  # Skill
    "official/skills/tdd",            # Skill
]

#─────────────────────────────────────────────────────────────
# SYSTEM PROMPT
#─────────────────────────────────────────────────────────────
[prompt]
system = """
You are an expert software engineer assistant.
You have access to file system tools, git operations, and shell execution.
Always explain your reasoning before making changes.
"""

# Context files to inject after system prompt (if they exist)
context_files = [
    "CLAUDE.md",
    "AGENTS.md",
    ".cursorrules",
    ".colony/rules.md",
]

#─────────────────────────────────────────────────────────────
# TOOLS & PERMISSIONS
#─────────────────────────────────────────────────────────────
[tools]
# MCP servers to start
mcp_servers = [
    { name = "filesystem", command = "npx", args = ["-y", "@anthropic/mcp-filesystem", "."] },
    { name = "git", command = "npx", args = ["-y", "@anthropic/mcp-git"] },
]

# Tool approval policies
[tools.approval]
auto_approve = [
    "read_file",
    "list_directory", 
    "git_status",
    "git_diff",
]
require_approval = [
    "write_file",
    "execute_command",
    "git_commit",
    "git_push",
]
deny = [
    "rm_rf",
]

#─────────────────────────────────────────────────────────────
# SKILLS (inline definitions)
#─────────────────────────────────────────────────────────────
[skills.code-review]
trigger = "/review"
description = "Review code for issues"
prompt = """
Review the following code for:
1. Bugs and logic errors
2. Security vulnerabilities
3. Performance issues
4. Style violations
"""

[skills.explain]
trigger = "/explain"
description = "Explain code in detail"
prompt = "Explain this code step by step, including the reasoning behind design decisions:"

#─────────────────────────────────────────────────────────────
# SLASH COMMANDS
#─────────────────────────────────────────────────────────────
[[commands]]
name = "test"
description = "Run project tests"
action = "shell"
command = "cargo test"

[[commands]]
name = "build"
description = "Build the project"
action = "shell"
command = "cargo build"

[[commands]]
name = "commit"
description = "Stage and commit changes"
action = "agent"
prompt = "Review staged changes, generate commit message, and commit"

#─────────────────────────────────────────────────────────────
# UI CONFIGURATION
#─────────────────────────────────────────────────────────────
[ui]
layout = "coding"  # Preset: coding, chat, research, minimal

# Override specific zones
[ui.zones]
left_sidebar = ["file-tree", "git-status"]
right_sidebar = ["agent-tasks", "symbols"]
bottom_pane = ["terminal"]

[ui.features]
diff_viewer = true
inline_completions = true
ghost_text = true

#─────────────────────────────────────────────────────────────
# KEYBINDINGS (mode-specific)
#─────────────────────────────────────────────────────────────
[keybindings]
"ctrl+enter" = "submit"
"ctrl+shift+enter" = "submit_and_run"
"ctrl+k" = "slash_command_palette"
"ctrl+/" = "toggle_agent"

#─────────────────────────────────────────────────────────────
# MODEL & BEHAVIOR
#─────────────────────────────────────────────────────────────
[model]
provider = "colony"           # or "anthropic", "openai", "ollama"
name = "claude-3.5-sonnet"
temperature = 0.7

[behavior]
streaming = true
auto_continue = true          # Continue after tool results without user prompt
max_turns = 50
checkpoint_interval = 10      # Auto-save every N turns
```

### Standalone Skill File

```toml
# ~/.config/colony/skills/brainstorming.toml

[skill]
name = "brainstorming"
description = "Structured brainstorming process"
trigger = "/brainstorm"
version = "1.0.0"

[skill.parameters]
topic = { type = "string", required = true, description = "What to brainstorm about" }
depth = { type = "string", default = "medium", enum = ["quick", "medium", "deep"] }

[skill.prompt]
template = """
Let's brainstorm about: {{topic}}

Depth level: {{depth}}

Follow this process:
1. **Diverge**: Generate 10+ ideas without judgment
2. **Converge**: Evaluate and rank ideas by feasibility and impact
3. **Develop**: Expand top 3 ideas with concrete next steps
4. **Decide**: Recommend best option with clear reasoning

{% if depth == "deep" %}
For each top idea, also include:
- Potential risks and mitigations
- Resource requirements
- Timeline estimate
{% endif %}
"""
```

### Tool Bundle File

```toml
# ~/.config/colony/bundles/git-ops.toml

[bundle]
name = "git-ops"
description = "Git operations toolkit"
version = "1.0.0"

[mcp]
servers = [
    { name = "git", command = "npx", args = ["-y", "@anthropic/mcp-git"] },
]

[tools]
# Built-in tools to enable
enable = [
    "git_status",
    "git_diff",
    "git_log",
    "git_commit",
    "git_push",
    "git_pull",
    "git_branch",
]

[approval]
auto_approve = ["git_status", "git_diff", "git_log"]
require_approval = ["git_commit", "git_push", "git_pull"]
```

## Plugin Format

Plugins are recipes that provide reusable components for other recipes to import. They can define prompts, commands, hooks, and subagents.

### Plugin File Structure

```toml
# ~/.config/colony/plugins/pr-review-toolkit.toml

[plugin]
name = "pr-review-toolkit"
description = "Pull request review tools and agents"
version = "1.0.0"
author = "colony"

#─────────────────────────────────────────────────────────────
# PROMPT FRAGMENTS (injected into importing recipe's prompt)
#─────────────────────────────────────────────────────────────
[prompt]
# Appended to system prompt when plugin is imported
inject = """
## PR Review Guidelines

When reviewing code:
- Focus on logic errors, security issues, and performance
- Be constructive, not critical
- Suggest specific improvements with examples
"""

# Injected only when specific conditions match
[[prompt.conditional]]
when = "file_extension == 'rs'"
inject = """
For Rust code, also check:
- Proper error handling (no unwrap in library code)
- Ownership and borrowing correctness
- Missing documentation on public APIs
"""

#─────────────────────────────────────────────────────────────
# SLASH COMMANDS
#─────────────────────────────────────────────────────────────
[[commands]]
name = "review"
description = "Review current PR or staged changes"
action = "agent"
agent = "code-reviewer"  # Use subagent defined below
prompt = "Review the staged changes or current PR for issues"

[[commands]]
name = "review-file"
description = "Review a specific file"
action = "agent"
agent = "code-reviewer"
prompt = "Review {{file}} for issues"
parameters = [
    { name = "file", type = "path", required = true }
]

#─────────────────────────────────────────────────────────────
# HOOKS (lifecycle interception)
#─────────────────────────────────────────────────────────────
[hooks]

# Before any tool executes
[hooks.before_tool]
match = ["write_file", "git_commit"]
action = "prompt"
prompt = "Before making this change, have you considered the impact on existing tests?"

# After tool completes
[hooks.after_tool]
match = ["git_commit"]
action = "suggest"
suggestion = "Run /review to check for issues before pushing"

# On session start
[hooks.on_session_start]
action = "prompt"
prompt = "Check if there's an active PR branch with `git status`"

# On error
[hooks.on_error]
match = ["test_failed", "build_failed"]
action = "agent"
agent = "debugger"
prompt = "Analyze this failure and suggest fixes"

#─────────────────────────────────────────────────────────────
# SUBAGENTS (specialized agent types)
#─────────────────────────────────────────────────────────────
[agents.code-reviewer]
name = "code-reviewer"
description = "Reviews code for bugs, security, and style issues"
system_prompt = """
You are a senior code reviewer. Your job is to:
1. Identify bugs and logic errors
2. Flag security vulnerabilities
3. Note performance concerns
4. Check style consistency

Be constructive. Suggest fixes, not just problems.
Rate confidence: HIGH (certain issue), MEDIUM (likely issue), LOW (nitpick).
"""
tools = ["read_file", "git_diff", "grep"]
model = "claude-3.5-sonnet"

[agents.debugger]
name = "debugger"
description = "Analyzes failures and suggests fixes"
system_prompt = """
You are a debugging expert. When given an error:
1. Identify the root cause
2. Explain why it happened
3. Suggest specific fixes
4. Recommend preventive measures
"""
tools = ["read_file", "grep", "execute_command"]
model = "claude-3.5-sonnet"

[agents.test-writer]
name = "test-writer"
description = "Generates tests for code"
system_prompt = """
You are a test engineering expert. Write comprehensive tests that:
1. Cover happy path
2. Cover edge cases
3. Test error conditions
4. Are readable and maintainable
"""
tools = ["read_file", "write_file", "execute_command"]
model = "claude-3.5-sonnet"
```

### Hook Types

| Hook | Trigger | Available Actions |
|------|---------|-------------------|
| `before_tool` | Before tool execution | `prompt`, `confirm`, `modify`, `block` |
| `after_tool` | After tool completion | `prompt`, `suggest`, `agent` |
| `on_session_start` | Session begins | `prompt`, `agent`, `shell` |
| `on_session_end` | Session ends | `prompt`, `shell` |
| `on_error` | Error occurs | `prompt`, `agent`, `suggest` |
| `on_message` | Each user message | `prompt`, `transform` |

### Hook Actions

| Action | Description |
|--------|-------------|
| `prompt` | Inject text into the conversation |
| `suggest` | Show a suggestion to the user (non-blocking) |
| `confirm` | Require user confirmation to proceed |
| `block` | Prevent the action entirely |
| `modify` | Transform the input/output |
| `agent` | Delegate to a subagent |
| `shell` | Execute a shell command |
| `transform` | Modify message content |

### Importing Plugins

```toml
# In a recipe
[recipe]
name = "my-coding-setup"

imports = [
    "official/plugins/pr-review-toolkit",
    "community/someuser/rust-helpers",
]

# Override plugin settings if needed
[plugins.pr-review-toolkit]
# Disable specific hooks
disable_hooks = ["on_session_start"]

# Override agent model
[plugins.pr-review-toolkit.agents.code-reviewer]
model = "claude-3-opus"
```

## Lua Scripting (Advanced)

For recipes that need programmatic logic beyond declarative TOML.

### When to Use Lua

| Use Case | TOML | Lua |
|----------|------|-----|
| Static configuration | ✅ | ❌ |
| Simple conditionals | ✅ (limited) | ✅ |
| Dynamic prompt generation | ❌ | ✅ |
| Complex hooks with logic | ❌ | ✅ |
| Project type detection | ❌ | ✅ |
| Custom tool implementations | ❌ | ✅ |

### Lua Integration

Recipes can include a `recipe.lua` alongside `recipe.toml`, or embed Lua inline:

```toml
[recipe]
name = "smart-coding"
lua_file = "smart-coding.lua"  # External Lua file
```

Or inline:

```toml
[recipe]
name = "smart-coding"

[lua]
script = """
-- Inline Lua script
function on_activate(ctx)
    if ctx.project.has_file("Cargo.toml") then
        ctx.import("official/bundles/rust-tools")
    elseif ctx.project.has_file("package.json") then
        ctx.import("official/bundles/node-tools")
    end
end
"""
```

### Lua API

```lua
-- smart-coding.lua

-----------------------------------------------------------
-- LIFECYCLE HOOKS
-----------------------------------------------------------

function on_activate(ctx)
    -- Called when recipe is activated
    print("Recipe activated for: " .. ctx.project.root)
end

function on_deactivate(ctx)
    -- Called when switching away from this recipe
end

function on_session_start(ctx)
    local branch = colony.shell("git branch --show-current")
    if branch:match("^feature/") then
        ctx.prompt.append("You're on a feature branch. Remember to write tests.")
    end
end

-----------------------------------------------------------
-- DYNAMIC PROMPT GENERATION
-----------------------------------------------------------

function build_system_prompt(ctx)
    local base = [[You are an expert software engineer.]]
    
    if ctx.project.language == "rust" then
        base = base .. [[

## Rust Guidelines
- Use `thiserror` for library errors
- No `unwrap()` in library code
]]
    end
    
    return base
end

-----------------------------------------------------------
-- HOOK IMPLEMENTATIONS
-----------------------------------------------------------

function before_tool(ctx, tool_name, input)
    if tool_name == "write_file" and input.path:match("%.env") then
        return false, "Cannot modify .env files directly"
    end
    return nil  -- Proceed normally
end

function after_tool(ctx, tool_name, result)
    if tool_name == "execute_command" and result.exit_code ~= 0 then
        ctx.suggest("Command failed. Want me to analyze the error?")
    end
end

-----------------------------------------------------------
-- CUSTOM COMMANDS
-----------------------------------------------------------

colony.command("smart-test", {
    description = "Run tests intelligently based on changes",
    action = function(ctx, args)
        local changed = colony.shell("git diff --name-only HEAD~1")
        -- ... smart test logic
    end
})

-----------------------------------------------------------
-- CUSTOM TOOLS
-----------------------------------------------------------

colony.tool("project_summary", {
    description = "Get a summary of the current project",
    parameters = {},
    execute = function(ctx, input)
        return {
            root = ctx.project.root,
            language = ctx.project.language,
            files = tonumber(colony.shell("find . -type f | wc -l")),
        }
    end
})
```

### Sandboxing

Lua runs in a restricted environment:

| Allowed | Blocked |
|---------|---------|
| `colony.*` API | `os.execute` |
| `string.*` | `io.*` (use colony.read_file) |
| `table.*` | `loadfile`, `dofile` |
| `math.*` | `require` (use colony.import) |
| `ipairs`, `pairs`, `type` | `debug.*` |

### Colony Lua API Reference

```lua
-- File operations
colony.read_file(path) -> string
colony.write_file(path, content)
colony.file_exists(path) -> bool
colony.list_dir(path) -> table

-- Shell
colony.shell(command) -> string (stdout)
colony.shell_async(command) -> handle

-- JSON/TOML
colony.json_encode(table) -> string
colony.json_decode(string) -> table
colony.toml_decode(string) -> table

-- Prompt manipulation
ctx.prompt.append(text)
ctx.prompt.prepend(text)
ctx.prompt.replace(pattern, replacement)

-- Suggestions/UI
ctx.suggest(message)
ctx.notify(message, level)  -- level: info, warn, error

-- Imports
ctx.import(recipe_or_bundle)
ctx.import_skill(skill_name)

-- Session state
ctx.session.get(key) -> value
ctx.session.set(key, value)

-- Project info
ctx.project.root -> string
ctx.project.language -> string
ctx.project.has_file(path) -> bool

-- Register extensions
colony.command(name, config)
colony.tool(name, config)
colony.hook(event, handler)
```

## CLI Interface

### Core Commands

```bash
# ─────────────────────────────────────────────────────────────
# RECIPE MANAGEMENT
# ─────────────────────────────────────────────────────────────

# List available recipes
colony recipe list
colony recipe list --source=official
colony recipe list --source=community
colony recipe list --source=local

# Show current active recipe
colony recipe current

# Activate a recipe
colony recipe use coding-agent
colony recipe use official/coding-agent
colony recipe use community/mcfearsome/rust-dev

# Show recipe details
colony recipe show coding-agent
colony recipe show --full coding-agent

# Create new recipe (interactive)
colony recipe new my-recipe

# Edit recipe (opens in $EDITOR)
colony recipe edit my-recipe

# Validate recipe syntax
colony recipe check my-recipe

# Deactivate (return to default)
colony recipe reset

# ─────────────────────────────────────────────────────────────
# REGISTRY (Colony Cloud)
# ─────────────────────────────────────────────────────────────

# Search registry
colony recipe search "rust coding"
colony recipe search --tag=rust --tag=coding

# Install from registry
colony recipe install official/coding-agent
colony recipe install community/user/recipe-name
colony recipe install community/user/recipe-name@1.2.0

# Update installed recipes
colony recipe update
colony recipe update official/coding-agent

# Publish to registry
colony recipe publish my-recipe
colony recipe publish my-recipe --private

# Uninstall
colony recipe uninstall community/user/recipe-name

# ─────────────────────────────────────────────────────────────
# PLUGINS
# ─────────────────────────────────────────────────────────────

colony plugin list
colony plugin install official/plugins/pr-review-toolkit
colony plugin show pr-review-toolkit
colony plugin enable pr-review-toolkit
colony plugin disable pr-review-toolkit

# ─────────────────────────────────────────────────────────────
# SKILLS & BUNDLES
# ─────────────────────────────────────────────────────────────

colony skill list
colony skill show brainstorming
colony skill run brainstorming --topic="new feature ideas"
colony skill install official/skills/tdd

colony bundle list
colony bundle show git-ops
colony bundle install official/bundles/kubernetes

# ─────────────────────────────────────────────────────────────
# IMPORT/EXPORT
# ─────────────────────────────────────────────────────────────

colony recipe export > my-session.toml
colony recipe import https://github.com/user/repo/blob/main/recipe.toml
colony recipe import git@github.com:user/colony-recipes.git
colony recipe clone official/coding-agent my-custom-agent
```

### Project Config Integration

```toml
# .colonyrc (project root)
version = 1

[recipe]
use = "official/coding-agent"

# Override specific settings for this project
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

### Auto-Detection Flow

```bash
$ cd ~/projects/my-rust-app
$ colony

Detected project type: Rust (Cargo workspace)
Suggested recipe: official/rust-workspace

Use this recipe? [Y/n/always/never]: y

Activating recipe: official/rust-workspace v1.0.0
Ready. Type /help for commands.
```

## Colony Cloud Registry Integration

### Registry Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Colony Cloud                              │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐        │
│  │  Official   │  │  Community  │  │   Private   │        │
│  │  Namespace  │  │  Namespace  │  │  Namespace  │        │
│  │             │  │             │  │  (Pro/Ent)  │        │
│  │ • recipes   │  │ • recipes   │  │             │        │
│  │ • plugins   │  │ • plugins   │  │ • org repos │        │
│  │ • skills    │  │ • skills    │  │ • team only │        │
│  │ • bundles   │  │ • bundles   │  │             │        │
│  └─────────────┘  └─────────────┘  └─────────────┘        │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐  │
│  │                   Registry API                       │  │
│  │  • Search & discovery                                │  │
│  │  • Version management                                │  │
│  │  • Download/install                                  │  │
│  │  • Publish (authenticated)                           │  │
│  │  • Usage analytics                                   │  │
│  └─────────────────────────────────────────────────────┘  │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### Registry API Endpoints

```
# Public (no auth required)
GET  /v1/registry/search?q=rust+coding&tags=rust
GET  /v1/registry/recipes
GET  /v1/registry/recipes/:namespace/:name
GET  /v1/registry/recipes/:namespace/:name/versions
GET  /v1/registry/recipes/:namespace/:name/:version/download

# Authenticated (COLONY_API_KEY required)
POST /v1/registry/recipes                    # Publish
PUT  /v1/registry/recipes/:namespace/:name   # Update
DELETE /v1/registry/recipes/:namespace/:name # Unpublish

# Private namespace (Pro/Enterprise)
GET  /v1/registry/private/:org/recipes
POST /v1/registry/private/:org/recipes
```

### Version Resolution

```toml
imports = [
    "official/bundles/file-ops",           # Latest
    "official/bundles/git-ops@1.2.0",      # Exact version
    "official/skills/tdd@^1.0.0",          # Compatible (>=1.0.0 <2.0.0)
    "community/user/tool@~1.2.0",          # Patch updates only (>=1.2.0 <1.3.0)
]
```

### Lockfile

```toml
# .colony/recipe.lock (auto-generated)

[[package]]
name = "official/coding-agent"
version = "1.2.0"
checksum = "sha256:abc123..."

[[package]]
name = "official/bundles/file-ops"
version = "1.0.3"
checksum = "sha256:def456..."
```

### Pricing Integration

| Feature | Free | Pro | Enterprise |
|---------|------|-----|------------|
| Install official recipes | ✅ | ✅ | ✅ |
| Install community recipes | ✅ | ✅ | ✅ |
| Publish public recipes | ✅ | ✅ | ✅ |
| Private recipes | ❌ | 10 | Unlimited |
| Org/team namespaces | ❌ | ❌ | ✅ |
| Priority support | ❌ | ✅ | ✅ |
| Usage analytics | Basic | Full | Full + API |

## Crate Architecture

### New Crate: `colony-recipe`

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

### Core Types

```rust
/// Complete recipe configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recipe {
    pub meta: RecipeMeta,
    pub imports: Vec<String>,
    pub prompt: PromptConfig,
    pub tools: ToolsConfig,
    pub skills: HashMap<String, Skill>,
    pub commands: Vec<Command>,
    pub ui: UiConfig,
    pub keybindings: HashMap<String, String>,
    pub model: ModelConfig,
    pub behavior: BehaviorConfig,
    pub lua: Option<LuaConfig>,
}

/// Plugin definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plugin {
    pub meta: PluginMeta,
    pub prompt: PluginPrompt,
    pub commands: Vec<Command>,
    pub hooks: HooksConfig,
    pub agents: HashMap<String, Subagent>,
}

/// Skill definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub description: Option<String>,
    pub trigger: String,
    pub parameters: HashMap<String, SkillParameter>,
    pub prompt: SkillPrompt,
}

/// Tool bundle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bundle {
    pub meta: BundleMeta,
    pub mcp: McpConfig,
    pub tools: ToolsConfig,
    pub approval: ToolApprovalConfig,
}
```

### Cargo.toml

```toml
[package]
name = "colony-recipe"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
mlua = { version = "0.9", features = ["lua54", "vendored", "serialize"] }
reqwest = { version = "0.12", features = ["json"], optional = true }
dirs = "5"
thiserror = "2"
tokio = { version = "1", features = ["fs"] }

[features]
default = ["registry"]
registry = ["reqwest"]
```

## Agent Integration

### Updated Agent with Recipe Support

```rust
impl Agent {
    /// Create agent with a recipe
    pub async fn with_recipe(recipe_name: &str) -> Result<Self> {
        let loader = RecipeLoader::new();
        let recipe = loader.load_recipe(recipe_name)?;
        let resolved = RecipeResolver::new(&loader).resolve(&recipe).await?;
        Self::from_resolved_recipe(resolved).await
    }
    
    /// Switch to a different recipe
    pub async fn switch_recipe(&mut self, recipe_name: &str) -> Result<()> {
        // Run deactivation hook, load new recipe, run activation hook
    }
    
    /// Execute a slash command
    pub async fn execute_command(&mut self, name: &str, args: HashMap<String, String>) -> Result<String> {
        // Dispatch to shell, agent, skill, or lua based on command type
    }
    
    /// Run a skill
    pub async fn run_skill(&mut self, name: &str, params: HashMap<String, String>) -> Result<String> {
        // Expand template, process with agent
    }
    
    /// Run a subagent
    pub async fn run_subagent(&mut self, name: &str, prompt: &str) -> Result<String> {
        // Create temporary agent with subagent config
    }
}
```

## Built-in Recipes

### Directory Structure

```
crates/colony-shell/
├── recipes/
│   ├── base-assistant.toml
│   ├── coding-agent.toml
│   ├── chat.toml
│   ├── research.toml
│   ├── colony-app-builder.toml
│   └── minimal.toml
├── bundles/
│   ├── file-ops.toml
│   ├── git-ops.toml
│   ├── shell-exec.toml
│   └── web-search.toml
└── skills/
    ├── brainstorming.toml
    ├── tdd.toml
    ├── debugging.toml
    ├── code-review.toml
    └── explain.toml
```

### coding-agent.toml (excerpt)

```toml
[recipe]
name = "coding-agent"
description = "Full-featured coding assistant with file and git operations"
version = "1.0.0"

imports = [
    "builtin/bundles/file-ops",
    "builtin/bundles/git-ops",
    "builtin/bundles/shell-exec",
    "builtin/skills/brainstorming",
    "builtin/skills/tdd",
    "builtin/skills/debugging",
    "builtin/skills/code-review",
]

[prompt]
system = """
You are an expert software engineer assistant running in Colony Shell.
...
"""

context_files = [
    "CLAUDE.md",
    "AGENTS.md",
    ".cursorrules",
]

[model]
provider = "colony"
name = "claude-3.5-sonnet"

[behavior]
streaming = true
auto_continue = true
```

### colony-app-builder.toml

A specialized recipe for building Colony TUI apps and widgets:

```toml
[recipe]
name = "colony-app-builder"
description = "Build interactive TUI apps and widgets for Colony Shell"
version = "1.0.0"

[prompt]
system = """
You are a Colony App Builder - an expert at creating interactive TUI applications
and widgets that run inside Colony Shell.

## Colony App Platform

Colony Apps are reactive TUI applications built with:
- **Ratatui** for terminal UI rendering
- **Colony Tuple Space** for reactive state management
- **Colony Channels** for streaming data
- **WASM** for sandboxed execution (optional)

## App Types

### 1. Widgets - Small, embeddable UI components
### 2. Panes - Larger UI sections that occupy shell zones
### 3. Full Apps - Standalone TUI applications
...
"""

[skills.new-widget]
trigger = "/new-widget"
description = "Create a new Colony widget"
prompt = "Create widget scaffold..."

[skills.new-pane]
trigger = "/new-pane"
description = "Create a new Colony pane"
prompt = "Create pane scaffold..."

[skills.new-app]
trigger = "/new-app"
description = "Create a new full Colony app"
prompt = "Create app scaffold..."

[agents.ui-designer]
name = "ui-designer"
description = "Designs TUI layouts and components"
system_prompt = "You are a TUI design expert specializing in Ratatui..."

[agents.state-architect]
name = "state-architect"
description = "Designs reactive state architectures"
system_prompt = "You are a reactive systems architect..."
```

## Key Decisions Summary

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Format | TOML + optional Lua | Simple declarative + power when needed |
| Composition | Imports (explicit) | Clear what's included, no inheritance confusion |
| Distribution | Local + Git + Registry | Flexibility for all use cases |
| Registry | Colony Cloud hosted | Drives subscription value |
| Namespaces | official/community/private | Quality curation + open ecosystem |
| Plugins | Prompts + commands + hooks + subagents | Claude Code parity |
| Skills | Standalone or inline | Maximum flexibility |
| UI config | Presets + zone overrides | Simple default, customizable |
| Activation | CLI + auto-detect + project config | Right tool for each context |

## Implementation Phases

### Phase 1: Core Recipe System (Weeks 1-2)
- [ ] `colony-recipe` crate with Recipe/Skill/Bundle parsing
- [ ] RecipeLoader for file discovery
- [ ] RecipeResolver for import resolution
- [ ] Basic agent integration (system prompt, tools)
- [ ] CLI: `colony recipe list/use/show`

### Phase 2: Plugins & Hooks (Weeks 3-4)
- [ ] Plugin format parsing
- [ ] Hook registry and execution
- [ ] Subagent spawning
- [ ] Slash command registration
- [ ] CLI: `colony plugin list/enable/disable`

### Phase 3: Lua Scripting (Week 5)
- [ ] mlua integration with sandbox
- [ ] Colony Lua API implementation
- [ ] Lifecycle hooks in Lua
- [ ] Custom commands/tools in Lua

### Phase 4: Registry Integration (Weeks 6-7)
- [ ] RegistryClient for Colony Cloud
- [ ] Install/publish workflow
- [ ] Version resolution and lockfile
- [ ] CLI: `colony recipe install/publish/update`

### Phase 5: Built-in Recipes (Week 8)
- [ ] Ship built-in recipes/bundles/skills
- [ ] Auto-detection for project types
- [ ] Documentation and examples
