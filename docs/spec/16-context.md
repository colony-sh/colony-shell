# Part 16: Context Management

## 16.1 Overview

Context management provides first-class support for AI interaction context. Unlike traditional shells where context is implicit and ephemeral, Colony makes context visible, persistent, shareable, and composable.

### 16.1.1 Design Goals

1. **Visible** - Users see exactly what information is available to agents
2. **Persistent** - Context survives sessions and syncs across devices
3. **Shareable** - Teams can access and contribute to shared context
4. **Composable** - Build context from reusable pieces
5. **Curable** - Tools to refine, prune, and optimize token usage

### 16.1.2 Context vs CONTEXT.md

| Aspect | CONTEXT.md (Part 12) | Context System (Part 16) |
|--------|---------------------|--------------------------|
| What | Static file format | Runtime system |
| When | Loaded on directory change | Managed throughout session |
| Scope | Per-project seed | All context sources |
| Management | Manual file editing | `context` command |

CONTEXT.md provides initial project context; the context system manages all context dynamically.

---

## 16.2 Context Types

### 16.2.1 Project Context

Automatically gathered from environment:

```rust
pub struct ProjectContext {
    pub name: String,
    pub root: PathBuf,
    pub languages: Vec<(String, f32)>,  // (language, percentage)
    pub build_system: Option<BuildSystem>,
    pub git: Option<GitContext>,
    pub dependencies: Vec<Dependency>,
    pub patterns: Vec<DetectedPattern>,
}

pub struct GitContext {
    pub branch: String,
    pub recent_commits: u32,
    pub contributors: Vec<String>,
    pub open_prs: u32,
}
```

### 16.2.2 File Context

Explicitly added files:

```rust
pub struct FileContext {
    pub path: PathBuf,
    pub content: ContextContent,
    pub tokens: usize,
    pub added_at: DateTime<Utc>,
    pub last_used: Option<DateTime<Utc>>,
    pub pinned: bool,
    pub priority: Priority,
}

pub enum ContextContent {
    Full(String),
    Summary(String),
    Structure(String),  // Signatures only
}
```

### 16.2.3 Conversation Context

Agent interaction history:

```rust
pub struct ConversationContext {
    pub id: ConversationId,
    pub timestamp: DateTime<Utc>,
    pub query: String,
    pub response_summary: String,
    pub tokens: usize,
    pub relevance_score: f32,
}
```

### 16.2.4 Knowledge Context

User-curated facts:

```rust
pub struct KnowledgeContext {
    pub id: KnowledgeId,
    pub content: String,
    pub category: Option<String>,
    pub source: KnowledgeSource,
    pub created_at: DateTime<Utc>,
}

pub enum KnowledgeSource {
    Manual,
    ExtractedFrom(PathBuf),
    Learned,  // From reflection
}
```

### 16.2.5 Team Context

Shared organizational knowledge (Team+ tier):

```rust
pub struct TeamContext {
    pub id: TeamContextId,
    pub team_id: TeamId,
    pub content: String,
    pub category: Option<String>,
    pub permissions: Permissions,
    pub contributors: Vec<UserId>,
}
```

---

## 16.3 Context Store

### 16.3.1 Local Storage

```
~/.colony/
├── context/
│   ├── projects/
│   │   └── {project-hash}/
│   │       ├── auto.ctx           # Auto-gathered project context
│   │       ├── files.ctx          # Added file contexts
│   │       ├── knowledge.ctx      # Learned facts
│   │       └── conversations.ctx  # Chat history
│   ├── global/
│   │   ├── preferences.ctx        # AI interaction preferences
│   │   └── knowledge.ctx          # Cross-project knowledge
│   └── snapshots/
│       └── {name}.ctx             # Named snapshots
```

### 16.3.2 Context File Format

```rust
pub struct ContextFile {
    pub version: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub entries: Vec<ContextEntry>,
}

pub struct ContextEntry {
    pub id: EntryId,
    pub kind: ContextKind,
    pub content: String,
    pub metadata: EntryMetadata,
}

pub struct EntryMetadata {
    pub tokens: usize,
    pub added_at: DateTime<Utc>,
    pub last_used: Option<DateTime<Utc>>,
    pub use_count: u32,
    pub pinned: bool,
    pub priority: Priority,
}
```

