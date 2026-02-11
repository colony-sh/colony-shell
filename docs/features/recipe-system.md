# Recipe System (F055)

## Overview

The Recipe System provides declarative TOML configuration for defining LLM-powered agents, tools, and workflows in Colony Shell. Recipes combine system prompts, available tools, UI layouts, and behaviors into reusable, shareable configurations.

## What is a Recipe?

A recipe is a TOML file that defines:

- **Agent configuration** - Model, provider, temperature, context
- **Tools** - Available functions the agent can invoke
- **System prompts** - Instructions that guide agent behavior
- **UI layouts** - Custom prompt components and styles
- **Behaviors** - Event hooks, auto-completion, shortcuts
- **Plugins** - Extensibility through subagents and commands

## Recipe Structure

### Basic Recipe

```toml
[metadata]
name = "code-reviewer"
version = "1.0.0"
author = "Colony"
description = "Automated code review with security focus"

[agent]
provider = "anthropic"
model = "claude-opus-4"
temperature = 0.2
max_tokens = 4096

[agent.system_prompt]
content = """
You are an expert code reviewer with deep knowledge of:
- Security best practices
- Performance optimization
- Code maintainability
- Testing strategies

Analyze code thoroughly and provide actionable feedback.
"""

[agent.tools.review]
description = "Review code for issues"
parameters = ["file", "focus"]

[agent.tools.suggest]
description = "Suggest improvements"
parameters = ["file", "type"]
```

### Advanced Recipe with Multiple Agents

```toml
[metadata]
name = "dev-assistant"
version = "2.0.0"

[agent]
provider = "anthropic"
model = "claude-opus-4"

[agent.subagents.tester]
provider = "openai"
model = "gpt-4"
specialization = "test generation"

[agent.subagents.documenter]
provider = "anthropic"
model = "claude-sonnet-4"
specialization = "documentation writing"

[behaviors]
auto_context = true
file_tracking = true
```

## Recipe Components

### Metadata

Required information about the recipe:

```toml
[metadata]
name = "recipe-name"          # Unique identifier
version = "1.0.0"             # Semantic version
author = "Your Name"          # Creator
description = "What it does"  # Brief description
tags = ["code", "review"]     # Searchable tags
license = "MIT"               # Optional license
```

### Agent Configuration

Define the LLM backend:

```toml
[agent]
provider = "anthropic"        # "anthropic", "openai", "ollama"
model = "claude-opus-4"       # Model identifier
temperature = 0.7             # Creativity (0.0-1.0)
max_tokens = 4096             # Response length limit
top_p = 0.9                   # Nucleus sampling
```

### System Prompts

Instructions that guide agent behavior:

```toml
[agent.system_prompt]
content = """
Your instructions here...
"""

# Or load from file
file = "prompts/code-reviewer.txt"
```

### Tools

Functions the agent can invoke:

```toml
[agent.tools.search]
description = "Search documentation"
parameters = ["query", "filters"]
handler = "builtin:search"

[agent.tools.execute]
description = "Run shell command"
parameters = ["command"]
handler = "builtin:exec"
requires_confirmation = true

[agent.tools.custom]
description = "Custom tool"
parameters = ["input"]
handler = "proc:my_custom_handler"
```

### UI Configuration

Customize the prompt display:

```toml
[ui.prompt]
format = "compact"           # "compact", "full", "minimal"
components = [
    "git_branch",
    "git_status",
    "duration",
    "exit_status"
]

[ui.colors]
primary = "#00ff00"
secondary = "#ffff00"
error = "#ff0000"
```

### Behaviors

Event hooks and automation:

```toml
[behaviors]
auto_context = true           # Automatically include git context
file_tracking = true          # Track file changes
on_error = "proc:handle_error"
on_success = "proc:celebrate"

[behaviors.hooks]
pre_execute = "validate_input"
post_execute = "log_result"
```

## Using Recipes

### Installation

Install a recipe from a file:

```colony
colony install ./code-reviewer.toml
```

Install from a URL:

```colony
colony install https://recipes.colony.sh/code-reviewer.toml
```

Install from the Colony registry:

```colony
colony install code-reviewer
```

### Activation

Activate a recipe in the current shell:

```colony
colony recipe use code-reviewer
```

Or use the `@agent` syntax directly:

```colony
@code-reviewer "review main.rs"
```

### Listing Recipes

Show installed recipes:

```colony
colony recipe list
```

Show recipe details:

```colony
colony recipe show code-reviewer
```

### Publishing

Publish a recipe to the Colony registry:

```colony
colony publish code-reviewer.toml
```

## Recipe Examples

### Security Auditor

