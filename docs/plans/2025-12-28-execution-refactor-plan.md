# Execution/Orchestration Refactor Plan

**Goal:** Split `crates/colony-shell/src/shell/engine.rs` into focused modules under `shell/execution/` (control_flow, background, traps, serialization, llm), wire callers (`orchestrator`, `repl`), and leave `main.rs` minimal. Build may break mid-way; plan covers end-to-end steps and verification.

## Scope & Targets
- Move control flow, backgrounding, traps, serialization, and LLM helper logic out of `engine.rs` into `shell/execution/`.
- Expose engine helpers (`execute_simple_command`, `execute_pipeline`, `execute_subshell`, `execute_time_statement`, `execute_assignment`, `execute_coproc`, etc.) as `pub(crate)` or relocate as needed.
- Update module wiring (`shell/mod.rs`, `orchestrator.rs`, `repl.rs`) to use `execution` exports.
- Keep `main.rs` as-is (<150 LOC); `orchestrator` remains entry coordinator.
- Basic unit tests per new module (smoke-level) if time permits.

## Module Layout
```
crates/colony-shell/src/shell/
  mod.rs
  engine.rs          # becomes thin façade or removed
  orchestrator.rs
  repl.rs
  startup.rs
  execution/
    mod.rs
    control_flow.rs  # ControlFlow, execute_list_with_control, errexit-aware statement execution, loops
    background.rs    # run_in_background, job limits, PGID/signal reset, logging
    traps.rs         # execute_exit_trap, execute_err_trap
    serialization.rs # statement_to_shell_string and helpers
    llm.rs           # handle_llm_command (llm subcommands)
```

## Step-by-Step Plan
1) **Reset execution tree**: ensure `shell/execution/` exists with empty `mod.rs` scaffold.
2) **Extract serialization**: move `statement_to_shell_string` (and helpers) from `engine.rs` into `execution/serialization.rs`; export from `execution::mod`.
3) **Extract background**: move `run_in_background` and MAX_JOBS limit into `execution/background.rs`; rely on serialization; export from `execution::mod`.
4) **Extract traps**: move `execute_exit_trap`/`execute_err_trap` into `execution/traps.rs`; export.
5) **Extract control flow**: move ControlFlow, execute_list_with_control, execute_statement_with_control/errexit_control, and loop handlers into `execution/control_flow.rs`; depend on background, traps, and engine helpers.
6) **Extract LLM**: move `handle_llm_command` into `execution/llm.rs`; export.
7) **Engine adjustments**:
   - Keep core executors (execute_simple_command, execute_pipeline, execute_time_statement, execute_subshell, execute_assignment, execute_coproc, execute_statement_for_code) in `engine.rs` and mark `pub(crate)` where needed by `execution`.
   - Remove moved code from `engine.rs` (ControlFlow, background, traps, serialization, llm, list execution) and rewire remaining internals to call `execution` where applicable.
8) **Wire module exports**:
   - Update `shell/mod.rs` to re-export `execution::*` as needed.
   - Update `orchestrator.rs` and `repl.rs` imports to use new paths (`crate::shell::execution::…` or re-exported `shell::*`).
9) **Fix compilation errors**: iterate on missing imports/visibility; run `cargo check -p colony-shell` until clean.
10) **(Optional) Tests**: add minimal unit tests in new modules (e.g., serialization round-trip for simple commands; background job limit error; control_flow executes simple list).
11) **Verify**: `cargo check -p colony-shell` (tests optional if time).

## Notes / Style Guardrails
- Avoid nested `if let`; use `let-else` per updated style guide.
- Prefer `&str` params where possible; minimize clones.
- Keep unsafe calls encapsulated; add `// SAFETY:` where applicable (signals/fork/exec already use libc/nix wrappers).
- No new comments unless necessary; prefer self-explanatory code.
- Expect warnings in other crates (parser/expand/tutorial); ignore for this refactor unless directly touched.

## Deliverables
- New `shell/execution/` modules populated with extracted logic.
- `engine.rs` slimmed to execution helpers only.
- Callers rewired (`shell/mod.rs`, `orchestrator.rs`, `repl.rs`).
- Build check log (cargo check outcome).