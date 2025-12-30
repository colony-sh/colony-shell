# Programmable Prompt Component System

Date: 2025-12-29

## Overview

A programmable prompt component system where users compose prompts from built-in and custom components. Components react to tuple space events, produce styled output with semantic data, and render to multiple targets (ANSI terminals, ratatui TUI, OSC 2337 for colony-terminal).

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         .colonyrc                               │
│  [prompt]                                                       │
│  left = ["pwd", "git_branch", "git_status"]                    │
│  right = ["model", "jobs", "exit_status"]                      │
│  include = "~/.config/colony/prompt.toml"  # optional          │
└─────────────────────────┬───────────────────────────────────────┘
                          │ parse
                          ▼
┌─────────────────────────────────────────────────────────────────┐
│                    colony-ui-core                               │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐             │
│  │ ComponentDef│  │Subscription │  │ StyledSpan  │             │
│  │   trait     │  │  Manager    │  │   types     │             │
│  └─────────────┘  └─────────────┘  └─────────────┘             │
│                                                                 │
│  Built-ins: pwd, git_branch, git_status, jobs, exit_status,   │
│             model                                               │
└─────────────────────────┬───────────────────────────────────────┘
                          │ ComponentOutput { spans, data }
          ┌───────────────┼───────────────┐
          ▼               ▼               ▼
   ┌────────────┐  ┌────────────┐  ┌────────────┐
   │ ANSI       │  │ ratatui    │  │ OSC 2337   │
   │ Renderer   │  │ Renderer   │  │ Renderer   │
   │ (rustyline)│  │ (tui)      │  │ (terminal) │
   └────────────┘  └────────────┘  └────────────┘
```

**Data flow:**
1. On startup, parse `.colonyrc` → build component list with subscriptions
2. Components register their tuple patterns with `SubscriptionManager`
3. On tuple event (e.g., `dir_change`), manager notifies affected components
4. Components produce `ComponentOutput` (spans + semantic data)
5. Active renderer converts output to its format (ANSI escape codes, ratatui Spans, or OSC 2337 JSON)

## Core Types

```rust
// colony-ui-core/src/component.rs

/// Style for a span of text
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Style {
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: bool,
    pub italic: bool,
    pub dim: bool,
}

/// A styled piece of text
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StyledSpan {
    pub text: String,
    pub style: Style,
}

/// Output from a component render
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ComponentOutput {
    /// Text representation with styling (always present)
    pub spans: Vec<StyledSpan>,
    /// Semantic data for rich renderers (optional)
    pub data: Option<serde_json::Value>,
    /// Should this component be hidden? (e.g., exit_status when 0)
    pub visible: bool,
}

/// Tuple patterns a component subscribes to
pub type TuplePattern = Vec<colony_tuple::Value>;

/// Context passed to components during render
pub struct ComponentContext<'a> {
    pub cwd: &'a Path,
    pub env: &'a HashMap<String, String>,
    pub last_exit_code: i32,
    pub jobs: &'a [JobInfo],
    pub model: Option<&'a str>,
}

/// The trait all prompt components implement
pub trait PromptComponent: Send + Sync {
    /// Unique identifier (e.g., "git_branch")
    fn id(&self) -> &str;
    
    /// Tuple patterns this component reacts to
    fn subscriptions(&self) -> Vec<TuplePattern>;
    
