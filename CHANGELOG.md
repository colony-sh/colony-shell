# Changelog

All notable changes to Colony Shell will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- GitHub Actions CI/CD workflows
- Automated crate publishing to crates.io
- Spec adherence audit script
- BATS integration test framework

## [0.1.0] - 2025-12-17

### Added - Core Shell Features
- POSIX-compliant command execution (F004)
- Pipeline execution with pipes (F005)
- I/O redirections: >, >>, <, >&, heredocs (F006)
- Core builtins: cd, export, unset, exit, pwd, echo, type, alias, unalias (F007)
- Job control infrastructure: jobs, fg, bg, wait (F008)
- Command substitution: $(cmd) support (F023)
- For loops with break/continue (F023)
- If/elif/else statements (F023)
- Test builtin with all POSIX operators (F023)
- Variable expansion: $VAR, ${VAR}, ${VAR:-default} (F003, F023)
- Builtin redirect support with RAII guards (F023)

### Added - Parser & Grammar
- Pure PEG parser using pest (F021)
- Tree-sitter parser for editor features (F022)
- Two-parser hybrid architecture
- Word expansion with field splitting (F003)
- Heredoc preprocessing (F021)
- POSIX compliance: 90%+ on BATS test suite (F023)

### Added - AI Agent Features
- Multi-provider model router: Anthropic, OpenAI, Ollama (F014)
- Auto-detection with free Ollama fallback (F014)
- `llm` command for model management (F014)
- Claude ADK integration (F015)
- Natural language detection (# and ? prefixes) (F014)
- Project configuration with direnv-style trust (.colonyrc) (F020)

### Added - Reactive Environment
- Linda-style tuple space coordination (F010)
- Persistent tuple space with SQLite (F012)
- IPC notifications layer (F011)
- Reactive environment: export/unset/cd emit tuples (F010)
- Observable state changes with 🔄 notifications

### Added - Terminal Communication
- OSC 2337 protocol emitter (F009)
- Extended channels system: fd 3-6 (F024)
- Context channel (stdcontext) with `context` builtin (F024)
- Channel redirects: >&3, >&4, >&5, >&6 (F024)
- Tuple space IPC for child→parent channel writes (F024)

### Architecture
- Workspace with 14 crates
- Async/await with tokio
- Fork/exec using nix crate
- Concurrent collections: scc::HashMap, parking_lot
- Error handling: thiserror + miette

### Testing
- 90%+ POSIX compliance validated
- 20/20 BATS tests passing
- Comprehensive unit test coverage
- Real-world bash script testing

## [0.0.1] - Initial Development

### Added
- Project structure and workspace setup
- Basic lexer and parser (deprecated)
- Core execution infrastructure

---

[Unreleased]: https://github.com/mcfearsome/colony-shell/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/mcfearsome/colony-shell/releases/tag/v0.1.0
