# Colony Shell Documentation

## Quick Start

**[QUICKSTART.md](QUICKSTART.md)** - Build and run Colony Shell

---

## Specification

The formal specification lives in `spec/`. This is the authoritative source for implementation details.

| Part | File | Content |
|------|------|---------|
| 0 | [overview](spec/0-overview.md) | Purpose, design goals, conventions |
| 1 | [architecture](spec/1-architecture.md) | Crates, binaries, system overview |
| 2 | [lexical-structure](spec/2-lexical-structure.md) | Tokens, modes, parsing |
| 3 | [type-system](spec/3-type-system.md) | Types, pipeline types |
| 4 | [ast-definitions](spec/4-ast-definitions.md) | AST node definitions |
| 5 | [execution](spec/5-execution.md) | Evaluator, scoping |
| 6 | [pipelines](spec/6-pipelines.md) | `|`, `->`, `=>`, `~>` operators |
| 7 | [channels](spec/7-channels.md) | fd 3-6 (stdcontext, stdllm, stdtool, stdtrace) |
| 8 | [buffers](spec/8-buffers.md) | Buffer storage, panes |
| 9 | [agents](spec/9-agents.md) | Agent definition, orchestration |
| 10 | [coordination](spec/10-coordination.md) | Tuple space, sessions |
| 11 | [mcp](spec/11-mcp.md) | MCP client integration |
| 12 | [special-files](spec/12-special-files.md) | AGENT.md, CONTEXT.md, etc. |
| 13 | [interactive](spec/13-interactive.md) | Live preview, help |
| 14 | [builtins](spec/14-builtins.md) | Command reference |
| 15 | [grammar](spec/15-grammar.md) | Formal grammar |
| 16 | [context](spec/16-context.md) | Context management system |

---

## Design Documents

Design rationale, research, and feature concepts in `design/`.

### Core Architecture
- **[ARCHITECTURE.md](design/ARCHITECTURE.md)** - System vision, hybrid shell-terminal model
- **[CHANNELS.md](design/CHANNELS.md)** - Extended channels and special files
- **[PARSER.md](design/PARSER.md)** - Two-parser hybrid (Tree-sitter + Pest)

### Features
- **[CONTEXT.md](design/CONTEXT.md)** - Context management as first-class citizen
- **[REFLECTION.md](design/REFLECTION.md)** - Deliberate practice infrastructure
- **[ENVIRONMENT.md](design/ENVIRONMENT.md)** - PATH and environment management
- **[TOOLS.md](design/TOOLS.md)** - Tool classification (PROGRAMMATIC vs USER_FACING)

### Terminal UI
- **[FILE_TREE.md](design/FILE_TREE.md)** - File tree pane location analysis
- **[FILE_TREE_IMPL.md](design/FILE_TREE_IMPL.md)** - File tree implementation details

### Business
- **[MONETIZATION.md](design/MONETIZATION.md)** - Tier structure (Free/Pro/Team/Enterprise)
- **[RESEARCH.md](design/RESEARCH.md)** - Survey of modern shell innovations

---

## Archive

Historical documents from parser development in `archive/`. These capture the journey from hand-written parser to pure PEG approach.

---

## Structure

```
docs/
├── README.md           # This file
├── QUICKSTART.md       # Getting started guide
├── spec/               # Formal specification (16 parts)
├── design/             # Design rationale and concepts
└── archive/            # Historical documents
```
