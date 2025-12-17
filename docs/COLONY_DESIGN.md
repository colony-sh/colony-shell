# Colony Shell Design Document

> A modern, agent-first shell that combines POSIX compatibility with AI-assisted workflows, structured data pipelines, and reflective growth.

## Vision

Colony reimagines the terminal for the AI age. Rather than choosing between "you type exact commands" and "AI does everything," Colony creates a **dialogue with your computer** where natural language and precise commands coexist seamlessly.

The shell becomes a partner in professional growth—observing patterns, surfacing insights, and accelerating expertise development.

## Core Principles

1. **POSIX Compatible, Not POSIX Limited** — Run existing scripts perfectly while offering a better path forward
2. **Agents as First-Class Citizens** — AI assistance is native, not bolted on
3. **Structured Data by Default** — Pipelines carry typed data, not just text
4. **Reflective by Design** — The shell learns your patterns and helps you grow
5. **Capability-Based Security** — Agents and commands get minimal, explicit permissions
6. **Observable Everything** — See data flow, understand failures, trace provenance

---

## Architecture

### Hybrid Parsing

Colony uses a dual-parser architecture combining incremental editing with precise semantics:

```
Source → [Tree-sitter] → CST (fast, error-tolerant)
              ↓
         [Reconciliation]
              ↓
       [Pest/PEG] → AST (precise, validated)
              ↓
         [Executor]
```

**Tree-sitter** provides:
- Sub-millisecond incremental reparsing
- Graceful error recovery (keeps parsing with ERROR nodes)
- Syntax highlighting and structural editing
- IDE-quality features

**Pest (PEG)** provides:
- Unambiguous parsing via ordered choice
- Unlimited lookahead with predicates
- Precise execution semantics
- Validation before execution

**Reconciliation** bridges them:
- Hash-based caching by Tree-sitter subtree
- Incremental validation of changed regions
- Error mapping between representations

### Channel System

Colony extends POSIX file descriptors with semantic, typed channels:

```
Standard Channels:
  0: stdin       1: stdout      2: stderr

Extended Numeric Channels:
  3-9: User-defined

Semantic Named Channels:
  @chat      Conversational exchange with LLM
  @context   Accumulating context window
  @system    System prompt modifications
  @tools     Tool/function definitions for agents
  @memory    Persistent memory operations
  @trace     Execution trace/debugging
  @audit     Security audit log
  @reflect   Reflection observations
  @patterns  Detected behavioral patterns
  @growth    Growth insights and suggestions
```

**Channel Content Types:**
```rust
enum ChannelContent {
    Text(String),
    Bytes(Vec<u8>),
    Structured(Value),      // JSON-like tables
    Image { format, data },
    Table { schema, rows },
    Markdown(String),
    Interactive(Component), // GPUI component
}
```

**Channel Operations:**
```colony
# Extended redirects
cmd 3> file.txt              # Write channel 3 to file
cmd @ctx> context.json       # Named channel to file
cmd 3|4 other_cmd            # Channel 3 → channel 4

# Channel introspection
channels list                # Show active channels
channels inspect @context    # View content and subscribers

# Channel persistence
@context >> session.ctx      # Save
@context << session.ctx      # Restore
```

### Buffer System

In-memory storage accessible by number or name:

```colony
$[0]                         # Numeric buffer
$[ctx]                       # Named buffer

echo "data" > $[0]           # Write to buffer
cat $[0]                     # Read from buffer
cmd < $[input] > $[output]   # Buffer I/O
```

Buffers are reactive—changes can trigger dependent computations.

---

## Agent Integration

### Agent Commands

```colony
# Simple invocation
agent "summarize this text"

# With options
agent -m claude-sonnet "quick analysis"
agent --model=gpt4 --timeout=30 "complex task"
agent --caps=fs,net "task needing permissions"
agent --stream "show output incrementally"

# Multi-line prompt with expansion
agent {
    Analyze the following data:
    $[data_buffer]
    
    Context: $(cat context.txt)
    Focus: ${ANALYSIS_FOCUS:-general trends}
}

# Agent pipe operator
cat data.csv |> agent "convert to JSON"
ls -la |> agent "explain these permissions"

# Chained agents
cat log.txt |> agent "extract errors" |> agent "categorize"
```

