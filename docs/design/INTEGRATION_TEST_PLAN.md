# Colony Shell + Colony Terminal Integration Test Plan

## Goals

- Validate the shell/terminal contract end-to-end (protocol, state, UI events).
- Prevent split-of-responsibility drift (logic in shell, rendering in terminal).
- Detect protocol regressions before either repo ships.
- Ensure graceful fallback in non-Colony terminals.

## Scope

- Protocol: OSC 2337 payloads (shell -> terminal) and terminal callbacks (terminal -> shell).
- UX: prompt rendering, widgets, approvals, recommendations, and structured output.
- Compatibility: version negotiation and forward-compat handling.
- Fallback: non-Colony terminals and pipe usage.

## Test Harness Strategy

### 1) Protocol Fixture Tests (pure)
- Use `colony-protocol` types as the shared truth.
- Golden fixtures for encoding/decoding OSC 2337 messages.
- Enforce backward compatibility: older schema must still parse or be safely ignored.

### 2) Headless Integration Tests (PTY)
- Launch `colony-terminal` in a headless or minimal render mode.
- Spawn `colony-shell` with a test config and predictable prompt state.
- Capture OSC 2337 traffic for assertions.

### 3) UI Event Round-Trip Tests
- Simulate terminal UI actions (accept suggestion, widget click, approval) and verify:
  - Shell receives correct response
  - Shell state changes reflect in subsequent OSC outputs

## Test Matrix

### A. Protocol Conformance
- OSC 2337 framing (BEL/ST terminators, buffering)
- JSON schema parsing for all message types
- Unknown command handling (ignore + log, no crash)
- Version negotiation message (if added)

### B. Prompt Rendering
- Shell emits prompt state, terminal renders
- Cursor position and prompt width changes tracked correctly
- Non-Colony terminal fallback emits ANSI prompt text

### C. Widget Lifecycle
- Approval widget request -> terminal render -> user response -> shell resumes
- Widget state updates on shell-side changes
- Socket IPC (if used) delivers events reliably

### D. Command Recommendations
- Shell emits suggestions on input changes
- Terminal renders ghost text or menu
- Accept/dismiss flows inject correct text into shell stdin

### E. Structured Output
- Shell emits output metadata and structured payloads
- Terminal renders table/chart widgets
- When not in Colony terminal, fallback prints text safely

### F. File Tree (Hybrid Model)
- Shell emits tree state snapshot and incremental updates
- Terminal emits user actions (open, delete, rename)
- Shell applies actions and sends state updates

## Required Fixtures

- Minimal test `.colonyrc` with deterministic prompt and agent settings
- Recorded OSC 2337 messages for baseline comparisons
- Cross-version compatibility fixtures (v0, v1 schemas)

## CI Integration

- Add a lightweight integration test target that runs in CI for both repos.
- For protocol changes, require updating fixtures in both repos.
- Gate releases on conformance tests passing.

## Ownership

- Shell repo owns message semantics and fixtures.
- Terminal repo owns rendering validation and input handling tests.
- `colony-protocol` changes require updating both test suites.
