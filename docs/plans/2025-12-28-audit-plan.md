# Dual Audit (Ideas & Code) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Produce two markdown documents: (1) Ideas/vision audit with recommendations; (2) Rust code best-practices audit with concrete improvement plan. Both saved under `docs/plans/`.

**Architecture:** Pure documentation task; synthesize existing project docs (README, IMPLEMENTATION_ARCHITECTURE.md, features.json) plus quick code sampling in `crates/colony-shell`, `colony-exec`, `colony-parser`, `colony-expand`. No code changes; recommendations only.

**Tech Stack:** Markdown, Rust ecosystem knowledge (Tokio, nix, pest, tree-sitter, miette/thiserror), POSIX shell semantics.

---

### Task 1: Gather context

**Files:**
- Read: `README.md`, `docs/IMPLEMENTATION_ARCHITECTURE.md` (at least executive summary), `.claude/system/features.json` (statuses)
- Skim: `crates/colony-shell/src/main.rs` (structure), `crates/colony-exec/src/*.rs`, `crates/colony-expand/src/*.rs`, `crates/colony-parser/src/pest_api.rs`

**Step 1:** Read the docs above and capture key facts (vision, differentiators, completion %, dual-parser rationale, tuple space/extended channels).

**Step 2:** Sample code structure for adherence patterns (error handling, module boundaries, async/fork, unsafe use, logging, tracing, tests presence).

**Step 3:** Note any gaps/risks to inform both audits.

### Task 2: Draft Ideas/Vision Audit

**Files:**
- Create: `docs/plans/2025-12-28-ideas-audit.md`

**Step 1:** Outline sections: (a) Vision fidelity vs implementation, (b) Strategic differentiators, (c) Gaps/risks/opportunities, (d) Recommendations (near/mid term), (e) Success metrics.

**Step 2:** Write full analysis using gathered context; ensure bullets with clear priorities.

**Step 3:** Self-review for clarity and actionability.

### Task 3: Draft Code Best-Practices Audit

**Files:**
- Create: `docs/plans/2025-12-28-code-audit.md`

**Step 1:** Outline sections: (a) Overall codebase health, (b) Architecture & modularity, (c) Concurrency/process safety, (d) Error handling & diagnostics, (e) Testing/CI, (f) Security/unsafe, (g) Performance & profiling, (h) Actionable recommendations with owners/priority.

**Step 2:** Write full analysis referencing concrete modules (shell, exec, expand, parser, agent, tuple) and note observed issues (e.g., god-object main.rs, manual arg parsing, trap handling, test coverage focus, TODO hotspots).

**Step 3:** Self-review for specificity and feasibility.

### Task 4: Deliverables and wrap-up

**Files:**
- Ensure both audit files saved in `docs/plans/`

**Step 1:** Summarize outputs to user with file paths.

**Step 2:** If requested later, propose execution path (subagent-driven or parallel session) using executing-plans skill.
