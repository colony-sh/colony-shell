# Remove Legacy Lexer/Parser Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Remove the unused hand-written lexer/parser/token pipeline from `colony-parser`, relying solely on the pest-based API. Eliminate related exports/examples to stop warnings and avoid accidental usage.

**Architecture:** Keep AST/span/builder/pest_api intact. Remove module wiring to legacy lexer/parser/token, delete files, and update examples to the pest API. Ensure crate exports only the pest-based ParseError.

**Tech Stack:** Rust, pest, cargo.

---

### Task 1: Detach legacy modules from lib

**Files:**
- Modify: `crates/colony-parser/src/lib.rs`

**Steps:**
1. Remove `mod lexer; mod parser; mod token;` and related `pub use` re-exports.
2. Remove legacy `LexError` and old `ParseError` types; re-export pest API types as `ParseError`.
3. Keep `span`, `ast`, `builder`, `pest_api`, `pest_parser`, `heredoc`, `alias_expand`, `arith_preprocess` modules intact.

### Task 2: Delete legacy source files

**Files:**
- Delete: `crates/colony-parser/src/lexer.rs`
- Delete: `crates/colony-parser/src/parser.rs`
- Delete: `crates/colony-parser/src/token.rs`

**Steps:**
1. Remove the files to avoid drift and dead code.

### Task 3: Update examples to pest API

**Files:**
- Modify: `examples/parser_demo.rs`

**Steps:**
1. Rewrite to use `colony_parser::parse` (and optionally `parse_statement`) instead of `Lexer`/`Parser`/`ShellOptions`.

### Task 4: Clean up warnings and docs references (light)

**Files:**
- Modify: `crates/colony-parser/src/pest_parser.rs` (remove unused `pest::Parser` import if still warned)
- Optional: `docs/spec/2-lexical-structure.md` retains legacy design notes; no code impact, leave untouched.

**Steps:**
1. After deletions, fix any compiler warnings in pest parser imports if needed.

### Task 5: Verification

**Steps:**
1. (If requested) `cargo check -p colony-parser` to ensure crate builds without legacy modules.
2. Report summary of removed modules and updated example.
