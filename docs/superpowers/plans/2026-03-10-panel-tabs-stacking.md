# Panel Tabs & Stacking Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Support tabbed and stacked panel groups at each position (including main), defaulting to tabs when multiple panels share a position.

**Architecture:** `SidebarCommand::Add` accepts multiple commands. `PanelHost` gains `PositionGroup` to track layout mode, member panels, and active tab. The layout engine gives one rect per position group. The TUI renderer splits that rect into tab bar + active panel (tabs mode) or evenly among all panels (stack mode). `[`/`]` keys cycle the active tab/panel within a focused group. `sidebar main` adds panel tabs to the main area — the shell prompt is always the implicit first tab, panels render in the scrollback region above the prompt, and `[`/`]` cycles tabs when main is focused.

**Tech Stack:** ratatui, colony-panel, colony-exec (sidebar parser)

---

## File Structure

| File | Change | Responsibility |
|------|--------|---------------|
| `crates/colony-exec/src/sidebar.rs` | Modify | Parse multi-command syntax + `--tabs`/`--stack` flags |
| `crates/colony-panel/src/host.rs` | Modify | Add `PositionGroup`, `LayoutMode`, group-aware focus/cycling |
| `crates/colony-panel/src/layout.rs` | Modify | Resolve one rect per position group instead of per panel |
| `crates/colony-panel/src/lib.rs` | Modify | Re-export new types |
| `crates/colony-shell/src/repl_tui.rs` | Modify | Tab bar rendering, `[`/`]` key handling, group-aware panel rendering |

---

## Chunk 1: Sidebar command parsing

### Task 1: Update SidebarCommand to support multiple commands and layout mode

**Files:**
- Modify: `crates/colony-exec/src/sidebar.rs`

- [ ] **Step 1: Add LayoutMode enum and update SidebarCommand::Add**

In `crates/colony-exec/src/sidebar.rs`, add above `SidebarCommand`:

```rust
/// How multiple panels at the same position are arranged.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum LayoutMode {
    /// Show one panel at a time with a tab bar (default).
    #[default]
    Tabs,
    /// Split the space and show all panels simultaneously.
    Stack,
}
```

Change `SidebarCommand::Add` to:

```rust
    Add {
        position: PanelPosition,
        commands: Vec<(String, Vec<String>)>,  // (program, args) per panel
        layout_mode: LayoutMode,
        weight: u16,
        min_size: Option<u16>,
        max_size: Option<u16>,
        outer: bool,
        hotkey: Option<String>,
    },
```

- [ ] **Step 2: Update parse_sidebar_args to collect multiple commands**

Replace the command parsing section (from `if command_start >= args.len()` to the end of the function) with:

```rust
    // Parse --tabs or --stack flag
    let mut layout_mode = LayoutMode::default();
    while i < args.len() {
        match args[i].as_str() {
            "--tabs" => {
                layout_mode = LayoutMode::Tabs;
                i += 1;
                command_start = i;
            }
            "--stack" => {
                layout_mode = LayoutMode::Stack;
                i += 1;
                command_start = i;
            }
            _ => {
                command_start = i;
                break;
            }
        }
    }

    if command_start >= args.len() {
        return Err("sidebar requires at least one command".to_string());
    }

    // Remaining args are all command names (no per-command args in multi-command mode)
    let remaining = &args[command_start..];
    let commands: Vec<(String, Vec<String>)> = remaining
        .iter()
        .map(|cmd| (cmd.clone(), Vec::new()))
        .collect();

    Ok(SidebarCommand::Add {
        position,
        commands,
        layout_mode,
        weight,
        min_size,
        max_size,
        outer,
        hotkey,
    })
```

Note: The option parsing loop already breaks on non-option args, so `--tabs`/`--stack` must come before commands. After them, all remaining words are command names.

- [ ] **Step 3: Update existing tests**

Update tests that match on `SidebarCommand::Add` to use the new `commands` field. For example:

`test_parse_simple`:
```rust
    #[test]
    fn test_parse_simple() {
        let cmd = parse_sidebar_args(&args(&["left", "filetree"])).unwrap();
        match cmd {
            SidebarCommand::Add {
                position, commands, layout_mode, ..
            } => {
                assert_eq!(position, PanelPosition::Left);
                assert_eq!(commands.len(), 1);
                assert_eq!(commands[0].0, "filetree");
                assert_eq!(layout_mode, LayoutMode::Tabs);
            }
            _ => panic!("expected Add"),
        }
    }
```

