# Colony Context Management

> Store, visualize, curate, and share the context that makes AI useful.

## The Problem

Every AI interaction starts with context. Today, developers:

- Re-explain their project every conversation
- Copy-paste the same files repeatedly
- Lose valuable context when sessions end
- Can't share working context with teammates
- Have no visibility into what the AI "knows"

Context is invisible, ephemeral, and unmanaged. Colony fixes this.

---

## Context as First-Class Citizen

In Colony, context is:

- **Visible** — See exactly what the AI knows
- **Persistent** — Survives sessions, syncs across devices
- **Shareable** — Team members can access shared context
- **Composable** — Build context from reusable pieces
- **Curable** — Tools to refine, prune, and optimize

---

## Context Types

### 1. Project Context

Automatically gathered from your environment:

```
┌─────────────────────────────────────────────────────────────────┐
│  Project: colony-shell                                          │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Structure                                                      │
│  ├─ Languages: Rust (82%), Pest (10%), JS (8%)                 │
│  ├─ Framework: None detected                                    │
│  ├─ Build: Cargo                                                │
│  ├─ Test: cargo test                                            │
│  └─ Key files: Cargo.toml, src/main.rs, grammars/              │
│                                                                 │
│  Git Context                                                    │
│  ├─ Branch: feature/context-management                         │
│  ├─ Recent commits: 12 in last 7 days                          │
│  ├─ Contributors: jesse, claude                                │
│  └─ Open PRs: 3                                                 │
│                                                                 │
│  Dependencies                                                   │
│  ├─ pest = "2.7"                                                │
│  ├─ tree-sitter = "0.20"                                       │
│  ├─ gpui = "0.1"                                                │
│  └─ ... 47 more                                                 │
│                                                                 │
│  Patterns Detected                                              │
│  ├─ Parser: hybrid tree-sitter + pest                          │
│  ├─ Error handling: thiserror + anyhow                         │
│  └─ Async: tokio                                                │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 2. File Context

Specific files added to context:

```colony
# Add files to context
context add src/parser/mod.rs          # Single file
context add src/parser/                # Directory
context add "*.pest"                   # Glob pattern
context add --summary README.md        # Summarized (saves tokens)

# Smart additions
context add --relevant                 # AI picks relevant files
context add --changed                  # Git-changed files only
context add --imports src/main.rs      # File + its imports
```

### 3. Conversation Context

History of agent interactions:

```colony
# Conversation becomes context
agent "explain the parser architecture"
# Response is retained in @context

# Reference past conversations
context show conversations
context add conversation:abc123        # Add specific chat
context add conversations:today        # Add today's chats
```

### 4. Knowledge Context

Curated information that persists:

```colony
# Add knowledge
context learn "We use pest for the authoritative grammar, tree-sitter for incremental parsing"
context learn "Error handling pattern: use thiserror for library errors, anyhow for application"
context learn --from=ARCHITECTURE.md   # Extract key points from doc

# Knowledge is always available to agents
agent "should I use thiserror here?"
# Agent knows your error handling conventions
```

### 5. Team Context

Shared organizational knowledge:

```colony
# Team-wide context
context team add "Production deployments require approval from #platform-team"
context team add --from=runbooks/      # Shared runbooks
context team add oncall:procedures     # Named context bundle

# Access team context
agent "how do I deploy to production?"
# Knows team procedures automatically
```

---

## Context Storage

### Local Storage (Free)

```
~/.colony/
├── context/
│   ├── projects/
│   │   └── colony-shell/
│   │       ├── auto.ctx           # Auto-gathered
│   │       ├── files.ctx          # Added files
│   │       ├── knowledge.ctx      # Learned facts
│   │       └── conversations.ctx  # Chat history
│   ├── global/
│   │   ├── preferences.ctx        # Your AI preferences
│   │   └── knowledge.ctx          # Cross-project knowledge
│   └── snapshots/
│       └── 2024-12-17-working.ctx # Named snapshots
```

### Cloud Storage (Pro+)

```
Colony Cloud
├── users/{user_id}/
│   ├── contexts/                  # Personal contexts
│   ├── sync/                      # Cross-device sync
│   └── shared/                    # Contexts you've shared
├── teams/{team_id}/
│   ├── contexts/                  # Team contexts
│   ├── templates/                 # Context templates
│   └── agents/                    # Team agent contexts
└── public/
    └── contexts/                  # Published contexts
