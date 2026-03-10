# Panel Tabs & Stacking

## Goal

When multiple panels share a position, default to tabs (one visible at a time with a tab bar). Optionally stack vertically.

## Layout Modes

| Mode | Behavior |
|------|----------|
| **Tabs** (default) | Tab bar at top showing panel names, `[`/`]` to cycle when focused, only one panel rendered at a time |
| **Stack** | Panels split the space vertically (left/right) or horizontally (top/bottom), all visible |

## Init Script Syntax

```bash
# Tabs (default) — these are equivalent:
sidebar left filetree filedetails
sidebar left --tabs filetree filedetails

# Vertical stack:
sidebar left --stack filetree filedetails

# Single panel (no tabs needed):
sidebar left --hotkey ctrl-b filetree

# Hotkey works with multi-panel too:
sidebar left --hotkey ctrl-b --stack filetree filedetails
```

Multi-panel commands accept multiple program names. Each becomes a panel in the group.

## Tab Bar Rendering

When a position has tabs, a single-line tab bar renders at the top of the panel area:

```
 filetree │ filedetails
┌─────────────────────┐
│ src/                 │
│ tests/               │
│ Cargo.toml           │
└─────────────────────┘
```

Active tab: bold/underline. Inactive tabs: dim.

## Key Bindings (when panel area is focused)

- `[` / `]` — cycle active tab left/right
- `Tab` — still cycles focus between main and panel positions
- Panel-specific keys (j/k/etc.) go to the active tab's panel

## Architecture Changes

### PanelHost: PositionGroup

Each position gains a group concept:

```rust
struct PositionGroup {
    mode: LayoutMode,         // Tabs or Stack
    panels: Vec<PanelId>,     // ordered list
    active_tab: usize,        // index into panels (tabs mode)
}

enum LayoutMode {
    Tabs,
    Stack,
}
```

`PanelHost` stores `HashMap<PanelPosition, PositionGroup>` alongside the existing slots map.

### SidebarCommand Changes

`SidebarCommand::Add` changes to accept multiple programs:

```rust
SidebarCommand::Add {
    position: PanelPosition,
    commands: Vec<String>,       // was: command: String
    args: Vec<Vec<String>>,      // args per command
    layout_mode: Option<LayoutMode>,  // None = Tabs (default)
    hotkey: Option<String>,
    // size constraints apply to the whole group
    weight: u16,
    min_size: Option<u16>,
    max_size: Option<u16>,
    outer: bool,
}
```

### Rendering

- **Tabs mode:** Render tab bar (1 line) at top of panel area, then render only `panels[active_tab]`'s frame below it.
- **Stack mode:** Split the panel rect evenly among all panels, render each in its sub-rect.

### Focus

- Focus targets a position, not an individual panel.
- In tabs mode, keys go to the active tab's panel.
- In stack mode, `[`/`]` cycles which stacked panel receives keys. The focused sub-panel gets highlighted border; others get dim border.

### Tab/Stack Cycling

- `[` — previous tab/panel in group
- `]` — next tab/panel in group
- These keys are intercepted in `read_input_line` when a panel position is focused, before forwarding to the panel process.
