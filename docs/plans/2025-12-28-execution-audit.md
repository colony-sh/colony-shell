# Execution Engine / Process Management / Backgrounding Audit

## Strengths
- Control-flow propagation is explicit via `ControlFlow` and `execute_statement_with_errexit_control`.
- Backgrounding pipeline uses `run_in_background` with job-table updates and PGID creation; SIGINT/SIGQUIT/SIGTSTP reset to defaults in child.
- Coproc path present (execvp or shell string), and `execute_command_with_status` handles alias-aware parsing for `-c`.

## Issues / Risks
1) Child safety after fork
- `run_in_background` clones `Statement` and `ExpansionContext` in parent, then child calls `execute_statement_for_process_sub` with cloned context (allocations post-fork, shared state risk).
- No `_exit`-only path on exec failure; no full fd cleanup; allocator use after fork is unsafe in theory.
- Coproc child allocates (CString, Vec) after fork; fds not fully closed on all paths.

2) Job control robustness
- Background job uses `list_jobs().len()` as job index (unstable if jobs removed; race with reordering).
- No tcsetpgrp handoff here; foregrounding absent in this module (may be elsewhere, not visible).
- setpgid result ignored.

3) Error surfacing
- `run_in_background` errors map to `(1, Normal)` silently; no user diagnostics.
- `execute_command_with_status` prints only parse error, lacking command context.

4) Resource hygiene
- Child path doesn’t close extra inherited fds (tuple pipes, channels, tracing fds) → leak risk into background jobs.
- No wait/reap hook here; relies on external SIGCHLD/JobTable handling—coupling is unclear.

5) Coproc specifics
- Ad-hoc exec selection; shell-string path uses env unsafely; exec failure exits 127 silently.
- No backpressure/limits on coproc pipes; parent failure before registration could leak fds.

6) Warnings/unused indicate dead paths
- Large unused imports and unreachable patterns (tutorial/expand/exec) add noise and hide issues.

## Recommendations (Prioritized)
- Child safety: Minimize work after fork; close all non-stdio fds except intended pipes; handle setpgid errors; on exec failure, write to stderr then `_exit(127)`. Avoid context/state mutation in child—pass only serialized command or argv.
- Job control: Assign stable job IDs from JobTable (not `list_jobs().len()`); check setpgid/tcsetpgrp results; ensure foregrounding/terminal handoff is centralized and called.
- Diagnostics: Emit user-facing errors when fork/setpgid/exec fail; propagate `run_in_background` errors with context.
- Resource hygiene: Add helper to close extraneous fds in child; bound/validate tuple/channel fds before forking; consider CLOEXEC on management fds.
- Coproc: Add explicit exec-failure message + `_exit(127)`; close unused pipe ends on all paths; consider aligning with background path helpers for consistency.
- Cleanup: Remove unused imports/patterns in shell/tutorial to quiet warnings; improves signal for future audits.