```

---

## Context Visualization

### Terminal UI (colony-terminal)

```
┌─────────────────────────────────────────────────────────────────┐
│  Context Inspector                                     [?] Help │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  @context                              Tokens: 12,847 / 100,000│
│  ████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░  12.8%     │
│                                                                 │
│  ┌─ Composition ────────────────────────────────────────────┐  │
│  │                                                          │  │
│  │  project    ████████████████░░░░░░░░░░  4,200 (33%)     │  │
│  │  files      ██████████░░░░░░░░░░░░░░░░  3,100 (24%)     │  │
│  │  knowledge  ████░░░░░░░░░░░░░░░░░░░░░░  1,200 (9%)      │  │
│  │  history    ████████░░░░░░░░░░░░░░░░░░  2,500 (19%)     │  │
│  │  system     █████░░░░░░░░░░░░░░░░░░░░░  1,847 (15%)     │  │
│  │                                                          │  │
│  └──────────────────────────────────────────────────────────┘  │
│                                                                 │
│  ┌─ Sources ────────────────────────────────────────────────┐  │
│  │                                                          │  │
│  │  📁 src/parser/mod.rs              1,847 tokens    [x]  │  │
│  │  📁 src/parser/pest.rs             1,253 tokens    [x]  │  │
│  │  📁 grammars/colony.pest             892 tokens    [x]  │  │
│  │  💬 "explain parser" (2h ago)        743 tokens    [x]  │  │
│  │  💬 "fix pest grammar" (1h ago)      512 tokens    [x]  │  │
│  │  📝 Knowledge: error handling        234 tokens    [x]  │  │
│  │  📝 Knowledge: parser strategy       189 tokens    [x]  │  │
│  │  🔧 System prompt                  1,847 tokens    [-]  │  │
│  │                                                          │  │
│  │  [+] Add source   [⊕] Expand all   [−] Collapse all     │  │
│  └──────────────────────────────────────────────────────────┘  │
│                                                                 │
│  ┌─ Actions ────────────────────────────────────────────────┐  │
│  │  [Optimize] [Snapshot] [Share] [Export] [Clear]         │  │
│  └──────────────────────────────────────────────────────────┘  │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Source Detail View

Click/select a source to see details:

```
┌─────────────────────────────────────────────────────────────────┐
│  Source: src/parser/mod.rs                                      │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Tokens: 1,847                                                  │
│  Added: 2 hours ago (manual)                                    │
│  Last used: 45 minutes ago                                      │
│  Relevance: High (referenced in 3 conversations)                │
│                                                                 │
│  ┌─ Content Preview ────────────────────────────────────────┐  │
│  │  1  //! Colony Shell Parser                              │  │
│  │  2  //!                                                  │  │
│  │  3  //! Hybrid parsing: Tree-sitter for incremental,    │  │
│  │  4  //! Pest for precise semantics.                     │  │
│  │  5                                                       │  │
│  │  6  pub mod tree_sitter;                                │  │
│  │  7  pub mod pest;                                       │  │
│  │  8  pub mod reconcile;                                  │  │
│  │  ...                                                     │  │
│  └──────────────────────────────────────────────────────────┘  │
│                                                                 │
│  ┌─ Actions ────────────────────────────────────────────────┐  │
│  │  [View Full] [Summarize] [Remove] [Pin] [Refresh]       │  │
│  └──────────────────────────────────────────────────────────┘  │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Context Graph View

Visualize relationships:

```
┌─────────────────────────────────────────────────────────────────┐
│  Context Graph                                                   │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│                    ┌─────────────┐                              │
│                    │   Project   │                              │
│                    │ colony-shell│                              │
│                    └──────┬──────┘                              │
│              ┌────────────┼────────────┐                        │
│              ▼            ▼            ▼                        │
│       ┌──────────┐ ┌──────────┐ ┌──────────┐                   │
│       │  Parser  │ │ Terminal │ │  Agents  │                   │
│       └────┬─────┘ └────┬─────┘ └────┬─────┘                   │
│            │            │            │                          │
│       ┌────┴────┐  ┌────┴────┐  ┌────┴────┐                    │
│       ▼         ▼  ▼         ▼  ▼         ▼                    │
│   ┌──────┐ ┌──────┐ ┌────┐ ┌────┐ ┌────┐ ┌────┐               │
│   │ pest │ │ t-s  │ │gpui│ │ ui │ │defn│ │ ctx│               │
│   └──────┘ └──────┘ └────┘ └────┘ └────┘ └────┘               │
│                                                                 │
│  Legend: [●] In context  [○] Available  [◐] Summarized        │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## Context Curation Tools

### Optimize

Reduce token usage while preserving meaning:

