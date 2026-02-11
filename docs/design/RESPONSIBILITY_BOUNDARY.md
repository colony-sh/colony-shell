# Colony Shell + Colony Terminal Responsibility Boundary

## Purpose

Define a hard split between shell and terminal responsibilities to prevent
feature duplication, regressions, and protocol drift.

## Shell Responsibilities (colony-shell)

- Parse and execute commands (POSIX semantics).
- Own state: prompt state, agent state, tuple space, memory, focus, history.
- Generate structured UI state (widgets, prompt, recommendations).
- Emit OSC 2337 messages and handle terminal responses.
- Provide ANSI/plain-text fallbacks when not in Colony terminal.

## Terminal Responsibilities (colony-terminal)

- Terminal emulation (PTY, escape sequences, scrollback).
- Rendering and UI (GPU rendering, widgets, panes, themes).
- Input handling (keyboard, mouse, selection, clicks).
- Interpret OSC 2337 and render corresponding UI.
- Send user actions back to shell using OSC 2337 or side channel.

## Hard Rules

1. The shell owns **logic and state**; the terminal owns **rendering and input**.
2. The shell must not implement terminal-native UI when Colony terminal is detected.
3. The terminal must not re-implement shell logic (parsing, env, history, agents).
4. All cross-boundary features require protocol definitions and tests.

## Fallback Rules (Non-Colony Terminals)

- Shell must render ANSI prompt text directly.
- Widgets and structured output must degrade to text.
- Recommendations are disabled or use readline/rustyline fallback.
- Markdown rendering is terminal-only; shell should emit plain text fallback when not in colony-terminal.

## Protocol Stability

- `colony-protocol` types are the shared contract.
- Add version negotiation before incompatible changes.
- Unknown OSC 2337 commands must be ignored safely by the terminal.

## Examples

- **Command recommendations**: shell generates suggestions, terminal renders them.
- **File tree** (hybrid): shell owns tree state, terminal renders and sends actions.
- **Approvals**: shell requests, terminal renders dialog, terminal replies.
- **Namespaced builtins**: `context:show` is parsed as `context show` inside the shell.

## Enforcement

- Integration tests cover round-trip flows for all cross-boundary features.
- Changes to OSC 2337 must update fixtures and tests in both repos.
