# Colony Shell Features Audit Report

**Date:** December 2025  
**Auditor:** Sisyphus Agent

## Executive Summary

Audited `features.json` against design documents and codebase. 

**Key Findings:**
- **41 features currently tracked** (F001-F041)
- **Many POSIX features implemented but not explicitly tracked** (case, while, functions, subshells already work!)
- **23 new features identified** for language extensions and improvements
- **Recommend consolidation** of existing untracked work + addition of extension features

---

## Roadmap Ownership

This document is the **canonical roadmap** for colony-shell feature delivery.

- If other docs conflict, **this file wins**.
- README should link here for roadmap guidance.
- `docs/IMPLEMENTATION_ARCHITECTURE.md` tracks **current implementation status** and should reference this roadmap.
- Re-run and refresh this audit **quarterly** or after major refactors.

---

## Current Feature Status by Category

### Core Infrastructure (COMPLETE)
| ID | Feature | Status |
|----|---------|--------|
| F001 | Lexer (deprecated) | deprecated |
| F002 | Parser (deprecated) | deprecated |
| F021 | Pest PEG Parser | **complete** |
| F022 | Tree-sitter Parser | **complete** |
| F003 | Expansion System | **complete** |

### Execution Engine (COMPLETE)
| ID | Feature | Status | Notes |
|----|---------|--------|-------|
| F004 | Simple Command Execution | **complete** | |
| F005 | Pipeline Execution | **complete** | |
| F006 | Redirections | **complete** | |
| F007 | Core Builtins | **complete** | |
| F008 | Job Control | **complete** | Basic infrastructure |
| F039 | Heredoc Support | **complete** | 84% BATS |
| F040 | Read Builtin | **complete** | 72% BATS |
| F041 | Tilde Expansion | **complete** | |

### Agent & AI (MOSTLY COMPLETE)
| ID | Feature | Status |
|----|---------|--------|
| F014 | Agent NL Detection + Router | **complete** |
| F015 | Claude ADK Integration | **complete** |
| F009 | OSC 2337 Protocol | **complete** |

### Coordination (COMPLETE)
| ID | Feature | Status |
|----|---------|--------|
| F010 | Tuple Space (in-process) | **complete** |
| F011 | Tuple Space (IPC) | **complete** |
| F012 | Persistent Tuple Space | **complete** |

### Channels (PARTIALLY COMPLETE)
| ID | Feature | Status |
|----|---------|--------|
| F024 | Extended Channels (fd 3-6) | **complete** |
| F025 | Buffer Management | new |

### Configuration (COMPLETE)
| ID | Feature | Status |
|----|---------|--------|
| F020 | .colonyrc Configuration | **complete** |

### Test Suite
| ID | Feature | Status |
|----|---------|--------|
| F023 | Comprehensive Tests | **complete** | 100% initial suite |

### Not Started
| ID | Feature | Status |
|----|---------|--------|
| F013 | Memory Substrate (LanceDB) | new |
| F016 | MCP Connection Pool | new |
| F017 | MCP TypeScript Codegen | new |
| F018 | MCP Executor Session | new |
| F019 | Goal & Task Tracking | new |
| F025 | Buffer Management | new |
| F026 | Pane Management | new |
| F027 | Reactive Prompt | new |
| F028 | File Tree State | new |
| F029 | File Tree Protocol | new |
| F030 | Special Files | new |

---

## ALREADY IMPLEMENTED (Not in features.json)

These features are working but should be tracked:

### POSIX Control Flow (Working)
- ✅ **Case statements** - `execute_case_internal()` exists
- ✅ **While loops** - `execute_while_internal()` exists  
- ✅ **Until loops** - `execute_until_internal()` exists (variant of while)
- ✅ **Function definitions** - `execute_function()` exists
- ✅ **Subshell execution** - Handled in `execute_pipeline_element_in_child`
- ✅ **Brace groups** - Handled in pipeline execution
- ✅ **Array expansion** - `${arr[@]}`, `${arr[*]}` in parameter.rs
- ✅ **Arithmetic evaluation** - `$((expr))` in arithmetic.rs

### Additional Implemented Features
- ✅ **History expansion** - `!!`, `!n`, `!-n`, `!prefix` in history.rs
- ✅ **Printf builtin** - Full implementation in builtins.rs
- ✅ **Declare builtin** - Array/assoc array support
- ✅ **Alias expansion** - Working in execute_simple_command

---

## NEW FEATURES TO ADD

### Category A: Language Extensions (from LANGUAGE_EXTENSIONS.md)

| ID | Feature | Description | Priority | Dependencies |
|----|---------|-------------|----------|--------------|
| F042 | **Typed Variables** | `var`, `const` keywords with optional type annotations | 2 | F021 |
| F043 | **Structured Data Literals** | Native `{}` dict and `[]` list literals with dot notation | 2 | F021 |
| F044 | **Expression Syntax** | `$[expr]` rich expressions with functions, methods, lambdas | 3 | F021, F042 |
| F045 | **Pipeline Extensions** | `?>` error-aware, `\|>` tap, `!>` error handler pipes | 3 | F005 |
| F046 | **Error Handling** | `try/catch/finally` structured error handling | 3 | F021 |
| F047 | **Proc Definitions** | `proc name(args) {}` with named params, defaults, return values | 3 | F021 |
| F048 | **Pattern Matching** | `match` statement with destructuring and guards | 4 | F021, F043 |
| F049 | **Namespaced Builtins** | `cmd:sub` syntax for organized command namespacing | 3 | F007 |