```colony
# Auto-optimize
context optimize
# Summarizes verbose files, removes redundancy
# Before: 45,000 tokens → After: 28,000 tokens

# Optimize specific source
context optimize src/parser/mod.rs
# Keeps key structures, removes implementation details

# Optimization strategies
context optimize --strategy=summarize    # LLM summarization
context optimize --strategy=structure    # Keep signatures, remove bodies
context optimize --strategy=recent       # Prioritize recent additions
context optimize --strategy=relevant     # Keep what's been referenced
```

### Prune

Remove low-value context:

```colony
# Interactive pruning
context prune
# Shows each source with usage stats, asks keep/remove

# Auto-prune
context prune --unused=7d         # Remove if unused for 7 days
context prune --low-relevance     # Remove AI-scored low relevance
context prune --duplicates        # Remove redundant information

# Prune simulation
context prune --dry-run
# Shows what would be removed without doing it
```

### Pin & Prioritize

Control what stays in context:

```colony
# Pin critical context (never auto-removed)
context pin src/parser/mod.rs
context pin knowledge:error-handling

# Set priority (higher = kept when space is tight)
context priority src/parser/ high
context priority tests/ low

# View priorities
context show --priorities
```

### Snapshot & Restore

Save working context states:

```colony
# Create snapshot
context snapshot "working-parser-context"
context snapshot --auto            # Auto-snapshot before major changes

# List snapshots
context snapshots
# working-parser-context     12,847 tokens   2 hours ago
# pre-refactor               34,521 tokens   2 days ago
# clean-slate                 3,200 tokens   1 week ago

# Restore snapshot
context restore "working-parser-context"

# Compare snapshots
context diff "pre-refactor" "working-parser-context"
```

---

## Context Sharing

### Share Modes

```colony
# Private link (anyone with link can view)
context share
# → https://colony.dev/ctx/abc123

# Team sharing
context share --team
# Available to all team members

# Public (listed in context marketplace)
context share --public --name="Rust CLI Project Starter"
# Discoverable by all Colony users

# Collaborative (others can edit)
context share --team --collaborative
# Team members can add/remove sources
```

### Share Specific Subsets

```colony
# Share only project context
context share --include=project

# Share curated bundle
context bundle create "parser-deep-dive" \
  --add=src/parser/ \
  --add=grammars/ \
  --add=knowledge:parser-strategy
context share bundle:parser-deep-dive

# Share with expiration
context share --expires=7d
```

### Import Shared Context

```colony
# Import from link
context import https://colony.dev/ctx/abc123

# Import from team
context import @team:onboarding

# Import from marketplace
context import marketplace:rust-cli-starter

# Preview before importing
context preview https://colony.dev/ctx/abc123
```

---

## Context Templates

Pre-built contexts for common scenarios:

### Create Templates

```colony
# Save current context as template
context template create "rust-parser-project" \
  --description="Context for building parsers in Rust" \
  --include=project \
  --include=knowledge

# Template with placeholders
context template create "microservice" \
  --placeholder="SERVICE_NAME" \
  --placeholder="DATABASE_TYPE"
```

### Use Templates

```colony
# Apply template
context template apply "rust-parser-project"

# Apply with customization
context template apply "microservice" \
  SERVICE_NAME=user-service \
  DATABASE_TYPE=postgres

# List available templates
context templates
# Built-in:
#   rust-cli          Basic Rust CLI project context
#   typescript-api    TypeScript API project
#   python-ml         Python ML project
# Team:
#   @team:onboarding  New hire onboarding context
#   @team:incident    Incident response context
# Personal:
#   my-rust-setup     My preferred Rust context
```

---

## Context Marketplace (Pro+)

### Browse & Discover

```colony
# Search marketplace
context marketplace search "kubernetes"

# Browse categories
context marketplace browse
# Categories:
#   Languages      Rust, Python, TypeScript, Go...
#   Frameworks     React, Rails, Django...
#   Tools          Kubernetes, Docker, Terraform...
#   Domains        ML, DevOps, Security...

# View details
context marketplace info "k8s-troubleshooting"
# Author: @platform-team
# Downloads: 12,847
# Rating: 4.8/5
# Tokens: ~8,000
# Description: Context for debugging Kubernetes issues
```

### Publish

```colony
# Publish to marketplace
context publish "rust-error-handling" \
  --category=Languages/Rust \
  --description="Best practices for Rust error handling" \
  --tags="rust,errors,thiserror,anyhow"

# Update published context
context publish update "rust-error-handling"

# View your publications
context marketplace mine
```

### Marketplace Economics

