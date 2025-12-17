# Colony Shell Implementation Specification

**Version:** 1.0.0-draft  
**Date:** December 2025  
**Status:** Implementation Ready

---

# Part 0: Overview

## 0.1 Purpose

Colony is an agent-first shell that provides full POSIX compatibility while introducing modern language features for structured data processing, AI agent orchestration, and reliable scripting. This specification defines the complete Colony system in sufficient detail for implementation.

## 0.2 Design Goals

1. **POSIX Compatibility**: Run existing shell scripts unchanged in compatibility mode
2. **Progressive Enhancement**: Opt-in to modern features via directives or binary selection
3. **Typed Pipelines**: Data carries type annotations through pipelines
4. **Agent-Native**: First-class support for AI agents and multi-agent orchestration
5. **Fail-Safe Defaults**: Modern mode fails on errors; legacy mode preserves shell semantics
6. **Observable**: Multiple semantic channels (context, llm, tools) with visual buffer management
7. **Project-Aware**: Special files configure behavior per-project

## 0.3 Document Structure

| Part | File | Content |
|------|------|---------|
| 0 | `00-overview.md` | This document |
| 1 | `01-architecture.md` | System architecture, crates, binaries |
| 2 | `02-lexical.md` | Lexical structure, tokens, modes |
| 3 | `03-types.md` | Type system, pipeline types |
| 4 | `04-ast.md` | Abstract syntax tree definitions |
| 5 | `05-execution.md` | Evaluator, scoping, execution model |
| 6 | `06-pipelines.md` | Pipeline operators, type flow |
| 7 | `07-channels.md` | Extended channels (stdcontext, stdllm, etc.) |
| 8 | `08-buffers.md` | Buffer storage, pane management |
| 9 | `09-agents.md` | Agent definition, execution, orchestration |
| 10 | `10-coordination.md` | Tuple space, sessions, messaging |
| 11 | `11-mcp.md` | MCP client integration |
| 12 | `12-special-files.md` | AGENT.md, CONTEXT.md, TOOLS.md |
| 13 | `13-interactive.md` | Live preview, help system |
| 14 | `14-builtins.md` | Builtin commands reference |
| 15 | `15-grammar.md` | Formal grammar specification |
| 16 | `16-context.md` | Context management system |

## 0.4 Conventions

### 0.4.1 Code Examples

```colony
# This is Colony shell syntax
var x = 42
echo $x -> jq '.value'
```

```rust
// This is Rust implementation code
pub struct Example {
    field: String,
}
```

### 0.4.2 Requirement Levels

- **MUST**: Absolute requirement
- **SHOULD**: Recommended but not required
- **MAY**: Optional feature

### 0.4.3 Notation

- `[x]` - Optional element
- `{x}` - Zero or more repetitions
- `x | y` - Alternatives
- `'x'` - Literal token

## 0.5 Key Innovations

### 0.5.1 Dual-Binary Architecture

Colony ships as two binaries from the same codebase:

| Binary | Default Mode | Purpose |
|--------|--------------|---------|
| `colony-sh` | POSIX | Drop-in `/bin/sh` replacement |
| `colony` | Modern | Full Colony features enabled |

### 0.5.2 Extended Channels

Beyond stdin/stdout/stderr, Colony provides semantic channels:

| Channel | FD | Purpose |
|---------|-----|---------|
| stdcontext | 3 | Session context accumulation |
| stdllm | 4 | LLM conversation stream |
| stdtool | 5 | Tool call/response logging |
| stdtrace | 6 | Execution tracing |

### 0.5.3 Pipeline Operators

| Operator | Name | Semantics |
|----------|------|-----------|
| `\|` | Pipe | POSIX byte stream |
| `->` | Arrow | Typed pipe with annotations |
| `=>` | Fat Arrow | Auto-converting pipe |
| `~>` | Alter | Structured modification |

### 0.5.4 Special Files

| File | Purpose |
|------|---------|
| `AGENT.md` | Define project-specific agents |
| `CONTEXT.md` | Seed session context |
| `TOOLS.md` | Configure available tools |
| `PROMPT.md` | Prompt templates |
| `.colony/` | Project configuration |

## 0.6 Implementation Language

Colony is implemented in Rust, using:

- **GPUI**: Terminal UI framework (from Zed editor)
- **tokio**: Async runtime
- **serde**: Serialization
- **tree-sitter**: Syntax highlighting (optional)

## 0.7 Target Platforms

- Linux (primary)
- macOS
- Windows (WSL recommended)

## 0.8 License

Colony is open source under the MIT license.

---

*Continue to Part 1: Architecture*