    /// Render current state
    fn render(&self, ctx: &ComponentContext) -> ComponentOutput;
}
```

**Key decisions:**
- `ComponentOutput.visible` lets components hide themselves (exit_status hides when 0, git hides outside repos)
- `ComponentContext` provides all data components might need — no reaching into global state
- Components are `Send + Sync` for potential async/parallel rendering later

## Built-in Components

Each built-in component is a Rust struct implementing `PromptComponent`:

| Component | Subscribes To | Output Example | Semantic Data |
|-----------|---------------|----------------|---------------|
| `pwd` | `dir_change` | `~/src/colony-shell` (cyan) | `{ "full": "/Users/...", "truncated": "~/src/..." }` |
| `git_branch` | `dir_change` | ` main` (green) | `{ "branch": "main", "detached": false }` |
| `git_status` | `dir_change`, `file_change` | `12 • +1380 -488` | `{ "modified": 12, "insertions": 1380, "deletions": 488, "staged": 3 }` |
| `jobs` | `job_change` | `⚙ 2` (yellow) | `{ "running": 2, "stopped": 0 }` |
| `exit_status` | `cmd_complete` | `✘ 127` (red) | `{ "code": 127 }` |
| `model` | `model_change` | `gpt-5` (blue) | `{ "model": "gpt-5", "provider": "openai" }` |
| `path_status` | `env_change` (PATH) | `PATH: 12` (dim) | `{ "count": 12, "missing": ["/old/bin"] }` |

**Implementation notes:**

- **`pwd`**: Uses `std::env::current_dir()`. Truncates with `~` for home, `...` prefix for deep paths (like Warp shows `...arsome/colony-terminal/target/release`).

- **`git_branch`** and **`git_status`**: Shell out to `git` but cache results. Same `dir_change` event updates both — they should batch the git calls to avoid running git twice.

- **`git_status` diff stats**: Runs `git diff --stat HEAD` and parses the summary line. Shows modified file count + insertions/deletions like Warp.

- **`exit_status`**: Only visible when non-zero. Red color signals failure.

- **`model`**: Reads from agent state. Hidden when no agent is active.

- **`path_status`**: Shows PATH entry count. Highlights missing directories in red. Useful for debugging PATH issues. Works with the `path` builtin (`path add`, `path prepend`, `path remove`, `path list`, `path has`).

## User-Defined Components

Users can define custom components in `.colonyrc` using shell commands:

```toml
[component.docker_status]
command = "docker ps -q | wc -l | tr -d ' '"
format = "🐳 {}"           # {} is replaced with command output
style = { fg = "blue" }
subscribe = ["dir_change"]  # when to re-run
interval = 30               # also poll every 30s (optional)
condition = "command -v docker"  # only show if this succeeds
position = "right"          # default placement hint

[component.node_version]
command = "node -v 2>/dev/null"
format = "⬢ {}"
style = { fg = "green" }
subscribe = ["dir_change"]
condition = "test -f package.json"

[component.kube_context]
command = "kubectl config current-context 2>/dev/null"
format = "⎈ {}"
style = { fg = "cyan" }
subscribe = []              # only updates on interval
interval = 60
```

**Shell component execution:**

```rust
pub struct ShellComponent {
    id: String,
    command: String,
    format: String,
    style: Style,
    subscriptions: Vec<TuplePattern>,
    interval: Option<Duration>,
    condition: Option<String>,
}

impl PromptComponent for ShellComponent {
    fn render(&self, ctx: &ComponentContext) -> ComponentOutput {
        // 1. Check condition (if set)
        if let Some(cond) = &self.condition {
            if !run_condition(cond, ctx.cwd) {
                return ComponentOutput::hidden();
            }
        }
        
        // 2. Run command, capture stdout
        let output = run_command(&self.command, ctx.cwd);
        
        // 3. Format and return
        let text = self.format.replace("{}", &output.trim());
        ComponentOutput::visible(text, &self.style)
    }
}
```

**Safety:**
- Commands run with timeout (default 500ms) — slow commands don't block prompt
- Commands inherit shell's cwd but restricted env (no secrets by default)
- `interval` components run in background, cache results

## Subscription Manager

The subscription manager connects tuple space events to components:

```rust
// colony-ui-core/src/subscription.rs

pub struct SubscriptionManager {
    /// Map from event type to component IDs that care about it
    subscriptions: HashMap<String, Vec<String>>,
    /// Cached component outputs (invalidated on relevant events)
    cache: HashMap<String, ComponentOutput>,
    /// Interval-based components with last run time
    intervals: HashMap<String, (Duration, Instant)>,
}