| Action | Creator | Colony |
|--------|---------|--------|
| Free context | - | - |
| Premium context ($2-10) | 70% | 30% |
| Team subscription includes | Flat fee | Revenue |

---

## Team Context Features (Team+)

### Team Knowledge Base

```colony
# Team-wide knowledge
context team learn "Deploy process: PR → staging → prod, requires 2 approvals"
context team learn --from=wiki/processes.md

# Categorized team knowledge
context team learn --category=deploys "..."
context team learn --category=oncall "..."

# Query team knowledge
context team show --category=oncall
```

### Team Context Analytics

```
┌─────────────────────────────────────────────────────────────────┐
│  Team Context Analytics                                         │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Most Used Contexts (last 30 days)                             │
│  1. @team:deploy-procedures      847 uses    12 contributors   │
│  2. @team:api-conventions        623 uses     8 contributors   │
│  3. @team:incident-response      412 uses    15 contributors   │
│                                                                 │
│  Context Coverage                                               │
│  Onboarding        ████████████████████  100%                  │
│  Deployment        ████████████████░░░░   80%                  │
│  Debugging         ████████░░░░░░░░░░░░   40%  ← Gap!          │
│  Security          ██████░░░░░░░░░░░░░░   30%  ← Gap!          │
│                                                                 │
│  Stale Contexts (not updated in 90 days)                       │
│  • @team:old-deploy-process      Suggest: archive or update    │
│  • @team:legacy-api              Suggest: merge with api-v2    │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Context Permissions

```colony
# Set permissions
context permissions @team:secrets --role=admin-only
context permissions @team:runbooks --role=oncall
context permissions @team:general --role=everyone

# View permissions
context permissions show @team:secrets
# Admins: @alice, @bob
# Read: admin-only
# Write: admin-only
```

---

## Context Commands Reference

### Core Commands

```colony
context show                     # Show current context summary
context show --full              # Show all sources with content
context show --tokens            # Show token breakdown
context show @context            # Alias for show

context add <source>             # Add to context
context remove <source>          # Remove from context
context clear                    # Clear all context
context clear --keep=pinned      # Clear except pinned
```

### Curation Commands

```colony
context optimize [source]        # Reduce tokens
context prune [--options]        # Remove low-value
context pin <source>             # Pin source
context unpin <source>           # Unpin source
context priority <source> <level> # Set priority
```

### Persistence Commands

```colony
context save [name]              # Save current context
context load <name>              # Load saved context
context snapshot <name>          # Create snapshot
context restore <name>           # Restore snapshot
context snapshots                # List snapshots
```

### Sharing Commands

```colony
context share [--options]        # Share context
context import <url|name>        # Import context
context preview <url|name>       # Preview before import
context publish <name>           # Publish to marketplace
```

### Team Commands

```colony
context team show                # Show team contexts
context team add <source>        # Add to team context
context team learn <fact>        # Add team knowledge
context team permissions <ctx>   # Manage permissions
```

---

## Integration with Reflection

The reflection engine observes context usage:

```
@reflect: "You add src/parser/ to context in 80% of sessions.
          Consider pinning it as default project context."

@reflect: "Context 'api-conventions' was referenced but not in context.
          3 failed agent queries might have succeeded with it loaded."

@reflect: "Your context averages 45k tokens. Most productive sessions
          use 20-30k. Consider pruning for better agent focus."
```

---

## Pricing Integration

| Feature | Free | Pro | Team | Enterprise |
|---------|------|-----|------|------------|
| Local context storage | ✓ | ✓ | ✓ | ✓ |
| Cloud sync | - | ✓ | ✓ | ✓ |
| Context sharing (links) | - | ✓ | ✓ | ✓ |
| Team contexts | - | - | ✓ | ✓ |
| Context marketplace access | - | ✓ | ✓ | ✓ |
| Publish to marketplace | - | ✓ | ✓ | ✓ |
| Team analytics | - | - | ✓ | ✓ |
| Context permissions | - | - | ✓ | ✓ |
| Custom templates | - | ✓ | ✓ | ✓ |
| Context retention policies | - | - | - | ✓ |
| Audit logs for context | - | - | - | ✓ |

---

## Summary

Context management transforms Colony from "shell with AI" to "AI-native development environment." By making context visible, persistent, and shareable, developers can:

- **Stop repeating themselves** — Context persists and syncs
- **Learn from teammates** — Shared context transfers knowledge
- **Optimize AI interactions** — Curated context = better responses
- **Build organizational memory** — Team knowledge compounds over time

The cloud service provides the infrastructure; the tools make context a manageable, valuable asset.