- [ ] **Step 4: Add tests for multi-command and layout mode parsing**

```rust
    #[test]
    fn test_parse_multi_command_tabs() {
        let cmd = parse_sidebar_args(&args(&["left", "filetree", "filedetails"])).unwrap();
        match cmd {
            SidebarCommand::Add { commands, layout_mode, .. } => {
                assert_eq!(commands.len(), 2);
                assert_eq!(commands[0].0, "filetree");
                assert_eq!(commands[1].0, "filedetails");
                assert_eq!(layout_mode, LayoutMode::Tabs);
            }
            _ => panic!("expected Add"),
        }
    }

    #[test]
    fn test_parse_stack_mode() {
        let cmd = parse_sidebar_args(&args(&["left", "--stack", "filetree", "filedetails"])).unwrap();
        match cmd {
            SidebarCommand::Add { commands, layout_mode, .. } => {
                assert_eq!(commands.len(), 2);
                assert_eq!(layout_mode, LayoutMode::Stack);
            }
            _ => panic!("expected Add"),
        }
    }

    #[test]
    fn test_parse_explicit_tabs_mode() {
        let cmd = parse_sidebar_args(&args(&["left", "--tabs", "filetree", "filedetails"])).unwrap();
        match cmd {
            SidebarCommand::Add { layout_mode, .. } => {
                assert_eq!(layout_mode, LayoutMode::Tabs);
            }
            _ => panic!("expected Add"),
        }
    }

    #[test]
    fn test_parse_hotkey_with_stack() {
        let cmd = parse_sidebar_args(&args(&["left", "--hotkey", "ctrl-b", "--stack", "filetree", "filedetails"])).unwrap();
        match cmd {
            SidebarCommand::Add { hotkey, layout_mode, commands, .. } => {
                assert_eq!(hotkey, Some("ctrl-b".to_string()));
                assert_eq!(layout_mode, LayoutMode::Stack);
                assert_eq!(commands.len(), 2);
            }
            _ => panic!("expected Add"),
        }
    }
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p colony-exec -- sidebar`
Expected: all sidebar tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/colony-exec/src/sidebar.rs
git commit -m "feat(sidebar): support multi-command syntax with --tabs/--stack layout modes"
```

---

## Chunk 2: PanelHost position groups

### Task 2: Add PositionGroup and LayoutMode to PanelHost

**Files:**
- Modify: `crates/colony-panel/src/host.rs`
- Modify: `crates/colony-panel/src/lib.rs`

- [ ] **Step 1: Add PositionGroup and LayoutMode types**

At the top of `crates/colony-panel/src/host.rs`, after the existing imports, add:

```rust
use crate::layout::PanelPosition;
```

After `PanelSpec`, add:

```rust
/// How panels in a position group are arranged.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LayoutMode {
    #[default]
    Tabs,
    Stack,
}

/// A group of panels at the same position.
#[derive(Debug)]
pub struct PositionGroup {
    pub mode: LayoutMode,
    pub panels: Vec<PanelId>,
    pub active_tab: usize,
}

impl PositionGroup {
    fn new(mode: LayoutMode) -> Self {
        Self {
            mode,
            panels: Vec::new(),
            active_tab: 0,
        }
    }

    /// Cycle to the next tab/panel.
    pub fn next_tab(&mut self) {
        if self.panels.is_empty() {
            return;
        }
        self.active_tab = (self.active_tab + 1) % self.panels.len();
    }

    /// Cycle to the previous tab/panel.
    pub fn prev_tab(&mut self) {
        if self.panels.is_empty() {
            return;
        }
        self.active_tab = if self.active_tab == 0 {
            self.panels.len() - 1
        } else {
            self.active_tab - 1
        };
    }

    /// The currently active panel ID.
    pub fn active_panel(&self) -> Option<PanelId> {
        self.panels.get(self.active_tab).copied()
    }

    /// Remove a panel from the group, adjusting active_tab.
    pub fn remove_panel(&mut self, id: PanelId) {
        if let Some(pos) = self.panels.iter().position(|p| *p == id) {
            self.panels.remove(pos);
            if self.active_tab >= self.panels.len() && !self.panels.is_empty() {
                self.active_tab = self.panels.len() - 1;
            }
        }
    }
}
```

- [ ] **Step 2: Add position groups to PanelHost**

Add a field to `PanelHost`:

```rust
    /// Position groups: position → group of panels with layout mode.
    groups: HashMap<PanelPosition, PositionGroup>,