### 16.3.3 Serialization

Context files use MessagePack for efficiency:

```rust
impl ContextFile {
    pub fn load(path: &Path) -> Result<Self>;
    pub fn save(&self, path: &Path) -> Result<()>;
}
```

---

## 16.4 Context Builtin

The `context` builtin manages context at runtime.

### 16.4.1 Core Commands

```
context show [--full] [--tokens]     Show current context summary
context add <source>                 Add source to context
context remove <source>              Remove source from context
context clear [--keep=pinned]        Clear context
```

### 16.4.2 Source Specifiers

| Specifier | Example | Description |
|-----------|---------|-------------|
| Path | `src/parser/mod.rs` | Single file |
| Directory | `src/parser/` | All files in directory |
| Glob | `"*.pest"` | Pattern match |
| Conversation | `conversation:abc123` | Specific chat |
| Knowledge | `knowledge:error-handling` | Named knowledge |
| Team | `@team:runbooks` | Team context |

### 16.4.3 Add Options

```
context add <source>
    --summary              Summarize instead of full content
    --relevant             AI picks relevant files
    --changed              Git-changed files only
    --imports <file>       File plus its imports
```

### 16.4.4 Curation Commands

```
context optimize [source]            Reduce tokens
    --strategy=summarize             LLM summarization
    --strategy=structure             Keep signatures only
    --strategy=recent                Prioritize recent
    --strategy=relevant              Keep referenced items

context prune [options]              Remove low-value
    --unused=<duration>              Remove if unused for duration
    --low-relevance                  Remove low-scored items
    --duplicates                     Remove redundant info
    --dry-run                        Show without removing

context pin <source>                 Never auto-remove
context unpin <source>               Allow auto-removal
context priority <source> <level>    Set priority (low|normal|high)
```

### 16.4.5 Persistence Commands

```
context snapshot <name>              Save current state
    --auto                           Auto-snapshot before changes

context restore <name>               Restore snapshot
context snapshots                    List snapshots
context diff <snap1> <snap2>         Compare snapshots
context save [name]                  Save to named context
context load <name>                  Load named context
```

### 16.4.6 Knowledge Commands

```
context learn <fact>                 Add knowledge
    --from=<file>                    Extract from file
    --category=<cat>                 Categorize

context forget <knowledge-id>        Remove knowledge
context knowledge [--category=<cat>] List knowledge
```

### 16.4.7 Sharing Commands (Pro+)

```
context share [options]              Share context
    --team                           Share with team
    --public                         List in marketplace
    --collaborative                  Allow edits
    --expires=<duration>             Set expiration
    --include=<type>                 Include specific types

context import <url|name>            Import context
context preview <url|name>           Preview before import
context bundle create <name>         Create sharable bundle
    --add=<source>                   Add to bundle
```

### 16.4.8 Team Commands (Team+)

```
context team show                    Show team contexts
context team add <source>            Add to team context
context team learn <fact>            Add team knowledge
    --category=<cat>                 Categorize
context team permissions <ctx>       Manage permissions
    --role=<role>                    Set access role
```

---

## 16.5 Context Integration

### 16.5.1 Agent Integration

Context is automatically provided to agents:

```rust
impl Agent {
    pub async fn query(&self, prompt: &str, ctx: &ContextManager) -> Result<Response> {
        let context = ctx.build_context_for_agent()?;
        let messages = self.build_messages(prompt, &context);
        self.client.send(messages).await
    }
}

impl ContextManager {
    pub fn build_context_for_agent(&self) -> Result<String> {
        let mut parts = Vec::new();

        // Project context (always included)
        if let Some(project) = &self.project_context {
            parts.push(project.render()?);
        }

        // Files (by priority, respecting token budget)
        for file in self.files_by_priority() {
            if self.would_exceed_budget(&parts, &file) {
                break;
            }
            parts.push(file.render()?);
        }

        // Knowledge (always included, typically small)
        for knowledge in &self.knowledge {
            parts.push(knowledge.render()?);
        }

        // Recent conversations (limited)
        for conv in self.recent_conversations(5) {
            parts.push(conv.render()?);
        }

        Ok(parts.join("\n\n"))
    }
}
```