### Agent Definitions (Reusable)

```colony
agent define analyst {
    model: claude-sonnet
    system: "You are a data analyst..."
    tools: [read_file, query_db, create_chart]
    context: @project_context
}

agent define coder {
    model: claude-opus
    system: "You are an expert programmer..."
    tools: [read_file, write_file, run_tests, git]
    capabilities: [fs:., exec:cargo]
}
```

### Agent Orchestration

```colony
# Composition
pipeline analyze_and_fix {
    analyst "Find issues in $1"
    | coder "Fix the issues"
    | reviewer "Review changes"
}

# Delegation
agent analyst "Analyze codebase" --delegate-to=coder

# Memory
agent analyst remember "User prefers concise responses"
agent analyst context show
```

---

## Tuple Space

Linda-style coordination for multi-agent systems:

### Operations

```colony
# Write tuple to space
out { task: "analyze", data: $input, priority: 1 }

# Write to named space
out @tasks { id: "task-001", status: "pending" }

# Blocking read and remove (with pattern matching)
in { task: "analyze", data: ?payload, priority: ?p }
# Binds $payload and $p

# Blocking read without remove (peek)
rd { task: *, status: "pending" }

# Non-blocking variants
inp { task: "analyze" }      # Returns tuple or null
rdp { status: ?s }           # Peek without blocking

# With timeout
in { task: * } timeout 5s
```

### Pattern Matching

```colony
{ key: "literal" }           # Exact match
{ key: ?var }                # Bind to variable
{ key: * }                   # Wildcard (match anything)
{ *: "value" }               # Wildcard key
{ nested: { inner: ?x } }    # Nested patterns
```

---

## Capability System

Sandboxed execution with explicit permissions:

```colony
# Grant specific capabilities
with caps(fs:/home/user/data, net:api.example.com) {
    agent "Fetch data and save locally"
}

# Capability types
fs:<path>      # Filesystem (globs supported)
net:<host>     # Network (wildcards: *.example.com)
env:<var>      # Environment variables
exec:<cmd>     # Command execution
agent          # Agent invocation

# Script declarations
#!colony
# @caps: fs:/project, net:api.internal.com, agent:claude-haiku

# Runtime requests
request_cap net:external.api.com "Need external data"
```

---

## Reactive Bindings

```colony
# Reactive variable (re-evaluates on dependency change)
let! summary = $(agent "summarize $[input]")

# Watch for changes
watch $[input] {
    echo "Input changed, reprocessing..."
    process_data
}

# Cell-based execution (notebook-style)
cell --name="fetch" --outputs=raw_data {
    curl $API_URL
}

cell --name="process" --inputs=raw_data --outputs=clean_data {
    transform $raw_data
}

cell --name="analyze" --inputs=clean_data {
    agent "analyze $clean_data"
}

# Run DAG
run analyze   # Runs dependencies first if stale
```

---

## Reflection Engine

Colony observes usage patterns and surfaces growth insights.

### Observation Sources

| Category | Data Points |
|----------|-------------|
| Commands | Executed, timing, errors, retries, abandonment |
| Agent Chats | Questions, topics, clarifications, resolutions |
| Context | Time of day, project, focus duration, energy |
| Files | Touched, edit frequency, navigation paths |
| Pipelines | Common chains, bottlenecks, repeated sequences |
| Learning | New tools, new flags, mistakes, corrections |

### Pattern Categories

**Workflow Friction**
```
@reflect: "You run 'git status' before and after 'git add' 94% of the time.
          Consider: alias gac='git add . && git status && git commit'"
```