impl SubscriptionManager {
    /// Register a component's subscriptions
    pub fn register(&mut self, component: &dyn PromptComponent) {
        for pattern in component.subscriptions() {
            // Extract event type (first element of tuple pattern)
            if let Some(Value::Str(event_type)) = pattern.first() {
                self.subscriptions
                    .entry(event_type.clone())
                    .or_default()
                    .push(component.id().to_string());
            }
        }
    }
    
    /// Called when tuple space emits an event
    pub fn on_event(&mut self, event_type: &str) {
        // Invalidate cache for affected components
        if let Some(component_ids) = self.subscriptions.get(event_type) {
            for id in component_ids {
                self.cache.remove(id);
            }
        }
    }
    
    /// Get component output (cached or fresh)
    pub fn get_output(
        &mut self, 
        component: &dyn PromptComponent,
        ctx: &ComponentContext,
    ) -> ComponentOutput {
        if let Some(cached) = self.cache.get(component.id()) {
            return cached.clone();
        }
        
        let output = component.render(ctx);
        self.cache.insert(component.id().to_string(), output.clone());
        output
    }
}
```

**Integration with tuple space:**

```rust
// In the REPL/shell main loop
let tuple_space = executor.tuple_space();

// Subscribe to all UI-relevant events
tuple_space.subscribe(("dir_change", _, _), |tuple| {
    subscription_manager.on_event("dir_change");
});

tuple_space.subscribe(("cmd_complete", _), |tuple| {
    subscription_manager.on_event("cmd_complete");
    // Also extract exit code for context
});

// ... etc for job_change, model_change, file_change
```

**Interval handling:**
- Background task checks interval components periodically
- If interval elapsed, re-render and update cache
- Prompt render always uses cache — never blocks on interval components

## Renderers

Three renderers convert `ComponentOutput` to their target format:

### ANSI Renderer (rustyline / plain terminal)

```rust
// colony-ui-core/src/render/ansi.rs

pub fn render_prompt(
    left: &[ComponentOutput],
    right: &[ComponentOutput],
    width: u16,
) -> String {
    let left_str = render_spans_ansi(left);
    let right_str = render_spans_ansi(right);
    
    // Calculate padding to right-align the right components
    let left_visible = strip_ansi(&left_str).len();
    let right_visible = strip_ansi(&right_str).len();
    let padding = width.saturating_sub(left_visible + right_visible + 2);
    
    format!("{}{:padding$}{}\n❯ ", left_str, "", right_str)
}

fn render_spans_ansi(outputs: &[ComponentOutput]) -> String {
    outputs
        .iter()
        .filter(|o| o.visible)
        .map(|o| spans_to_ansi(&o.spans))
        .collect::<Vec<_>>()
        .join("  ")  // gap between components
}

fn spans_to_ansi(spans: &[StyledSpan]) -> String {
    spans.iter().map(|s| {
        let mut codes = vec![];
        if let Some(fg) = &s.style.fg {
            codes.push(fg.to_ansi_fg());
        }
        if s.style.bold { codes.push("1"); }
        if s.style.dim { codes.push("2"); }
        
        if codes.is_empty() {
            s.text.clone()
        } else {
            format!("\x1b[{}m{}\x1b[0m", codes.join(";"), s.text)
        }
    }).collect()
}
```

### OSC 2337 Renderer (colony-terminal)

```rust
// colony-protocol/src/lib.rs (addition)

#[derive(Serialize, Deserialize)]
pub struct PromptComponent {
    pub id: String,
    pub spans: Vec<StyledSpan>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

// New protocol message type
pub enum ProtocolMessage {
    // ... existing variants ...
    Prompt {
        left: Vec<PromptComponent>,
        right: Vec<PromptComponent>,
    },
}

pub fn emit_prompt(left: &[ComponentOutput], right: &[ComponentOutput]) {
    let msg = ProtocolMessage::Prompt {
        left: outputs_to_protocol(left),
        right: outputs_to_protocol(right),
    };
    emit_message(&msg);
}
```

### Ratatui Renderer (future colony-tui)

```rust
// colony-tui/src/prompt/renderer.rs (future)

pub fn render_prompt<'a>(
    left: &[ComponentOutput],
    right: &[ComponentOutput],
) -> (Line<'a>, Line<'a>) {
    let left_spans = outputs_to_ratatui(left);
    let right_spans = outputs_to_ratatui(right);
    (Line::from(left_spans), Line::from(right_spans))
}
```

## Configuration

### Configuration parsing

```rust
// colony-ui-core/src/config.rs