```toml
[metadata]
name = "security-auditor"
version = "1.0.0"
description = "Security-focused code analysis"
tags = ["security", "audit"]

[agent]
provider = "anthropic"
model = "claude-opus-4"
temperature = 0.1  # Low temperature for consistent analysis

[agent.system_prompt]
content = """
You are a security expert specializing in:
- SQL injection prevention
- XSS vulnerabilities
- Authentication/authorization flaws
- Cryptographic weaknesses
- Dependency vulnerabilities

Always err on the side of caution.
"""

[agent.tools.scan]
description = "Scan code for vulnerabilities"
parameters = ["file", "severity"]

[agent.tools.suggest_fix]
description = "Suggest security fixes"
parameters = ["vulnerability"]

[behaviors]
auto_context = true
file_tracking = true
```

Usage:

```colony
@security-auditor "audit authentication system"
@security-auditor:scan "check api/auth.rs" "critical"
```

### Documentation Generator

```toml
[metadata]
name = "doc-generator"
version = "1.0.0"
description = "Generate comprehensive documentation"

[agent]
provider = "anthropic"
model = "claude-sonnet-4"
temperature = 0.5

[agent.system_prompt]
content = """
You are a documentation specialist. Generate clear,
comprehensive documentation following best practices:
- Start with overview and purpose
- Include examples for each feature
- Document edge cases and gotchas
- Use consistent formatting
"""

[agent.tools.generate]
description = "Generate documentation"
parameters = ["source_file", "format"]

[agent.tools.update]
description = "Update existing docs"
parameters = ["doc_file", "changes"]

[ui.prompt]
components = ["git_branch", "model"]
```

### Test Generator

```toml
[metadata]
name = "test-generator"
version = "1.0.0"
description = "Generate comprehensive test suites"

[agent]
provider = "openai"
model = "gpt-4"

[agent.system_prompt]
content = """
Generate thorough test suites covering:
- Happy path scenarios
- Edge cases
- Error conditions
- Boundary values
- Invalid inputs
"""

[agent.tools.unit]
description = "Generate unit tests"
parameters = ["function"]

[agent.tools.integration]
description = "Generate integration tests"
parameters = ["module"]
```

## Implementation Details

### Files Involved

- `crates/colony-recipe/` - New crate for recipe management
  - `src/loader.rs` - TOML parsing and validation
  - `src/registry.rs` - Recipe storage and lookup
  - `src/types.rs` - Recipe data structures
- `crates/colony-exec/src/builtins.rs` - `colony install`, `colony publish` commands
- `crates/colony-agent/src/providers/` - Agent provider integration

### Recipe Storage

Recipes are stored in:

```
~/.colony/recipes/
├── code-reviewer/
│   ├── recipe.toml
│   ├── prompts/
│   └── tools/
├── security-auditor/
│   └── recipe.toml
└── registry.json  # Installed recipe metadata
```

### Recipe Resolution

1. Check local file path
2. Check installed recipes in `~/.colony/recipes/`
3. Check Colony registry (if online)
4. Return error if not found

### Tool Handlers

Tools can be implemented as:

- **Builtin handlers**: `builtin:search`, `builtin:exec`
- **Proc handlers**: `proc:my_handler`
- **External commands**: `cmd:custom-tool`
- **MCP servers**: `mcp:server_name.tool_name`

## Recipe Distribution

### Local Distribution

Share recipe files directly:

```colony
# Copy to colleague
cp code-reviewer.toml /shared/recipes/

# Install locally
colony install /shared/recipes/code-reviewer.toml
```

### URL Distribution

Host recipes on web servers:

```colony
colony install https://example.com/recipes/reviewer.toml
```

### Registry Distribution

Publish to Colony registry:

```colony
# Login to registry
colony login

# Publish recipe
colony publish code-reviewer.toml

# Others can install
colony install code-reviewer
```

## Best Practices

### Naming Conventions

- Use kebab-case: `code-reviewer`, `test-generator`
- Be descriptive: `security-auditor` not `auditor`
- Avoid version numbers in name: use `version` field

### System Prompts

- Be specific about agent role and expertise
- Include output format requirements
- Specify constraints and limitations
- Provide examples of desired behavior

### Temperature Settings

- 0.0-0.3: Consistent, deterministic (code review, security)
- 0.4-0.7: Balanced creativity (general assistance)
- 0.8-1.0: High creativity (brainstorming, ideation)

### Tool Design

- Keep tools focused and single-purpose
- Use clear, descriptive names
- Require confirmation for destructive operations
- Provide helpful parameter descriptions

### Versioning

Follow semantic versioning:
- MAJOR: Breaking changes to interface
- MINOR: New features, backward compatible
- PATCH: Bug fixes, no new features

## Testing

The recipe system has 17 tests covering:

- TOML parsing and validation
- Recipe loading from files
- Registry operations (install, list, remove)
- Tool handler resolution
- System prompt loading
- Agent configuration validation

Run tests with:

```bash
cargo test -p colony-recipe
```

## Future Enhancements

Potential future additions:

- Recipe templates: `colony recipe new --template=basic`
- Recipe composition: Combine multiple recipes
- Dynamic prompts: Template variables in prompts
- Recipe marketplace: Browse and rate recipes
- Version constraints: Dependency resolution
- Recipe linting: Validate best practices
- Hot reload: Update recipe without restart
- Recipe analytics: Track usage and performance
