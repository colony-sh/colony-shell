# File Tree Sidebar Design

Date: 2026-03-09

## Overview

Integrate the file tree sidebar into the TUI REPL as a split persistent pane. The sidebar shows the project file tree with git status indicators. Users toggle it with Ctrl+B, navigate with vim-style keys, and interact with files via hotkeys or a context menu.

## Layout

Split persistent panes — sidebar on the left, shell on the right. The sidebar auto-hides during command execution and reappears at the next prompt (configurable).

- **Off by default** — Ctrl+B toggles visibility
- **Tab** switches focus between sidebar and prompt
- **Focus indicator** — cyan border on focused pane, dark gray on unfocused
- **Sidebar width** — 25 columns (configurable)

## Cursor & Selection State

Cursor state lives in `repl_tui.rs` as a `SidebarState` struct, not in `FileTreePanel` or `TreeState`. This keeps the UI-specific state out of the data model.

```rust
struct SidebarState {
    cursor_index: usize,        // index into visible_nodes()
    context_menu: Option<ContextMenu>,
    show_hidden: bool,
}
```

On each render:
1. `FileTreePanel.render_block()` produces `ComponentLine`s and returns visible nodes
2. `cursor_index` is clamped to `0..visible_nodes.len()` to handle filesystem changes between renders
3. The cursor line gets a highlight style (reverse video or background color)
4. `SidebarRenderer` accepts `selected_index: Option<usize>` and applies highlight

## Navigation (sidebar focused)

| Key | Action |
|-----|--------|
| `↑/↓` or `j/k` | Move cursor up/down |
| `Enter` | Directory: toggle expand/collapse. File: show context menu |
| `l` / `→` | Collapsed dir: expand. Expanded dir: move cursor to first child. File: no-op |
| `h` / `←` | Expanded dir: collapse. Collapsed dir or file: move cursor to parent |
| `i` | Insert file path into prompt, switch focus to prompt |
| `o` | Open file in $EDITOR |
| `p` | Preview: cat first 50 lines into shell output area |
| `.` | Toggle hidden files |
| `Tab` | Switch focus to prompt |
| `Ctrl+B` | Close sidebar |

## Context Menu

When pressing Enter on a file, a context menu renders as a small floating `Paragraph` widget over the sidebar area. Rendered last in the draw loop so it appears on top.

```
┌─────────────────┐
│ i  Insert path   │
│ p  Preview       │
│ o  Open $EDITOR  │
│ y  Copy path     │
│                  │
│ Esc  Cancel      │
└─────────────────┘
```

State:

```rust
struct ContextMenu {
    node_path: PathBuf,
    selected: usize,  // 0-3
}
```

- Arrow keys / j/k move selection within menu
- Enter confirms the highlighted action
- Hotkey letters (`i`, `o`, `p`, `y`) execute immediately
- Escape dismisses the menu

## File Actions

- **Insert path** (`i`): Inserts relative path at cursor in prompt, switches focus to prompt
- **Preview** (`p`): Outputs first 50 lines of the file into the shell output area (like running `head -50`). Focus stays on sidebar for continued browsing.
- **Open in $EDITOR** (`o`): Disables raw mode, runs `$EDITOR <path>` (falls back to `vi`), re-enables raw mode when editor exits.
- **Copy path** (`y`): Copies relative path to system clipboard via `pbcopy` (macOS) or `xclip`/`xsel` (Linux).

## Command Execution Behavior

`repl_tui.rs` already disables raw mode and exits the ratatui draw loop during command execution. The sidebar cannot remain visible during this time since ratatui is not drawing.

**Default behavior**: Sidebar hides during command execution, reappears at next prompt. This matches the existing execution model with no additional complexity.

**Future enhancement** (not in this spec): A PTY-based approach could capture command output and render it within the ratatui split pane, keeping the sidebar visible. This is a significant scope increase.

Configurable via `COLONY_SIDEBAR_HIDE_ON_EXEC`:
- **Default / "true"**: Sidebar hides during execution, reappears at next prompt
- **"false"**: Sidebar stays visible (future PTY-based implementation)

## Directory Change Behavior

When the user runs `cd`, `FileTreePanel` detects the cwd change on next render (compares `current_root` to `ctx.cwd`). The tree reloads from the new directory. Cursor resets to index 0. Expansion state is lost (fresh scan).

## Changes Required

### colony-shell/Cargo.toml
Change `default = []` to `default = ["tui"]` so the TUI REPL is the default. This is a behavior change for existing builds — the shell switches from rustyline to ratatui-based input.

### repl_tui.rs
- Add `SidebarState` struct (cursor, context menu, hidden files toggle)
- Add `SidebarManager` with `FileTreePanel` registration
- Add `Focus` enum (Prompt | Sidebar)
- Add sidebar keyboard handling when sidebar is focused
- Update `draw_prompt_and_input` to include sidebar in layout (horizontal split)
- Add `SidebarRenderer` calls in draw loop, passing `selected_index`
- Handle Ctrl+B toggle, Tab focus switch
- Handle file actions (insert path, preview, open in $EDITOR, copy path)
- Render context menu as floating widget when active

### colony-tui (SidebarRenderer — `sidebar/renderer.rs`)
- Use `sidebar/renderer.rs` (not the older `sidebar_renderer.rs`)
- Already has `focused: bool` parameter with focus-aware border coloring
- Add `selected_index: Option<usize>` parameter
- Apply highlight style (reverse video) to the selected line

### colony-ui-core (FileTreePanel)
- No cursor state changes needed (cursor lives in repl_tui.rs)
- May need `handle_action()` wiring for toggle_expanded, toggle_hidden

## Architecture

```
repl_tui.rs
  ├── SidebarState (cursor_index, context_menu, show_hidden)
  ├── SidebarManager (from colony-ui-core)
  │     └── FileTreePanel
  │           └── TreeState (from colony-filetree)
  │                 ├── FileNode tree
  │                 ├── GitStatusCache
  │                 └── visible_nodes()
  ├── SidebarRenderer (from colony-tui, sidebar/renderer.rs)
  ├── Focus state (Prompt | Sidebar)
  └── InputEditor + PromptRenderer (existing)
```

Data flow:
1. FileTreePanel lazily loads TreeState on first render
2. TreeState scans directory, builds FileNode tree, queries git status
3. SidebarManager calls FileTreePanel.render_block() → ComponentLines
4. repl_tui clamps cursor_index to visible node count
5. SidebarRenderer converts ComponentLines to ratatui widgets, highlights cursor line
6. repl_tui draws sidebar pane alongside prompt/output
7. Keyboard events route to sidebar or prompt based on Focus state
8. On cd, FileTreePanel detects cwd change and reloads tree