#[derive(Deserialize)]
pub struct PromptConfig {
    pub left: Vec<String>,           // component IDs
    pub right: Vec<String>,
    #[serde(default)]
    pub gap: String,                  // default "  "
    pub include: Option<PathBuf>,     // optional external file
}

#[derive(Deserialize)]
pub struct ComponentDef {
    pub command: String,
    #[serde(default = "default_format")]
    pub format: String,               // default "{}"
    #[serde(default)]
    pub style: Style,
    #[serde(default)]
    pub subscribe: Vec<String>,
    pub interval: Option<u64>,        // seconds
    pub condition: Option<String>,
}

pub fn load_prompt_config(colonyrc: &Path) -> Result<PromptConfig> {
    let config: toml::Value = parse_colonyrc(colonyrc)?;
    
    // Check for include
    if let Some(include_path) = config.get("prompt").and_then(|p| p.get("include")) {
        let external = load_external_prompt_config(include_path)?;
        return merge_configs(config, external);
    }
    
    config.get("prompt").try_into()
}
```

### Integration with REPL

```rust
// colony-shell/src/repl.rs (modified)

pub async fn run_repl(agent: Option<Agent>, executor: Executor) -> Result<()> {
    // Load prompt config
    let prompt_config = load_prompt_config(&find_colonyrc()?)?;
    
    // Build component registry
    let mut registry = ComponentRegistry::new();
    registry.register_builtins();  // pwd, git_branch, etc.
    registry.register_user_components(&prompt_config)?;
    
    // Set up subscription manager
    let mut sub_manager = SubscriptionManager::new();
    for component in registry.all() {
        sub_manager.register(component);
    }
    
    // Wire tuple space events
    wire_subscriptions(&executor.tuple_space(), &sub_manager);
    
    // Detect render mode
    let render_mode = detect_render_mode();
    
    loop {
        // Build context
        let ctx = ComponentContext::from_executor(&executor, &agent);
        
        // Render components
        let left = render_components(&prompt_config.left, &registry, &mut sub_manager, &ctx);
        let right = render_components(&prompt_config.right, &registry, &mut sub_manager, &ctx);
        
        // Output based on render mode
        let prompt = match render_mode {
            RenderMode::Plain => format!("colony> "),
            RenderMode::Ansi => render_prompt_ansi(&left, &right, terminal_width()),
            RenderMode::ColonyTerminal => {
                emit_prompt(&left, &right);
                String::new()  // terminal renders it
            }
        };
        
        let line = rl.readline(&prompt)?;
        // ... rest of REPL loop
    }
}
```

### Default `.colonyrc` prompt section

```toml
[prompt]
left = ["pwd", "git_branch", "git_status"]
right = ["model", "jobs", "exit_status"]
gap = "  "
```

## Crate Structure

New crate `colony-ui-core`:

```
crates/colony-ui-core/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── component.rs       # PromptComponent trait, ComponentOutput, Style, StyledSpan
    ├── subscription.rs    # SubscriptionManager
    ├── config.rs          # TOML parsing for prompt config
    ├── context.rs         # ComponentContext
    ├── render/
    │   ├── mod.rs
    │   └── ansi.rs        # ANSI renderer
    └── builtins/
        ├── mod.rs
        ├── pwd.rs
        ├── git_branch.rs
        ├── git_status.rs
        ├── jobs.rs
        ├── exit_status.rs
        └── model.rs
```

## Success Criteria

1. Default prompt shows pwd, git branch, git status (with diff stats), model, jobs, exit_status
2. Components only re-render when relevant tuple space events fire
3. User can define custom shell-based components in `.colonyrc`
4. ANSI output looks good in standard terminals
5. OSC 2337 `Prompt` message emitted when in colony-terminal
6. Prompt renders in <50ms even with all components
7. No regression in shell functionality
