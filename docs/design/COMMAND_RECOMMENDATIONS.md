# Command Recommendation System Design

This document describes the architecture for Colony Shell's real-time command recommendation engine, including inline suggestions (ghost text), contextual help, and AI-powered recommendations.

## Table of Contents

1. [Overview](#overview)
2. [Architecture Decision](#architecture-decision)
3. [OSC 2337 Protocol Extensions](#osc-2337-protocol-extensions)
4. [Shell-Side Implementation](#shell-side-implementation)
5. [Terminal-Side Rendering](#terminal-side-rendering)
6. [Recommendation Sources](#recommendation-sources)
7. [Rendering Strategies](#rendering-strategies)
8. [Integration with Rustyline](#integration-with-rustyline)
9. [Configuration](#configuration)
10. [Implementation Plan](#implementation-plan)

---

## Overview

Colony Shell provides real-time command recommendations as users type. The system supports:

- **Inline ghost text** (Fish-style autosuggestions)
- **Dropdown menus** (VS Code-style completion)
- **Contextual help panels** (command previews, warnings)
- **AI-powered suggestions** (optional, async)

```
┌─────────────────────────────────────────────────────────────────────────┐
│  $ git com█mit -m "message"                                             │
│           ^^^^^^^^^^^^^^^^^ (ghost text - dim gray)                     │
│                                                                         │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │ ● commit        Record changes to repository                    │   │
│  │   config        Get and set repository options                  │   │
│  │   checkout      Switch branches or restore files                │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                                                         │
│  Press Tab to accept, ↑↓ to navigate, Esc to dismiss                    │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## Architecture Decision

### Key Question: Where does recommendation logic live?

**Answer: Split between shell and terminal**

- **Shell (colony-shell)**: Generates recommendations (has context, history, environment)
- **Terminal (colony-terminal)**: Renders recommendations (handles UI, styling, interaction)

### Why This Split?

| Component | Shell | Terminal |
|-----------|-------|----------|
| Current directory | ✅ | ❌ |
| Command history | ✅ | ❌ |
| Environment variables | ✅ | ❌ |
| Git repository state | ✅ | ❌ |
| Agent/LLM access | ✅ | ❌ |
| Ghost text rendering | ❌ | ✅ |
| Dropdown menus | ❌ | ✅ |
| Rich styling/colors | ❌ | ✅ |
| Keyboard handling | Partial | ✅ |

### Data Flow

```
User types: "git com"
     │
     ▼
┌─────────────────────────────────────────────────┐
│  SHELL (colony-shell)                           │
│                                                 │
│  1. Detect keystroke via rustyline callback    │
│  2. Debounce (150ms default)                   │
│  3. Parse current input (tree-sitter)          │
│  4. Generate recommendations:                  │
│     - Command completion (PATH + builtins)     │
│     - History matching                         │
│     - Context-aware suggestions                │
│     - AI-powered recommendations (optional)    │
│  5. Emit via OSC 2337:                         │
│     ESC ] 2337 ; Suggest=inline,... ST         │
│                                                 │
└─────────────────────────────────────────────────┘
     │
     ▼ OSC 2337 Suggest= message
┌─────────────────────────────────────────────────┐
│  TERMINAL (colony-terminal)                     │
│                                                 │
│  1. Receive OSC 2337 Suggest= message          │
│  2. Render as configured:                       │
│     - Ghost text (inline, dim)                 │
│     - Dropdown menu                            │
│     - Side preview panel                       │
│  3. Handle keyboard:                           │
│     - Tab: Accept suggestion                   │
│     - Esc: Dismiss                             │
│     - ↑↓: Navigate menu                        │
│  4. Send accepted text back to shell stdin     │
│                                                 │
└─────────────────────────────────────────────────┘
```

### Why Not a New Channel?

Channels (fd 3-6) are designed for **persistent data accumulation** (context, LLM history, tool logs). Recommendations are **ephemeral and event-driven** — they should not persist.

The OSC 2337 protocol is already the shell↔terminal communication mechanism, making it the natural choice.

**However**, recommendation metadata CAN be logged to `stdcontext` (fd 3) for agent awareness:

```rust
// Log that help was requested (for agent context)
context.add_simple(ContextType::HelpRequest, "git checkout: branch switching");
```

---

## OSC 2337 Protocol Extensions

### New Commands

| Command | Direction | Description |
|---------|-----------|-------------|
| `Suggest=inline,<text>` | Shell→Terminal | Single inline suggestion (ghost text) |
| `Suggest=menu,<json>` | Shell→Terminal | Dropdown menu with multiple options |
| `Suggest=help,<json>` | Shell→Terminal | Contextual help panel |
| `Suggest=preview,<json>` | Shell→Terminal | Command effect preview |
| `Suggest=clear` | Shell→Terminal | Clear all suggestions |
| `Suggest=accept` | Terminal→Shell | User accepted a suggestion |

### Suggest=inline Payload

Simple completion text to show as ghost text after cursor:

```
ESC ] 2337 ; Suggest=inline,mit -m "" ST
```

With metadata (JSON):
```
ESC ] 2337 ; Suggest=inline,{"text":"mit -m \"\"","desc":"Commit with message"} ST
```

### Suggest=menu Payload

Array of menu items:

```json
{
  "items": [
    {
      "text": "commit",
      "display": "commit",
      "description": "Record changes to repository",
      "kind": "subcommand",
      "icon": "📝"
    },
    {
      "text": "checkout",
      "display": "checkout",
      "description": "Switch branches or restore files",
      "kind": "subcommand",
      "icon": "🔀"
    }
  ],
  "selected": 0,
  "position": {"line": 0, "column": 8}
}
```

### Suggest=help Payload

Contextual help information:

```json
{
  "command": "git",
  "subcommand": "commit",
  "brief": "Record changes to the repository",
  "usage": "git commit [-m <msg>] [--amend] [<file>...]",
  "options": [
    {"short": "-m", "long": "--message", "arg": "<msg>", "desc": "Commit message"},
    {"short": "-a", "long": "--all", "desc": "Stage modified files automatically"},
    {"long": "--amend", "desc": "Amend previous commit"}
  ],
  "examples": [
    {"code": "git commit -m 'Fix bug'", "desc": "Simple commit"},
    {"code": "git commit --amend", "desc": "Fix last commit"}
  ],
  "source": "tldr"
}
```

### Suggest=preview Payload

Preview of command effect (for destructive commands):

```json
{
  "command": "rm -rf ./build",
  "warnings": [
    {"level": "warning", "message": "This will delete 342 files (15.2 MB)"}
  ],
  "preview": {
    "kind": "file_list",
    "files": [
      {"path": "build/src/main.js", "size": 1024},
      {"path": "build/dist/bundle.js", "size": 52000}
    ],
    "truncated": true,
    "total_count": 342,
    "total_size": 15900000
  },
  "actions": [
    {"key": "Enter", "label": "Execute"},
    {"key": "Tab", "label": "Show all files"},
    {"key": "Esc", "label": "Cancel"}
  ]
}
```

---

## Shell-Side Implementation

### New Crate: colony-recommend

```
crates/
  colony-recommend/
    src/
      lib.rs           # Public API
      engine.rs        # RecommendationEngine
      sources/
        mod.rs
        command.rs     # Command completion
        history.rs     # History-based
        file.rs        # File/directory completion
        git.rs         # Git-aware suggestions
        ai.rs          # AI-powered suggestions
      protocol.rs      # OSC 2337 emission
    Cargo.toml
```

### Core Types

```rust
// crates/colony-recommend/src/lib.rs

use serde::{Deserialize, Serialize};

/// Configuration for the recommendation engine
#[derive(Clone, Debug)]
pub struct RecommendConfig {
    /// Enable inline ghost text suggestions
    pub inline_enabled: bool,
    /// Enable dropdown menu suggestions
    pub menu_enabled: bool,
    /// Enable contextual help panel
    pub help_enabled: bool,
    /// Enable AI-powered suggestions (requires agent)
    pub ai_enabled: bool,
    /// Debounce delay in milliseconds
    pub debounce_ms: u64,
    /// Maximum suggestions to generate
    pub max_suggestions: usize,
    /// Timeout for AI suggestions in milliseconds
    pub ai_timeout_ms: u64,
}

impl Default for RecommendConfig {
    fn default() -> Self {
        Self {
            inline_enabled: true,
            menu_enabled: true,
            help_enabled: true,
            ai_enabled: false,  // Opt-in
            debounce_ms: 150,
            max_suggestions: 10,
            ai_timeout_ms: 500,
        }
    }
}

/// A single recommendation
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Recommendation {
    /// Type of recommendation
    pub kind: RecommendKind,
    /// Content to display/insert
    pub content: RecommendContent,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f32,
    /// Source that generated this recommendation
    pub source: RecommendSource,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum RecommendKind {
    /// Inline ghost text completion
    InlineSuggestion,
    /// Menu item option
    MenuItem,
    /// Contextual help
    Help,
    /// Command preview/warning
    Preview,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RecommendContent {
    /// Simple text insertion
    Inline {
        /// Text to insert at cursor
        text: String,
        /// Optional description
        description: Option<String>,
    },
    /// Menu item
    Menu(MenuItem),
    /// Help information
    Help(HelpInfo),
    /// Command preview
    Preview(PreviewInfo),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MenuItem {
    /// Text to insert when selected
    pub text: String,
    /// Display text (may differ from insert text)
    pub display: String,
    /// Description shown in menu
    pub description: String,
    /// Type of completion
    pub kind: CompletionKind,
    /// Optional icon
    pub icon: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum CompletionKind {
    Command,
    Builtin,
    Alias,
    Function,
    File,
    Directory,
    Variable,
    Option,
    Subcommand,
    Branch,
    History,
    Keyword,
    Agent,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HelpInfo {
    pub command: String,
    pub subcommand: Option<String>,
    pub brief: String,
    pub usage: Option<String>,
    pub options: Vec<OptionHelp>,
    pub examples: Vec<ExampleHelp>,
    pub source: HelpSource,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OptionHelp {
    pub short: Option<String>,
    pub long: Option<String>,
    pub argument: Option<String>,
    pub description: String,
    pub required: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExampleHelp {
    pub code: String,
    pub description: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum HelpSource {
    Builtin,
    ManPage,
    Tldr,
    Custom,
    Agent,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PreviewInfo {
    pub command: String,
    pub warnings: Vec<Warning>,
    pub preview: Option<PreviewContent>,
    pub actions: Vec<Action>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Warning {
    pub level: WarningLevel,
    pub message: String,
    pub details: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum WarningLevel {
    Info,
    Warning,
    Danger,
    Critical,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PreviewContent {
    FileList {
        files: Vec<FilePreview>,
        truncated: bool,
        total_count: usize,
        total_size: u64,
    },
    Diff {
        hunks: Vec<DiffHunk>,
    },
    Output {
        lines: Vec<String>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FilePreview {
    pub path: String,
    pub size: Option<u64>,
    pub kind: FileKind,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum FileKind {
    File,
    Directory,
    Symlink,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiffHunk {
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub content: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum DiffLineKind {
    Context,
    Added,
    Removed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Action {
    pub key: String,
    pub label: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum RecommendSource {
    Command,
    History,
    File,
    Git,
    Context,
    Agent,
}
```

### Recommendation Engine

```rust
// crates/colony-recommend/src/engine.rs

use crate::*;
use colony_exec::History;
use colony_expand::ExpansionContext;
use colony_agent::Agent;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Input context for generating recommendations
#[derive(Clone, Debug)]
pub struct RecommendInput {
    /// Current input line
    pub line: String,
    /// Cursor position (byte offset)
    pub cursor: usize,
    /// Word being typed (extracted from line at cursor)
    pub word: String,
    /// Word start position
    pub word_start: usize,
    /// Previous words on the line
    pub previous_words: Vec<String>,
    /// Detected command name (if any)
    pub command: Option<String>,
    /// Argument position (0 = command itself)
    pub arg_position: usize,
}

impl RecommendInput {
    pub fn from_line(line: &str, cursor: usize) -> Self {
        let line = line.to_string();
        let cursor = cursor.min(line.len());
        
        // Find word boundaries
        let before_cursor = &line[..cursor];
        let word_start = before_cursor.rfind(|c: char| c.is_whitespace())
            .map(|i| i + 1)
            .unwrap_or(0);
        let word = line[word_start..cursor].to_string();
        
        // Split previous words
        let previous_words: Vec<String> = before_cursor[..word_start]
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();
        
        // Detect command
        let command = if previous_words.is_empty() && !word.is_empty() {
            None  // Still typing command
        } else {
            previous_words.first().cloned()
        };
        
        let arg_position = previous_words.len();
        
        Self {
            line,
            cursor,
            word,
            word_start,
            previous_words,
            command,
            arg_position,
        }
    }
}

/// Main recommendation engine
pub struct RecommendationEngine {
    config: RecommendConfig,
    history: Arc<RwLock<History>>,
    context: Arc<RwLock<ExpansionContext>>,
    agent: Option<Arc<RwLock<Agent>>>,
    
    // Caches
    command_cache: Arc<RwLock<Vec<String>>>,
    last_cache_update: Arc<RwLock<Instant>>,
}

impl RecommendationEngine {
    pub fn new(
        config: RecommendConfig,
        history: Arc<RwLock<History>>,
        context: Arc<RwLock<ExpansionContext>>,
        agent: Option<Arc<RwLock<Agent>>>,
    ) -> Self {
        Self {
            config,
            history,
            context,
            agent,
            command_cache: Arc::new(RwLock::new(Vec::new())),
            last_cache_update: Arc::new(RwLock::new(Instant::now() - Duration::from_secs(3600))),
        }
    }
    
    /// Generate recommendations for the current input
    pub async fn recommend(&self, input: &RecommendInput) -> Vec<Recommendation> {
        let mut recommendations = Vec::new();
        
        // Refresh command cache if needed
        self.refresh_command_cache_if_needed().await;
        
        // 1. Command completion (when typing command name)
        if input.arg_position == 0 {
            recommendations.extend(self.command_completions(input).await);
        }
        
        // 2. History-based suggestions
        recommendations.extend(self.history_suggestions(input).await);
        
        // 3. Context-aware completions (files, options, git, etc.)
        recommendations.extend(self.context_completions(input).await);
        
        // 4. AI-powered suggestions (if enabled)
        if self.config.ai_enabled {
            if let Some(ai_recs) = self.ai_suggestions(input).await {
                recommendations.extend(ai_recs);
            }
        }
        
        // Sort by confidence and deduplicate
        recommendations.sort_by(|a, b| {
            b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal)
        });
        
        // Remove duplicates (by text content)
        let mut seen = std::collections::HashSet::new();
        recommendations.retain(|r| {
            let key = match &r.content {
                RecommendContent::Inline { text, .. } => text.clone(),
                RecommendContent::Menu(item) => item.text.clone(),
                _ => return true,
            };
            seen.insert(key)
        });
        
        // Truncate to max
        recommendations.truncate(self.config.max_suggestions);
        
        recommendations
    }
    
    /// Generate inline suggestion (single best match for ghost text)
    pub async fn inline_suggestion(&self, input: &RecommendInput) -> Option<Recommendation> {
        if !self.config.inline_enabled || input.line.is_empty() {
            return None;
        }
        
        // Best match from history (most relevant for inline)
        let history = self.history.read().await;
        let history_match = history.iter()
            .rev()  // Most recent first
            .find(|h| h.starts_with(&input.line) && h.len() > input.line.len());
        
        if let Some(hist) = history_match {
            let suffix = &hist[input.line.len()..];
            return Some(Recommendation {
                kind: RecommendKind::InlineSuggestion,
                content: RecommendContent::Inline {
                    text: suffix.to_string(),
                    description: Some("from history".to_string()),
                },
                confidence: 0.9,
                source: RecommendSource::History,
            });
        }
        
        // Fallback to command completion
        if input.arg_position == 0 && !input.word.is_empty() {
            let cache = self.command_cache.read().await;
            if let Some(cmd) = cache.iter().find(|c| c.starts_with(&input.word)) {
                let suffix = &cmd[input.word.len()..];
                return Some(Recommendation {
                    kind: RecommendKind::InlineSuggestion,
                    content: RecommendContent::Inline {
                        text: suffix.to_string(),
                        description: None,
                    },
                    confidence: 0.7,
                    source: RecommendSource::Command,
                });
            }
        }
        
        None
    }
    
    /// Get contextual help for current input
    pub async fn contextual_help(&self, input: &RecommendInput) -> Option<Recommendation> {
        if !self.config.help_enabled {
            return None;
        }
        
        let command = input.command.as_ref()?;
        
        // Try TLDR first (concise)
        if let Some(help) = self.lookup_tldr(command).await {
            return Some(Recommendation {
                kind: RecommendKind::Help,
                content: RecommendContent::Help(help),
                confidence: 1.0,
                source: RecommendSource::Context,
            });
        }
        
        // Fallback to man page brief
        if let Some(help) = self.lookup_man_brief(command).await {
            return Some(Recommendation {
                kind: RecommendKind::Help,
                content: RecommendContent::Help(help),
                confidence: 0.8,
                source: RecommendSource::Context,
            });
        }
        
        None
    }
    
    /// Generate command preview (for destructive commands)
    pub async fn command_preview(&self, input: &RecommendInput) -> Option<Recommendation> {
        let command = input.command.as_ref()?;
        
        // Only preview destructive commands
        let destructive_commands = ["rm", "mv", "chmod", "chown", "dd"];
        if !destructive_commands.contains(&command.as_str()) {
            return None;
        }
        
        // Parse and analyze command
        // TODO: Implement actual file counting, size calculation
        
        None
    }
    
    // --- Private helper methods ---
    
    async fn refresh_command_cache_if_needed(&self) {
        let last_update = *self.last_cache_update.read().await;
        if last_update.elapsed() < Duration::from_secs(60) {
            return;  // Cache is fresh
        }
        
        let mut cache = self.command_cache.write().await;
        let mut last = self.last_cache_update.write().await;
        
        // Scan PATH
        let mut commands = Vec::new();
        if let Ok(path) = std::env::var("PATH") {
            for dir in path.split(':') {
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for entry in entries.filter_map(|e| e.ok()) {
                        if let Ok(ft) = entry.file_type() {
                            if ft.is_file() || ft.is_symlink() {
                                commands.push(entry.file_name().to_string_lossy().to_string());
                            }
                        }
                    }
                }
            }
        }
        
        // Add builtins
        let builtins = [
            "cd", "echo", "exit", "export", "alias", "source", "var", "const",
            "fn", "proc", "agent", "context", "llm", "mcp", "pane", "buffer",
            "pwd", "read", "true", "false", "test", "[", "printf", "type",
            "hash", "unset", "set", "shift", "return", "break", "continue",
        ];
        commands.extend(builtins.iter().map(|s| s.to_string()));
        
        commands.sort();
        commands.dedup();
        
        *cache = commands;
        *last = Instant::now();
    }
    
    async fn command_completions(&self, input: &RecommendInput) -> Vec<Recommendation> {
        let cache = self.command_cache.read().await;
        let prefix = &input.word;
        
        cache.iter()
            .filter(|cmd| cmd.starts_with(prefix))
            .take(self.config.max_suggestions)
            .map(|cmd| {
                let suffix = &cmd[prefix.len()..];
                Recommendation {
                    kind: RecommendKind::MenuItem,
                    content: RecommendContent::Menu(MenuItem {
                        text: suffix.to_string(),
                        display: cmd.clone(),
                        description: self.command_description(cmd),
                        kind: self.command_kind(cmd),
                        icon: None,
                    }),
                    confidence: 0.8,
                    source: RecommendSource::Command,
                }
            })
            .collect()
    }
    
    async fn history_suggestions(&self, input: &RecommendInput) -> Vec<Recommendation> {
        let history = self.history.read().await;
        let line = &input.line;
        
        history.iter()
            .rev()
            .filter(|h| h.starts_with(line) && h != line)
            .take(5)
            .map(|h| {
                let suffix = &h[line.len()..];
                Recommendation {
                    kind: RecommendKind::MenuItem,
                    content: RecommendContent::Menu(MenuItem {
                        text: suffix.to_string(),
                        display: h.clone(),
                        description: "from history".to_string(),
                        kind: CompletionKind::History,
                        icon: Some("⏱".to_string()),
                    }),
                    confidence: 0.85,
                    source: RecommendSource::History,
                }
            })
            .collect()
    }
    
    async fn context_completions(&self, input: &RecommendInput) -> Vec<Recommendation> {
        let mut recs = Vec::new();
        
        // File completions
        if input.arg_position > 0 {
            recs.extend(self.file_completions(input).await);
        }
        
        // Git completions
        if input.command.as_deref() == Some("git") {
            recs.extend(self.git_completions(input).await);
        }
        
        // Option completions (if word starts with -)
        if input.word.starts_with('-') {
            recs.extend(self.option_completions(input).await);
        }
        
        recs
    }
    
    async fn file_completions(&self, input: &RecommendInput) -> Vec<Recommendation> {
        let word = &input.word;
        let (dir, prefix) = if let Some(pos) = word.rfind('/') {
            (&word[..=pos], &word[pos+1..])
        } else {
            (".", word.as_str())
        };
        
        let dir_path = std::path::PathBuf::from(shellexpand::tilde(dir).as_ref());
        let mut recs = Vec::new();
        
        if let Ok(entries) = std::fs::read_dir(&dir_path) {
            for entry in entries.filter_map(|e| e.ok()).take(20) {
                let name = entry.file_name().to_string_lossy().to_string();
                
                // Skip hidden unless prefix starts with .
                if name.starts_with('.') && !prefix.starts_with('.') {
                    continue;
                }
                
                if name.to_lowercase().starts_with(&prefix.to_lowercase()) {
                    let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    let display = if is_dir { format!("{}/", name) } else { name.clone() };
                    let text = if dir == "." {
                        display.clone()
                    } else {
                        format!("{}{}", dir, display)
                    };
                    let suffix = &text[word.len()..];
                    
                    recs.push(Recommendation {
                        kind: RecommendKind::MenuItem,
                        content: RecommendContent::Menu(MenuItem {
                            text: suffix.to_string(),
                            display,
                            description: String::new(),
                            kind: if is_dir { CompletionKind::Directory } else { CompletionKind::File },
                            icon: Some(if is_dir { "📁".to_string() } else { "📄".to_string() }),
                        }),
                        confidence: 0.6,
                        source: RecommendSource::File,
                    });
                }
            }
        }
        
        recs
    }
    
    async fn git_completions(&self, input: &RecommendInput) -> Vec<Recommendation> {
        let mut recs = Vec::new();
        
        // Git subcommands
        if input.arg_position == 1 {
            let subcommands = [
                ("add", "Add file contents to index"),
                ("branch", "List, create, or delete branches"),
                ("checkout", "Switch branches or restore files"),
                ("clone", "Clone a repository"),
                ("commit", "Record changes to repository"),
                ("diff", "Show changes between commits"),
                ("fetch", "Download objects from remote"),
                ("init", "Create empty Git repository"),
                ("log", "Show commit logs"),
                ("merge", "Join development histories"),
                ("pull", "Fetch and integrate with remote"),
                ("push", "Update remote refs"),
                ("rebase", "Reapply commits on top of another base"),
                ("reset", "Reset current HEAD to specified state"),
                ("stash", "Stash changes in dirty working directory"),
                ("status", "Show working tree status"),
            ];
            
            for (cmd, desc) in subcommands {
                if cmd.starts_with(&input.word) {
                    let suffix = &cmd[input.word.len()..];
                    recs.push(Recommendation {
                        kind: RecommendKind::MenuItem,
                        content: RecommendContent::Menu(MenuItem {
                            text: suffix.to_string(),
                            display: cmd.to_string(),
                            description: desc.to_string(),
                            kind: CompletionKind::Subcommand,
                            icon: None,
                        }),
                        confidence: 0.9,
                        source: RecommendSource::Git,
                    });
                }
            }
        }
        
        // Branch names for checkout/branch/merge
        let branch_commands = ["checkout", "branch", "merge", "rebase"];
        if input.arg_position >= 2 {
            let subcommand = input.previous_words.get(1).map(|s| s.as_str());
            if branch_commands.contains(&subcommand.unwrap_or("")) {
                recs.extend(self.git_branch_completions(input).await);
            }
        }
        
        recs
    }
    
    async fn git_branch_completions(&self, input: &RecommendInput) -> Vec<Recommendation> {
        // Execute git branch to get branch names
        let output = tokio::process::Command::new("git")
            .args(["branch", "--format=%(refname:short)"])
            .output()
            .await
            .ok();
        
        let mut recs = Vec::new();
        
        if let Some(output) = output {
            let branches = String::from_utf8_lossy(&output.stdout);
            for branch in branches.lines() {
                if branch.starts_with(&input.word) {
                    let suffix = &branch[input.word.len()..];
                    recs.push(Recommendation {
                        kind: RecommendKind::MenuItem,
                        content: RecommendContent::Menu(MenuItem {
                            text: suffix.to_string(),
                            display: branch.to_string(),
                            description: "branch".to_string(),
                            kind: CompletionKind::Branch,
                            icon: Some("🔀".to_string()),
                        }),
                        confidence: 0.85,
                        source: RecommendSource::Git,
                    });
                }
            }
        }
        
        recs
    }
    
    async fn option_completions(&self, input: &RecommendInput) -> Vec<Recommendation> {
        // TODO: Parse man pages or maintain option database
        Vec::new()
    }
    
    async fn ai_suggestions(&self, input: &RecommendInput) -> Option<Vec<Recommendation>> {
        let agent = self.agent.as_ref()?;
        
        // Don't query for very short inputs
        if input.line.len() < 3 {
            return None;
        }
        
        // TODO: Implement async AI query with timeout
        // let response = tokio::time::timeout(
        //     Duration::from_millis(self.config.ai_timeout_ms),
        //     agent.read().await.suggest(&input.line)
        // ).await.ok()??;
        
        None
    }
    
    async fn lookup_tldr(&self, command: &str) -> Option<HelpInfo> {
        let cache_dir = dirs::cache_dir()?.join("colony/tldr");
        let path = cache_dir.join(format!("{}.md", command));
        
        if !path.exists() {
            return None;
        }
        
        let content = std::fs::read_to_string(&path).ok()?;
        self.parse_tldr(&content, command)
    }
    
    fn parse_tldr(&self, content: &str, name: &str) -> Option<HelpInfo> {
        let mut brief = String::new();
        let mut examples = Vec::new();
        let mut current_desc = String::new();
        
        for line in content.lines() {
            if line.starts_with('>') {
                brief = line[1..].trim().to_string();
            } else if line.starts_with('-') {
                current_desc = line[1..].trim().to_string();
            } else if line.starts_with('`') && line.ends_with('`') {
                examples.push(ExampleHelp {
                    code: line[1..line.len()-1].to_string(),
                    description: current_desc.clone(),
                });
            }
        }
        
        Some(HelpInfo {
            command: name.to_string(),
            subcommand: None,
            brief,
            usage: None,
            options: Vec::new(),
            examples,
            source: HelpSource::Tldr,
        })
    }
    
    async fn lookup_man_brief(&self, command: &str) -> Option<HelpInfo> {
        let output = tokio::process::Command::new("man")
            .args(["-f", command])
            .output()
            .await
            .ok()?;
        
        let brief = String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()?
            .to_string();
        
        Some(HelpInfo {
            command: command.to_string(),
            subcommand: None,
            brief,
            usage: None,
            options: Vec::new(),
            examples: Vec::new(),
            source: HelpSource::ManPage,
        })
    }
    
    fn command_description(&self, cmd: &str) -> String {
        // Brief descriptions for common commands
        match cmd {
            "cd" => "Change directory".to_string(),
            "ls" => "List directory contents".to_string(),
            "cat" => "Concatenate and print files".to_string(),
            "grep" => "Search for patterns".to_string(),
            "git" => "Version control system".to_string(),
            "echo" => "Display text".to_string(),
            "pwd" => "Print working directory".to_string(),
            "rm" => "Remove files".to_string(),
            "cp" => "Copy files".to_string(),
            "mv" => "Move/rename files".to_string(),
            "mkdir" => "Create directories".to_string(),
            _ => String::new(),
        }
    }
    
    fn command_kind(&self, cmd: &str) -> CompletionKind {
        let builtins = [
            "cd", "echo", "exit", "export", "alias", "source", "var", "const",
            "fn", "proc", "agent", "context", "llm", "mcp", "pane", "buffer",
            "pwd", "read", "true", "false", "test", "[", "printf", "type",
            "hash", "unset", "set", "shift", "return", "break", "continue",
        ];
        
        if builtins.contains(&cmd) {
            CompletionKind::Builtin
        } else {
            CompletionKind::Command
        }
    }
}
```

### Protocol Emission

```rust
// crates/colony-recommend/src/protocol.rs

use crate::*;
use std::io::Write;

/// Emit recommendations via OSC 2337
pub struct RecommendEmitter;

impl RecommendEmitter {
    /// Emit inline suggestion
    pub fn emit_inline<W: Write>(
        writer: &mut W,
        text: &str,
        description: Option<&str>,
    ) -> std::io::Result<()> {
        if let Some(desc) = description {
            let payload = serde_json::json!({
                "text": text,
                "desc": desc
            });
            write!(writer, "\x1b]2337;Suggest=inline,{}\x07", payload)
        } else {
            write!(writer, "\x1b]2337;Suggest=inline,{}\x07", text)
        }
    }
    
    /// Emit menu suggestions
    pub fn emit_menu<W: Write>(
        writer: &mut W,
        items: &[MenuItem],
        selected: usize,
    ) -> std::io::Result<()> {
        let payload = serde_json::json!({
            "items": items,
            "selected": selected
        });
        write!(writer, "\x1b]2337;Suggest=menu,{}\x07", payload)
    }
    
    /// Emit contextual help
    pub fn emit_help<W: Write>(
        writer: &mut W,
        help: &HelpInfo,
    ) -> std::io::Result<()> {
        let payload = serde_json::to_string(help)?;
        write!(writer, "\x1b]2337;Suggest=help,{}\x07", payload)
    }
    
    /// Emit command preview
    pub fn emit_preview<W: Write>(
        writer: &mut W,
        preview: &PreviewInfo,
    ) -> std::io::Result<()> {
        let payload = serde_json::to_string(preview)?;
        write!(writer, "\x1b]2337;Suggest=preview,{}\x07", payload)
    }
    
    /// Clear all suggestions
    pub fn emit_clear<W: Write>(writer: &mut W) -> std::io::Result<()> {
        write!(writer, "\x1b]2337;Suggest=clear\x07")
    }
}
```

---

## Terminal-Side Rendering

### Rendering Strategies

#### A. Inline Ghost Text (Fish-style)

The most common and least intrusive suggestion display:

```
$ git com█mit -m "message"
        ^^^^^^^^^^^^^^^^^ (dim gray, not selectable)
        
Press Tab or → to accept
Press any other key to dismiss
```

**Implementation notes:**
- Render dim text after the cursor position
- Do NOT move the actual cursor
- Clear ghost text on any keystroke (except Tab/→)
- Tab/→ inserts the ghost text and moves cursor

#### B. Dropdown Menu (VS Code-style)

For multiple suggestions:

```
$ git co█
┌─────────────────────────────────────────┐
│ ● commit      Record changes            │
│   checkout    Switch branches           │
│   config      Get/set options           │
│   clone       Clone repository          │
└─────────────────────────────────────────┘
```

**Implementation notes:**
- Floating widget positioned below cursor
- ● indicates selected item
- ↑↓ to navigate, Enter to accept, Esc to dismiss
- Typing filters the list
- Shows brief description for each option

#### C. Side/Bottom Help Panel

For contextual help and previews:

```
$ rm -rf ./build█                  │ ⚠ Warning
                                   │ 
                                   │ This will delete:
                                   │   342 files (15.2 MB)
                                   │   
                                   │ build/
                                   │ ├── src/
                                   │ ├── dist/
                                   │ └── test/
                                   │
                                   │ [Enter] Execute
                                   │ [Tab] Show all
                                   │ [Esc] Cancel
```

**Implementation notes:**
- Right-side panel (or bottom split)
- Shows real-time preview as command is typed
- Warnings for destructive operations
- Action hints at bottom

### Terminal State Machine

```rust
// In colony-terminal

enum SuggestionState {
    None,
    Inline {
        text: String,
        description: Option<String>,
    },
    Menu {
        items: Vec<MenuItem>,
        selected: usize,
        filter: String,
    },
    Help {
        info: HelpInfo,
    },
    Preview {
        info: PreviewInfo,
    },
}

impl Terminal {
    fn handle_osc_2337(&mut self, command: &str, payload: &str) {
        match command {
            "Suggest=inline" => {
                self.suggestion_state = SuggestionState::Inline {
                    text: payload.to_string(),
                    description: None,
                };
                self.needs_redraw = true;
            }
            "Suggest=menu" => {
                let menu: MenuPayload = serde_json::from_str(payload).unwrap();
                self.suggestion_state = SuggestionState::Menu {
                    items: menu.items,
                    selected: menu.selected,
                    filter: String::new(),
                };
                self.needs_redraw = true;
            }
            "Suggest=help" => {
                let help: HelpInfo = serde_json::from_str(payload).unwrap();
                self.suggestion_state = SuggestionState::Help { info: help };
                self.needs_redraw = true;
            }
            "Suggest=preview" => {
                let preview: PreviewInfo = serde_json::from_str(payload).unwrap();
                self.suggestion_state = SuggestionState::Preview { info: preview };
                self.needs_redraw = true;
            }
            "Suggest=clear" => {
                self.suggestion_state = SuggestionState::None;
                self.needs_redraw = true;
            }
            _ => {}
        }
    }
    
    fn handle_key(&mut self, key: KeyEvent) -> Action {
        match (&self.suggestion_state, key) {
            // Accept inline suggestion
            (SuggestionState::Inline { text, .. }, KeyEvent::Tab | KeyEvent::Right) => {
                self.insert_text(text);
                self.suggestion_state = SuggestionState::None;
                Action::Handled
            }
            
            // Navigate menu
            (SuggestionState::Menu { items, selected, .. }, KeyEvent::Down) => {
                let new_selected = (*selected + 1).min(items.len() - 1);
                // Update selected...
                Action::Handled
            }
            (SuggestionState::Menu { items, selected, .. }, KeyEvent::Up) => {
                let new_selected = selected.saturating_sub(1);
                // Update selected...
                Action::Handled
            }
            (SuggestionState::Menu { items, selected, .. }, KeyEvent::Enter) => {
                self.insert_text(&items[*selected].text);
                self.suggestion_state = SuggestionState::None;
                Action::Handled
            }
            
            // Dismiss on Esc
            (_, KeyEvent::Escape) => {
                self.suggestion_state = SuggestionState::None;
                Action::Handled
            }
            
            // Any other key clears inline suggestion
            (SuggestionState::Inline { .. }, _) => {
                self.suggestion_state = SuggestionState::None;
                Action::PassThrough
            }
            
            _ => Action::PassThrough
        }
    }
}
```

---

## Integration with Rustyline

For non-Colony terminals, use rustyline's built-in completion features:

### Hinter (Inline Suggestions)

```rust
// crates/colony-shell/src/main.rs

use rustyline::hint::{Hint, Hinter};

struct ColonyHinter {
    history: Arc<RwLock<History>>,
    recommend_engine: Arc<RecommendationEngine>,
}

impl Hinter for ColonyHinter {
    type Hint = ColonyHint;
    
    fn hint(&self, line: &str, pos: usize, ctx: &rustyline::Context<'_>) -> Option<Self::Hint> {
        // Only hint at end of line
        if pos != line.len() || line.is_empty() {
            return None;
        }
        
        // Quick synchronous check from history
        let history = self.history.blocking_read();
        let hint_text = history.iter()
            .rev()
            .find(|h| h.starts_with(line) && h.len() > line.len())
            .map(|h| h[line.len()..].to_string())?;
        
        Some(ColonyHint {
            text: hint_text,
            complete_up_to: line.len(),
        })
    }
}

struct ColonyHint {
    text: String,
    complete_up_to: usize,
}

impl Hint for ColonyHint {
    fn display(&self) -> &str {
        &self.text
    }
    
    fn completion(&self) -> Option<&str> {
        Some(&self.text)
    }
}
```

### Completer (Tab Completion)

```rust
use rustyline::completion::{Completer, Pair};

impl Completer for ColonyHelper {
    type Candidate = Pair;
    
    fn complete(
        &self,
        line: &str,
        pos: usize,
        ctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Self::Candidate>)> {
        let input = RecommendInput::from_line(line, pos);
        
        // Use blocking runtime for synchronous context
        let rt = tokio::runtime::Handle::current();
        let recommendations = rt.block_on(async {
            self.recommend_engine.recommend(&input).await
        });
        
        let candidates: Vec<Pair> = recommendations.iter()
            .filter_map(|r| match &r.content {
                RecommendContent::Inline { text, description } |
                RecommendContent::Menu(MenuItem { text, description, .. }) => {
                    Some(Pair {
                        display: format!("{} ({})", text, description.as_deref().unwrap_or("")),
                        replacement: text.clone(),
                    })
                }
                _ => None,
            })
            .collect();
        
        Ok((input.word_start, candidates))
    }
}
```

---

## Recommendation Sources

### Source Priority and Timing

| Source | Priority | Timing | When Used |
|--------|----------|--------|-----------|
| History | High | <5ms | Always (prefix match) |
| Commands | High | <10ms | Command position (arg 0) |
| Files | Medium | <20ms | Argument positions |
| Git | Medium | <50ms | git subcommands |
| Options | Medium | <10ms | After `-` |
| Man/TLDR | Low | <100ms | Help requests |
| AI | Low | <500ms | If enabled, async |

### Source: History

```rust
impl HistorySource {
    fn suggest(&self, input: &RecommendInput) -> Vec<Recommendation> {
        // 1. Exact prefix match (highest confidence)
        // 2. Fuzzy match (lower confidence)
        // 3. Weight by recency and frequency
    }
}
```

### Source: Commands (PATH)

```rust
impl CommandSource {
    fn suggest(&self, input: &RecommendInput) -> Vec<Recommendation> {
        // 1. Scan PATH directories (cached)
        // 2. Include builtins, aliases, functions
        // 3. Prefix match on command name
    }
}
```

### Source: Files

```rust
impl FileSource {
    fn suggest(&self, input: &RecommendInput) -> Vec<Recommendation> {
        // 1. Parse path prefix from word
        // 2. List directory contents
        // 3. Filter by prefix
        // 4. Mark directories vs files
    }
}
```

### Source: Git

```rust
impl GitSource {
    fn suggest(&self, input: &RecommendInput) -> Vec<Recommendation> {
        // 1. Detect git repo
        // 2. Subcommand completion
        // 3. Branch name completion
        // 4. File completion (for add, diff, etc.)
        // 5. Remote name completion
    }
}
```

### Source: AI (Optional)

```rust
impl AiSource {
    async fn suggest(&self, input: &RecommendInput) -> Vec<Recommendation> {
        // 1. Build prompt with context
        // 2. Query agent with timeout
        // 3. Parse structured response
        // 4. Return suggestions with explanations
    }
}
```

---

## Configuration

### Shell Configuration (.colonyrc)

```toml
[recommendations]
# Enable/disable recommendation system
enabled = true

# Display modes
inline = true           # Ghost text suggestions
menu = true             # Dropdown menu
help_panel = true       # Contextual help

# AI-powered suggestions (opt-in, requires agent)
ai = false
ai_timeout_ms = 500

# Timing
debounce_ms = 150       # Wait before generating
max_suggestions = 10

[recommendations.sources]
history = true
commands = true
files = true
git = true
options = true
man_pages = true
tldr = true

[recommendations.appearance]
# Ghost text style (terminal renders this)
ghost_style = "dim"     # dim, italic, color
ghost_color = "gray"

# Menu appearance
menu_max_items = 8
menu_show_icons = true
menu_show_descriptions = true
```

### Keybindings

```toml
[keybindings]
# Accept suggestions
"Tab" = "accept_suggestion"
"Right" = "accept_suggestion"
"Ctrl+f" = "accept_suggestion"

# Show/hide
"Ctrl+Space" = "show_menu"
"Ctrl+/" = "show_help"
"Escape" = "dismiss_suggestions"

# Navigate menu
"Up" = "menu_up"
"Down" = "menu_down"
"Ctrl+p" = "menu_up"
"Ctrl+n" = "menu_down"

# Accept menu item
"Enter" = "menu_accept"
"Tab" = "menu_accept"
```

---

## Implementation Plan

### Phase 1: Core Infrastructure
- [ ] Create `colony-recommend` crate
- [ ] Implement `RecommendationEngine` struct
- [ ] Implement `RecommendInput` parser
- [ ] Add basic command completion source
- [ ] Add history completion source
- [ ] Define OSC 2337 `Suggest=` protocol

### Phase 2: Shell Integration
- [ ] Integrate with rustyline `Hinter` trait
- [ ] Integrate with rustyline `Completer` trait
- [ ] Add debouncing logic
- [ ] Emit OSC 2337 messages for Colony terminal
- [ ] Add configuration parsing

### Phase 3: Terminal Rendering (colony-terminal)
- [ ] Parse OSC 2337 `Suggest=` messages
- [ ] Implement inline ghost text rendering
- [ ] Implement dropdown menu widget
- [ ] Add keyboard handling for suggestions
- [ ] Implement help panel rendering

### Phase 4: Advanced Sources
- [ ] File completion source
- [ ] Git-aware completion source
- [ ] Option completion (parse man pages)
- [ ] TLDR integration

### Phase 5: AI Integration
- [ ] Async AI suggestion source
- [ ] Timeout and caching
- [ ] Rate limiting
- [ ] User opt-in/consent

### Phase 6: Polish
- [ ] Performance optimization
- [ ] Fuzzy matching
- [ ] Learning from user behavior
- [ ] Custom completion definitions

---

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_recommend_input_parsing() {
        let input = RecommendInput::from_line("git commit -m ", 14);
        assert_eq!(input.command, Some("git".to_string()));
        assert_eq!(input.arg_position, 3);
        assert_eq!(input.word, "");
    }
    
    #[tokio::test]
    async fn test_command_completion() {
        let engine = create_test_engine();
        let input = RecommendInput::from_line("gi", 2);
        let recs = engine.recommend(&input).await;
        
        assert!(recs.iter().any(|r| {
            matches!(&r.content, RecommendContent::Menu(m) if m.display == "git")
        }));
    }
    
    #[tokio::test]
    async fn test_history_suggestion() {
        let mut history = History::new();
        history.add("git commit -m 'test'");
        
        let engine = create_test_engine_with_history(history);
        let input = RecommendInput::from_line("git com", 7);
        let inline = engine.inline_suggestion(&input).await;
        
        assert!(inline.is_some());
        assert!(matches!(
            inline.unwrap().content,
            RecommendContent::Inline { text, .. } if text.starts_with("mit")
        ));
    }
    
    #[tokio::test]
    async fn test_file_completion() {
        // Create temp directory with files
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.txt"), "").unwrap();
        std::env::set_current_dir(&dir).unwrap();
        
        let engine = create_test_engine();
        let input = RecommendInput::from_line("cat te", 6);
        let recs = engine.recommend(&input).await;
        
        assert!(recs.iter().any(|r| {
            matches!(&r.content, RecommendContent::Menu(m) if m.text == "st.txt")
        }));
    }
    
    #[tokio::test]
    async fn test_git_branch_completion() {
        // Must be in a git repo for this test
        if !std::path::Path::new(".git").exists() {
            return;
        }
        
        let engine = create_test_engine();
        let input = RecommendInput::from_line("git checkout ma", 15);
        let recs = engine.recommend(&input).await;
        
        // Should suggest "main" if that branch exists
        assert!(recs.iter().any(|r| {
            matches!(&r.content, RecommendContent::Menu(m) if m.display == "main")
        }) || recs.is_empty()); // Empty is OK if no main branch
    }
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_osc_emission() {
    let mut output = Vec::new();
    RecommendEmitter::emit_inline(&mut output, "test", Some("description")).unwrap();
    
    let emitted = String::from_utf8(output).unwrap();
    assert!(emitted.starts_with("\x1b]2337;Suggest=inline,"));
    assert!(emitted.ends_with("\x07"));
}

#[tokio::test]
async fn test_full_flow() {
    let engine = create_test_engine();
    let input = RecommendInput::from_line("echo ", 5);
    
    // Generate recommendations
    let recs = engine.recommend(&input).await;
    
    // Should have file completions
    assert!(!recs.is_empty());
    
    // Emit to buffer
    let mut output = Vec::new();
    if let Some(inline) = engine.inline_suggestion(&input).await {
        if let RecommendContent::Inline { text, description } = inline.content {
            RecommendEmitter::emit_inline(&mut output, &text, description.as_deref()).unwrap();
        }
    }
    
    // Verify emission
    assert!(!output.is_empty());
}
```

---

## Appendix: Complete OSC 2337 Protocol Reference

### Recommendation Commands

| Command | Payload | Description |
|---------|---------|-------------|
| `Suggest=inline,<text>` | Plain text or JSON | Single inline suggestion |
| `Suggest=menu,<json>` | `MenuPayload` | Dropdown menu |
| `Suggest=help,<json>` | `HelpInfo` | Contextual help |
| `Suggest=preview,<json>` | `PreviewInfo` | Command preview |
| `Suggest=clear` | None | Clear all suggestions |

### Payload Schemas

#### MenuPayload
```json
{
  "items": [
    {
      "text": "string (insert text)",
      "display": "string (display text)",
      "description": "string",
      "kind": "Command|File|Directory|...",
      "icon": "string (optional)"
    }
  ],
  "selected": 0
}
```

#### HelpInfo
```json
{
  "command": "string",
  "subcommand": "string (optional)",
  "brief": "string",
  "usage": "string (optional)",
  "options": [
    {
      "short": "-x",
      "long": "--example",
      "argument": "<arg>",
      "description": "string",
      "required": false
    }
  ],
  "examples": [
    {
      "code": "string",
      "description": "string"
    }
  ],
  "source": "Builtin|ManPage|Tldr|Custom|Agent"
}
```

#### PreviewInfo
```json
{
  "command": "string",
  "warnings": [
    {
      "level": "Info|Warning|Danger|Critical",
      "message": "string",
      "details": "string (optional)"
    }
  ],
  "preview": {
    "kind": "FileList|Diff|Output",
    "files": [...],
    "truncated": true,
    "total_count": 100,
    "total_size": 1000000
  },
  "actions": [
    {"key": "Enter", "label": "Execute"}
  ]
}
```

---

*This document should be updated as the implementation progresses.*