```

Initialize in `new()`:

```rust
            groups: HashMap::new(),
```

- [ ] **Step 3: Add group management methods**

Add these methods to `PanelHost`:

```rust
    /// Add a group of panels at a position.
    pub async fn add_panel_group(
        &mut self,
        specs: Vec<PanelSpec>,
        mode: LayoutMode,
        hotkey: Option<&str>,
    ) -> Result<Vec<PanelId>, ProtocolError> {
        if specs.is_empty() {
            return Ok(Vec::new());
        }
        let position = specs[0].position;
        let group = self.groups.entry(position).or_insert_with(|| PositionGroup::new(mode));
        group.mode = mode;

        let mut ids = Vec::new();
        for spec in &specs {
            let arg_refs: Vec<&str> = spec.args.iter().map(String::as_str).collect();
            let id = self.add_panel(
                &spec.program, &arg_refs, spec.position,
                spec.weight, spec.min_size, spec.max_size, spec.outer,
            ).await?;
            group.panels.push(id);
            ids.push(id);
        }

        if let Some(hk) = hotkey {
            for spec in specs {
                self.hotkey_specs.entry(hk.to_string()).or_default().push(spec);
            }
            self.hotkey_active.entry(hk.to_string()).or_default().extend(ids.iter());
        }

        Ok(ids)
    }

    /// Get the position group for a given position.
    pub fn position_group(&self, position: PanelPosition) -> Option<&PositionGroup> {
        self.groups.get(&position)
    }

    /// Get a mutable reference to the position group for a given position.
    pub fn position_group_mut(&mut self, position: PanelPosition) -> Option<&mut PositionGroup> {
        self.groups.get_mut(&position)
    }

    /// Get the position group that contains the given panel.
    pub fn group_for_panel(&self, id: PanelId) -> Option<(&PanelPosition, &PositionGroup)> {
        self.groups.iter().find(|(_, g)| g.panels.contains(&id))
    }

    /// Get the active panel ID for a focused position.
    /// In tabs mode, returns the active tab. In stack mode, returns the focused sub-panel.
    pub fn active_panel_in_focus(&self) -> Option<PanelId> {
        let Focus::Panel(id) = self.focus else {
            return None;
        };
        // Find which group this panel belongs to
        let (_, group) = self.group_for_panel(id)?;
        group.active_panel()
    }
```

- [ ] **Step 4: Update remove_panel to clean up groups**

In the existing `remove_panel` method, after removing from `self.slots` and `self.layout`, add:

```rust
        // Remove from position group
        for group in self.groups.values_mut() {
            group.remove_panel(id);
        }
        // Clean up empty groups
        self.groups.retain(|_, g| !g.panels.is_empty());
```

- [ ] **Step 5: Update focus to target position groups**

Change `Focus` enum:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Main,
    Panel(PanelId),
}
```

Keep as-is (panel ID is enough — we look up the group when needed). But update `cycle_focus` to cycle through position groups rather than individual panels:

```rust
    pub fn cycle_focus(&mut self) {
        // Collect unique positions that have panels, in stable order
        let mut positions: Vec<PanelPosition> = Vec::new();
        for pos in &[PanelPosition::Left, PanelPosition::Right, PanelPosition::Top, PanelPosition::Bottom] {
            if self.groups.contains_key(pos) {
                positions.push(*pos);
            }
        }
        // Also include ungrouped panels (panels added directly, not via groups)
        let grouped_ids: std::collections::HashSet<PanelId> = self.groups.values()
            .flat_map(|g| g.panels.iter().copied())
            .collect();
        let mut ungrouped: Vec<PanelId> = self.slots.keys()
            .filter(|id| !grouped_ids.contains(id))
            .copied()
            .collect();
        ungrouped.sort_by_key(|id| id.0);

        if positions.is_empty() && ungrouped.is_empty() {
            self.focus = Focus::Main;
            return;
        }

        match self.focus {
            Focus::Main => {
                // Focus first position group's active panel, or first ungrouped
                if let Some(pos) = positions.first() {
                    if let Some(group) = self.groups.get(pos) {
                        if let Some(id) = group.active_panel() {
                            self.focus = Focus::Panel(id);
                            return;
                        }
                    }
                }
                if let Some(id) = ungrouped.first() {
                    self.focus = Focus::Panel(*id);
                }
            }
            Focus::Panel(current) => {
                // Find which position group or ungrouped list the current panel is in
                let current_pos = self.group_for_panel(current).map(|(p, _)| *p);

                if let Some(pos) = current_pos {
                    // Move to next position group
                    let idx = positions.iter().position(|p| *p == pos).unwrap_or(0);
                    if idx + 1 < positions.len() {
                        let next_pos = positions[idx + 1];
                        if let Some(group) = self.groups.get(&next_pos) {
                            if let Some(id) = group.active_panel() {
                                self.focus = Focus::Panel(id);
                                return;
                            }
                        }
                    }
                    // Past all positions → check ungrouped → main
                    if let Some(id) = ungrouped.first() {
                        self.focus = Focus::Panel(*id);
                        return;
                    }
                    self.focus = Focus::Main;
                } else {
                    // Currently on an ungrouped panel
                    let idx = ungrouped.iter().position(|id| *id == current).unwrap_or(0);
                    if idx + 1 < ungrouped.len() {
                        self.focus = Focus::Panel(ungrouped[idx + 1]);
                    } else {
                        self.focus = Focus::Main;
                    }
                }
            }
        }
    }
```