### 16.5.2 Channel Integration

Context flows through `stdcontext` (fd 3):

```rust
// Writing to context channel adds to session context
echo "Important: use thiserror for errors" >&3

// Reading from context channel retrieves current context
context show  // Internally reads from @context buffer
```

### 16.5.3 Reflection Integration

The reflection engine (Part 17 - future) observes context usage:

```rust
pub struct ContextObservation {
    pub session_id: SessionId,
    pub sources_added: Vec<SourceId>,
    pub sources_used: Vec<(SourceId, u32)>,  // (source, reference_count)
    pub average_tokens: usize,
    pub optimization_opportunities: Vec<Suggestion>,
}
```

---

## 16.6 Token Management

### 16.6.1 Token Counting

```rust
pub trait TokenCounter {
    fn count_tokens(&self, text: &str) -> usize;
}

// Default implementation uses tiktoken
pub struct TiktokenCounter {
    encoding: Encoding,
}
```

### 16.6.2 Budget Management

```rust
pub struct TokenBudget {
    pub total: usize,           // Model's context window
    pub system_reserved: usize, // For system prompt
    pub response_reserved: usize, // For response
    pub available: usize,       // For user context
}

impl ContextManager {
    pub fn available_tokens(&self) -> usize {
        self.budget.available - self.current_usage()
    }

    pub fn current_usage(&self) -> usize {
        self.entries.iter().map(|e| e.tokens).sum()
    }
}
```

### 16.6.3 Automatic Optimization

When approaching budget limits:

```rust
impl ContextManager {
    pub fn auto_optimize(&mut self) -> Result<()> {
        // 1. Remove unpinned, unused items
        self.prune_unused(Duration::days(7))?;

        // 2. Summarize large files
        for file in self.large_files() {
            if !file.pinned {
                file.content = ContextContent::Summary(
                    self.summarize(&file)?
                );
            }
        }

        // 3. Trim conversation history
        self.trim_conversations(10)?;

        Ok(())
    }
}
```

---

## 16.7 Visualization

### 16.7.1 Context Inspector Widget

The terminal displays context in a dedicated pane:

```rust
pub struct ContextInspector {
    pub composition: CompositionView,
    pub sources: SourceListView,
    pub actions: ActionBar,
}

pub struct CompositionView {
    pub total_tokens: usize,
    pub budget: usize,
    pub breakdown: Vec<(ContextKind, usize, f32)>,  // (kind, tokens, percentage)
}
```

### 16.7.2 OSC Protocol

Context state communicated via OSC 2337:

```
ESC ] 2337 ; Context=<json> ST
```

Where `<json>` contains:

```json
{
    "total_tokens": 12847,
    "budget": 100000,
    "composition": {
        "project": 4200,
        "files": 3100,
        "knowledge": 1200,
        "history": 2500,
        "system": 1847
    },
    "sources": [...]
}
```

---

## 16.8 Implementation Notes

### 16.8.1 Crate Structure

```
colony-context/
├── src/
│   ├── lib.rs
│   ├── types.rs         # Context types
│   ├── store.rs         # Storage implementation
│   ├── manager.rs       # Runtime management
│   ├── builtin.rs       # context command
│   ├── optimize.rs      # Optimization strategies
│   └── integration.rs   # Agent/channel integration
```

### 16.8.2 Dependencies

```toml
[dependencies]
rmp-serde = "1.0"        # MessagePack serialization
tiktoken-rs = "0.5"      # Token counting
glob = "0.3"             # File patterns
```

---

## 16.9 Tier Availability

| Feature | Free | Pro | Team | Enterprise |
|---------|------|-----|------|------------|
| Local storage | Yes | Yes | Yes | Yes |
| All context commands | Yes | Yes | Yes | Yes |
| Cloud sync | - | Yes | Yes | Yes |
| Context sharing | - | Yes | Yes | Yes |
| Team contexts | - | - | Yes | Yes |
| Marketplace | - | Yes | Yes | Yes |
| Analytics | - | - | Yes | Yes |
| Permissions | - | - | Yes | Yes |
| Retention policies | - | - | - | Yes |
| Audit logs | - | - | - | Yes |