**Error Patterns**
```
@reflect: "kubectl commands fail 34% with context errors.
          Consider: Context indicator in prompt + prod confirmation?"
```

**Learning Opportunities**
```
@reflect: "You used 'find | xargs grep' 12 times this week.
          'ripgrep' would be 10x faster: rg 'pattern'"
```

**Productivity Insights**
```
@reflect: "Most productive: Tuesday/Thursday mornings, 9-11am
          Least productive: High context-switching days (4+ projects)"
```

**Agent Interaction Patterns**
```
@reflect: "Best results when you provide examples upfront.
          You abandon chats when agent gives walls of text."
```

### Reflection Interface

```colony
reflect now                  # Immediate analysis
reflect today                # Today's patterns
reflect week                 # Weekly summary
reflect "why do I struggle with docker?"
reflect session              # Interactive exploration
```

### Privacy Controls

```colony
reflect config storage local       # Never leaves machine
reflect config storage encrypted   # At-rest encryption
reflect exclude path ~/personal    # Directory exclusion
reflect exclude command "pass*"    # Command exclusion
reflect config agent local         # Local LLM only
reflect retention 90d              # Auto-delete policy
reflect purge                      # Delete all data
```

---

## Terminal UI (colony-terminal)

Built with Rust and GPUI for native performance and feel.

### Layout

```
┌─────────────────────────────────────────────────────────────────────┐
│                     Colony Terminal                                  │
├──────────┬──────────────────────────────────┬───────────────────────┤
│  File    │         Main Terminal            │    Agent/Context      │
│  Tree    │  ┌────────────────────────────┐  │       Panel           │
│  Panel   │  │ $ cat data | agent "..."   │  │  ┌─────────────────┐  │
│          │  │                            │  │  │ @context:       │  │
│ ├─ src/  │  │ [Structured output]        │  │  │   tokens: 4521  │  │
│ │ └─ ..  │  │ [Inline images]            │  │  │   files: 3      │  │
│ ├─ test/ │  │ [Interactive tables]       │  │  │ @chat: [hist]   │  │
│          │  │                            │  │  └─────────────────┘  │
│          │  │ $ _                        │  │                       │
│          │  └────────────────────────────┘  │  ┌─ Agent Tools ────┐ │
│          │  ┌── Pipeline Observer ───────┐  │  │ [read] [write]   │ │
│          │  │ src → grep → sort → head   │  │  │ [search] [run]   │ │
│          │  │ 1.2M   847    847    10    │  │  └──────────────────┘ │
├──────────┴──────────────────────────────────┴───────────────────────┤
│  [main ✓] [session #4521] [4.5k tokens] [reflect: 2 insights]       │
└─────────────────────────────────────────────────────────────────────┘
```

### Rich Rendering

```colony
# Inline images
cat photo.jpg                    # Renders inline

# Data tables (interactive)
ls -la | to table | interactive  # Sortable, filterable

# Charts
ps aux | to table | chart --type=bar cpu,mem

# Markdown
agent "explain this" | render md

# Diff visualization
git diff | render diff           # Syntax-highlighted
```

### Panel Commands

```colony
panel tree toggle                # File tree visibility
panel tree filter "*.rs"         # Filter view
panel tree group-by git-status   # Semantic grouping

tree select                      # Multi-select in panel
selected | xargs wc -l           # Operate on selection

tree bookmark add @project .     # Quick access
cd @project                      # Jump to bookmark
```

### Session Management

```colony
session status                   # Current session info
session replay 2024-12-15-001    # Replay past session

# Time travel
timeline                         # Visual timeline
rewind 5                         # Go back 5 commands (with state)
branch experiment                # Create branch
snapshot "before risky change"   # Named snapshot
restore "before risky change"    # Full state restore

# Export
session export --from=10 --to=20 > workflow.colony
```

---

## Addressing User Pain Points

Based on community research, Colony specifically addresses:

| Pain Point | Colony Solution |
|------------|-----------------|
| Silent failures in bash | Strict mode by default, structured errors |
| Text parsing hell | Structured data pipelines, native JSON/YAML |
| No sensible defaults | Works well out of box, Fish-style UX |
| Arcane syntax | Clean, readable syntax for new constructs |
| No real types | Typed channels and structured data |
| Poor error messages | Precise error locations, suggestions |
| Configuration complexity | Sensible defaults, progressive disclosure |
| Cross-platform inconsistency | Identical behavior on Linux/macOS/Windows |
| Slow startup (zsh+plugins) | Fast startup, lazy loading |
| AI privacy concerns | Local-first, explicit consent, no mandatory login |

---

## File Structure

```
colony/
├── colony-shell/           # Shell implementation
│   ├── src/
│   │   ├── parser/
│   │   │   ├── tree_sitter/    # Incremental parser
│   │   │   ├── pest/           # PEG parser
│   │   │   └── reconcile.rs    # Bridge layer
│   │   ├── channels/           # Channel system
│   │   ├── buffers/            # Buffer management
│   │   ├── agents/             # Agent integration
│   │   ├── tuples/             # Tuple space
│   │   ├── capabilities/       # Security model
│   │   ├── reflect/            # Reflection engine
│   │   └── executor/           # Command execution
│   └── Cargo.toml
│
├── colony-terminal/        # Terminal application (GPUI)
│   ├── src/
│   │   ├── ui/
│   │   │   ├── terminal_view.rs
│   │   │   ├── file_tree.rs
│   │   │   ├── agent_panel.rs
│   │   │   ├── pipeline_observer.rs
│   │   │   └── reflection_panel.rs
│   │   ├── rendering/
│   │   │   ├── rich_content.rs
│   │   │   ├── tables.rs
│   │   │   └── images.rs
│   │   └── main.rs
│   └── Cargo.toml
│
├── grammars/
│   ├── tree-sitter-colony/
│   │   └── grammar.js
│   └── pest-colony/
│       └── colony.pest
│
└── docs/
    ├── DESIGN.md           # This document
    ├── SYNTAX.md           # Language reference
    └── MIGRATION.md        # From bash/zsh/fish
```

---

## Development Phases

### Phase 1: Foundation
- [ ] Hybrid parser (Tree-sitter + Pest)
- [ ] Basic POSIX command execution
- [ ] Extended channel system
- [ ] Buffer references

### Phase 2: Agents
- [ ] Agent command syntax
- [ ] Agent pipe operator
- [ ] Capability blocks
- [ ] Context channel management

### Phase 3: Coordination
- [ ] Tuple space implementation
- [ ] Reactive bindings
- [ ] Multi-agent orchestration

### Phase 4: Terminal
- [ ] GPUI terminal core
- [ ] File tree panel
- [ ] Agent/context panel
- [ ] Rich content rendering

### Phase 5: Reflection
- [ ] Observation collection
- [ ] Pattern detection
- [ ] Insight generation
- [ ] Growth tracking

### Phase 6: Polish
- [ ] Session management
- [ ] Time travel
- [ ] Cross-platform testing
- [ ] Performance optimization

---

## Key Differentiators

1. **Not just a shell or just an AI tool** — A synthesis that makes both better
2. **POSIX compatibility** — Your existing scripts work unchanged
3. **Structured data native** — No more parsing text with regex
4. **Reflection built-in** — The shell helps you become a better developer
5. **Capability security** — Agents can't do more than you allow
6. **Observable** — See what's happening, understand failures
7. **Local-first** — Your data stays yours

---

## Inspiration & References

- **Nushell** — Structured data pipelines
- **Fish** — Out-of-box experience, autosuggestions
- **Oil Shell** — Gradual POSIX migration
- **PowerShell** — Object pipelines (done right this time)
- **Linda** — Tuple space coordination
- **Tree-sitter** — Incremental parsing
- **Ghostty** — Native terminal feel
- **Warp** — AI integration (without the login)