- [ ] **Step 6: Re-export new types in lib.rs**

In `crates/colony-panel/src/lib.rs`, update:

```rust
pub use host::{Focus, LayoutMode, PanelHost, PanelSpec, PositionGroup};
```

- [ ] **Step 7: Verify it compiles**

Run: `cargo check -p colony-panel`
Expected: compiles.

- [ ] **Step 8: Commit**

```bash
git add crates/colony-panel/src/host.rs crates/colony-panel/src/lib.rs
git commit -m "feat(panel): add PositionGroup with tabs/stack layout modes"
```

---

## Chunk 3: TUI rendering and key handling

### Task 3: Update process_sidebar_commands for new SidebarCommand format

**Files:**
- Modify: `crates/colony-shell/src/repl_tui.rs`

- [ ] **Step 1: Update the SidebarCommand::Add handler**

In `process_sidebar_commands`, replace the `SidebarCommand::Add` match arm:

```rust
            SidebarCommand::Add {
                position,
                commands,
                layout_mode,
                weight,
                min_size,
                max_size,
                outer,
                hotkey,
            } => {
                let specs: Vec<colony_panel::PanelSpec> = commands
                    .into_iter()
                    .map(|(program, args)| colony_panel::PanelSpec {
                        program,
                        args,
                        position,
                        weight,
                        min_size: min_size.unwrap_or(15),
                        max_size: max_size.unwrap_or(40),
                        outer,
                    })
                    .collect();

                let mode = match layout_mode {
                    colony_exec::sidebar::LayoutMode::Tabs => colony_panel::LayoutMode::Tabs,
                    colony_exec::sidebar::LayoutMode::Stack => colony_panel::LayoutMode::Stack,
                };

                let result = panel_host
                    .add_panel_group(specs, mode, hotkey.as_deref())
                    .await;
                if let Err(e) = result {
                    scrollback.add_error(&format!("sidebar: {e}"));
                }
            }
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p colony-shell`
Expected: compiles (may have warnings).

- [ ] **Step 3: Commit**

```bash
git add crates/colony-shell/src/repl_tui.rs
git commit -m "feat(tui): update sidebar command handler for multi-command panel groups"
```

---

### Task 4: Tab bar rendering and tab cycling keys

**Files:**
- Modify: `crates/colony-shell/src/repl_tui.rs`

- [ ] **Step 1: Add tab bar rendering to render_panel_frame**

Replace the existing `render_panel_frame` signature and add a new `render_panel_group` function:

```rust
/// Render a group of panels in their position rect.
fn render_panel_group(
    frame: &mut ratatui::Frame,
    area: Rect,
    group: &PositionGroup,
    panel_host: &PanelHost,
    focused: bool,
) {
    match group.mode {
        LayoutMode::Tabs => {
            render_tabbed_panels(frame, area, group, panel_host, focused);
        }
        LayoutMode::Stack => {
            render_stacked_panels(frame, area, group, panel_host, focused);
        }
    }
}

/// Render panels in tab mode: tab bar at top, active panel below.
fn render_tabbed_panels(
    frame: &mut ratatui::Frame,
    area: Rect,
    group: &PositionGroup,
    panel_host: &PanelHost,
    focused: bool,
) {
    if area.height < 3 {
        return;
    }

    // Tab bar takes 1 line
    let tab_bar_area = Rect::new(area.x, area.y, area.width, 1);
    let panel_area = Rect::new(area.x, area.y + 1, area.width, area.height - 1);

    // Render tab bar
    let mut spans = Vec::new();
    for (i, id) in group.panels.iter().enumerate() {
        let name = panel_host.panel_command_name(*id).unwrap_or("?");
        let is_active = i == group.active_tab;
        let style = if is_active {
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        if i > 0 {
            spans.push(Span::styled(" \u{2502} ", Style::default().fg(Color::DarkGray)));
        }
        spans.push(Span::styled(format!(" {} ", name), style));
    }
    let tab_line = Paragraph::new(Line::from(spans))
        .style(Style::default().bg(Color::Black));
    frame.render_widget(tab_line, tab_bar_area);

    // Render active panel
    if let Some(active_id) = group.active_panel() {
        let lines = panel_host.current_frame(active_id);
        render_panel_frame(frame, panel_area, lines, focused);
    }
}

/// Render panels in stack mode: split space evenly.
fn render_stacked_panels(
    frame: &mut ratatui::Frame,
    area: Rect,
    group: &PositionGroup,
    panel_host: &PanelHost,
    focused: bool,
) {
    let n = group.panels.len() as u16;
    if n == 0 || area.height < n {
        return;
    }

    let per_panel = area.height / n;
    let remainder = area.height % n;

    let mut y = area.y;
    for (i, id) in group.panels.iter().enumerate() {
        let h = per_panel + if (i as u16) < remainder { 1 } else { 0 };
        let sub_area = Rect::new(area.x, y, area.width, h);
        let lines = panel_host.current_frame(*id);
        let panel_focused = focused && group.active_tab == i;
        render_panel_frame(frame, sub_area, lines, panel_focused);
        y += h;
    }
}
```

- [ ] **Step 2: Add panel_command_name method to PanelHost**

In `crates/colony-panel/src/host.rs`, add:

```rust
    /// Get the command name for a panel.
    pub fn panel_command_name(&self, id: PanelId) -> Option<&str> {
        self.slots.get(&id).map(|s| s.command_name())
    }
```

- [ ] **Step 3: Update draw_tui to render groups**

In `draw_tui`, replace the panel rendering loop:

```rust
            // Render panels
            for (id, panel_area) in &layout.panels {
                let lines = panel_host.current_frame(*id);
                let panel_focused = focus == Focus::Panel(*id);
                render_panel_frame(frame, *panel_area, lines, panel_focused);
            }
```

With:

```rust
            // Render panels — grouped panels get special rendering
            let grouped_ids: std::collections::HashSet<PanelId> = panel_host.all_grouped_ids();
            for (id, panel_area) in &layout.panels {
                if grouped_ids.contains(id) {
                    // Rendered as part of a group below
                    continue;
                }
                let lines = panel_host.current_frame(*id);
                let panel_focused = focus == Focus::Panel(*id);
                render_panel_frame(frame, *panel_area, lines, panel_focused);
            }

            // Render position groups
            for (position, group) in panel_host.all_groups() {
                // Use the rect of the first panel in the group
                let Some(first_id) = group.panels.first() else { continue };
                let Some(group_area) = layout.panels.get(first_id) else { continue };
                let group_focused = if let Focus::Panel(fid) = focus {
                    group.panels.contains(&fid)
                } else {
                    false
                };
                render_panel_group(frame, *group_area, group, panel_host, group_focused);
            }
```

- [ ] **Step 4: Add helper methods to PanelHost**

In `crates/colony-panel/src/host.rs`:

```rust
    /// Get all panel IDs that belong to a position group.
    pub fn all_grouped_ids(&self) -> std::collections::HashSet<PanelId> {
        self.groups.values().flat_map(|g| g.panels.iter().copied()).collect()
    }

    /// Iterate over all position groups.
    pub fn all_groups(&self) -> impl Iterator<Item = (&PanelPosition, &PositionGroup)> {
        self.groups.iter()
    }
```

- [ ] **Step 5: Add `[`/`]` key handling in read_input_line**

In `read_input_line`, in the panel-focused key handling section (after the Esc check, before forwarding to panel), add:

```rust
                        // Tab cycling within position group: [ and ]
                        if key.code == KeyCode::Char('[') || key.code == KeyCode::Char(']') {
                            if let Focus::Panel(fid) = panel_host.focus {
                                if let Some((pos, _)) = panel_host.group_for_panel(fid) {
                                    let pos = *pos;
                                    if let Some(group) = panel_host.position_group_mut(pos) {
                                        if key.code == KeyCode::Char(']') {
                                            group.next_tab();
                                        } else {
                                            group.prev_tab();
                                        }
                                        // Update focus to the new active panel
                                        if let Some(new_id) = group.active_panel() {
                                            panel_host.focus = Focus::Panel(new_id);
                                        }
                                    }
                                }
                            }
                            redraw(
                                terminal, prompt_manager, ctx, editor,
                                panel_host, scrollback, status_message, *show_help,
                            )?;
                            continue;
                        }
```

- [ ] **Step 6: Update send_to_focused for tabs mode**

In `PanelHost::send_to_focused`, ensure it sends to the active tab's panel, not just whatever `Focus::Panel(id)` says. Since we update `Focus::Panel` to point at the active tab whenever we cycle, the existing code should work. But verify the flow: when tab cycles via `[`/`]`, we update `panel_host.focus` to the new active panel, so `send_to_focused` naturally sends to the right panel.

No code change needed — just verify the logic is correct.

- [ ] **Step 7: Verify it compiles**

Run: `cargo check -p colony-shell`
Expected: compiles.

- [ ] **Step 8: Commit**

```bash
git add crates/colony-panel/src/host.rs crates/colony-panel/src/lib.rs crates/colony-shell/src/repl_tui.rs
git commit -m "feat(tui): tab bar rendering, stacked panels, and [/] cycling keys"
```

---

### Task 5: Update layout engine for grouped panels

**Files:**
- Modify: `crates/colony-panel/src/layout.rs`

Currently the layout engine gives each panel its own rect. For grouped panels in tabs mode, only the first panel's rect is used (the group occupies one rect). For stacked panels, all get individual rects but they should be adjacent.

The simplest approach: keep the layout engine as-is (it already stacks left panels horizontally). For tabs mode, only register the first panel in the layout engine; the rest share its rect. For stack mode within a position, register all panels and let the existing layout engine give them individual rects.

- [ ] **Step 1: Update add_panel_group to register only one panel for tabs**

In `PanelHost::add_panel_group`, for tabs mode, only add the first panel to the layout engine. The others still get spawned but don't get layout entries:

After spawning each panel with `self.add_panel(...)`, for tabs mode with index > 0, remove the panel from the layout engine:

```rust
        for (i, spec) in specs.iter().enumerate() {
            let arg_refs: Vec<&str> = spec.args.iter().map(String::as_str).collect();
            let id = self.add_panel(
                &spec.program, &arg_refs, spec.position,
                spec.weight, spec.min_size, spec.max_size, spec.outer,
            ).await?;
            group.panels.push(id);
            ids.push(id);

            // In tabs mode, only the first panel occupies layout space
            if mode == LayoutMode::Tabs && i > 0 {
                self.layout.remove_panel(id);
            }
        }
```

- [ ] **Step 2: Verify it compiles and run tests**

Run: `cargo test -p colony-panel`
Expected: existing tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/colony-panel/src/host.rs crates/colony-panel/src/layout.rs
git commit -m "feat(panel): layout engine integration for tabbed panel groups"
```

---

### Task 6: Integration test

- [ ] **Step 1: Full build**

Run: `cargo build -p colony-shell`
Expected: compiles cleanly.

- [ ] **Step 2: Run all tests**

Run: `cargo test --workspace`
Expected: all pass (may have pre-existing warnings).

- [ ] **Step 3: Manual test**

Update `~/.colony/init` to:
```
sidebar left --hotkey ctrl-b --stack filetree filedetails
```

Launch colony, press ctrl-b, verify:
- Both filetree and filedetails render stacked vertically in the left panel
- Tab focuses the position group
- `[`/`]` cycles which stacked panel is focused (highlighted border)

Then change to tabs:
```
sidebar left --hotkey ctrl-b filetree filedetails
```

Verify:
- Tab bar shows "filetree | filedetails" at top
- Only active tab renders below
- `[`/`]` switches tabs

- [ ] **Step 4: Commit any fixups**

```bash
git add -A
git commit -m "fix(panel): integration fixups for tabs and stacking"
```