### Category B: POSIX Completeness

| ID | Feature | Description | Priority | Dependencies |
|----|---------|-------------|----------|--------------|
| F050 | **Process Substitution** | `<(cmd)` and `>(cmd)` for command output as file | 3 | F006 |
| F051 | **Coprocess Support** | `coproc` for bidirectional pipes | 4 | F008 |
| F052 | **Extended Test** | `[[ ]]` conditional with regex, pattern matching | 2 | F007 |
| F053 | **Let Command** | `let` for arithmetic with multiple expressions | 3 | F003 |
| F054 | **Select Statement** | `select name in words; do; done` menu | 4 | F021 |

### Category C: Interactive Features (from COMMAND_RECOMMENDATIONS.md)

| ID | Feature | Description | Priority | Dependencies |
|----|---------|-------------|----------|--------------|
| F055 | **Command Recommendations** | Real-time suggestions, ghost text, menu | 3 | F022, F009 |
| F056 | **Live Command Preview** | Preview destructive command effects | 4 | F055 |
| F057 | **Safety Checking** | Dangerous command detection and warnings | 3 | F021 |
| F058 | **Contextual Help** | TLDR/man integration, inline help | 4 | F055 |

### Category D: Terminal

| ID | Feature | Description | Priority | Dependencies |
|----|---------|-------------|----------|--------------|
| F059 | **Colony Terminal** | GPUI-based terminal emulator | 5 | F009, F024, F027 |

---

## Recommended Roadmap

### Phase 1: POSIX Parity (Next)
1. F052: Extended Test `[[ ]]`
2. F050: Process Substitution
3. F053: Let Command
4. Expand test suite with upstream bash tests

### Phase 2: Core Language Extensions
1. F042: Typed Variables (`var`, `const`)
2. F043: Structured Data (`{}`, `[]`)
3. F044: Expression Syntax (`$[expr]`)
4. F049: Namespaced Builtins

**Scope notes (implementation-level):**
- **F042 Typed Variables**: tuple destructuring targets (e.g., `var (a, b) = ...`) — **Status:** implemented
- **F043 Structured Data**: list/dict/tuple literals, indexing, field access, optional chaining — **Status:** implemented
- **F044 Expression Syntax**: function + method calls; helper set includes `len`, `join`, `split`, `keys`, `values`, `get`, `has`, `trim`, `replace`, `contains`, `first`, `last`, `push`, `pop`, `insert`, `remove`, `slice`; numeric string coercions in binary ops — **Status:** implemented
- **F049 Namespaced Builtins**: `cmd:sub` aliases `cmd sub` (e.g., `context:show`) — **Status:** implemented

### Phase 3: Error Handling & Control Flow
1. F046: Error Handling (`try/catch`)
2. F045: Pipeline Extensions (`?>`, `|>`)
3. F047: Proc Definitions

### Phase 4: Interactive Experience
1. F055: Command Recommendations
2. F057: Safety Checking
3. F056: Live Preview
4. F058: Help System

### Phase 5: Advanced Features
1. F048: Pattern Matching
2. F051: Coprocess
3. F054: Select Statement
4. F059: Colony Terminal

---

## Dependency Graph

```
F021 (pest parser) ───┬─── F042 (Typed Variables)
                      │         │
                      │         └─── F044 (Expression Syntax)
                      │
                      ├─── F043 (Structured Data)
                      │         │
                      │         └─── F048 (Pattern Matching)
                      │
                      ├─── F046 (Error Handling)
                      │
                      └─── F047 (Proc Definitions)

F005 (Pipeline) ──────────── F045 (Pipeline Extensions)

F006 (Redirections) ──────── F050 (Process Substitution)

F007 (Builtins) ──────┬───── F049 (Namespaced Builtins)
                      │
                      └───── F052 (Extended Test)

F008 (Job Control) ───────── F051 (Coprocess)

F022 (Tree-sitter) ───┬───── F055 (Recommendations)
                      │           │
F009 (OSC 2337) ──────┤           ├─── F056 (Preview)
                      │           │
                      │           └─── F058 (Help)
                      │
                      └───── F057 (Safety)

F024 (Channels) ──────┬───── F059 (Terminal)
F027 (Prompt) ────────┘
```

---

## Notes

1. **Consolidation needed**: Many POSIX features work but aren't tracked
2. **Test coverage**: Need more BATS tests for compound commands
3. **Grammar support exists**: pest grammar already supports `var`, `const`, dict/list literals
4. **Parser ready**: Most extension syntax can be parsed; needs executor support
5. **Two-track development**: POSIX completion and extensions can proceed in parallel

---

*This audit should be re-run quarterly to ensure roadmap alignment.*
