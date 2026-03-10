# File Tree Sidebar Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Integrate the file tree sidebar into the TUI REPL as a split persistent pane with vim-style navigation, toggled via Ctrl+B.

**Architecture:** Wire `SidebarManager` and `SidebarRenderer` (both already exist) into `repl_tui.rs`. Add `SidebarState` for cursor tracking and context menu. Enable `tui` feature by default so the ratatui-based REPL is the standard shell experience.

**Tech Stack:** Rust, ratatui 0.29, crossterm 0.28, colony-filetree, colony-ui-core, colony-tui

**Spec:** `docs/superpowers/specs/2026-03-09-file-tree-sidebar-design.md`

---

## File Structure

| File | Action | Responsibility |
|------|--------|----------------|
| `crates/colony-shell/Cargo.toml` | Modify | Enable `tui` feature by default |
| `crates/colony-tui/src/sidebar/renderer.rs` | Modify | Add `selected_index` parameter for cursor highlighting |
| `crates/colony-shell/src/repl_tui.rs` | Modify | Add sidebar layout, focus management, keyboard routing, file actions |

---

## Chunk 1: SidebarRenderer cursor highlighting

### Task 1: Add selected_index to SidebarRenderer

**Files:**
- Modify: `crates/colony-tui/src/sidebar/renderer.rs:17-56` (render method)
- Test: `crates/colony-tui/src/sidebar/renderer.rs:229-248` (existing tests)

- [ ] **Step 1: Write failing test for cursor highlighting**

Add to `crates/colony-tui/src/sidebar/renderer.rs` in the `tests` module:

```rust
#[test]
fn test_component_line_highlight() {
    let line = ComponentLine::new(vec![StyledSpan::plain("selected")]);
    let normal = SidebarRenderer::component_line_to_ratatui(&line, 80, false);
    let highlighted = SidebarRenderer::component_line_to_ratatui(&line, 80, true);

    // Highlighted line should have reversed style on its spans
    let has_reverse = highlighted.spans.iter().any(|s| {
        s.style.add_modifier.contains(Modifier::REVERSED)
    });
    assert!(has_reverse, "highlighted line should have REVERSED modifier");

    // Normal line should not
    let has_reverse_normal = normal.spans.iter().any(|s| {
        s.style.add_modifier.contains(Modifier::REVERSED)
    });
    assert!(!has_reverse_normal, "normal line should not have REVERSED modifier");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p colony-tui -- test_component_line_highlight`
Expected: FAIL — `component_line_to_ratatui` takes 2 args, not 3.

- [ ] **Step 3: Add `highlighted` parameter to `component_line_to_ratatui`**

In `crates/colony-tui/src/sidebar/renderer.rs`, change the signature of `component_line_to_ratatui` (line 120):

```rust
fn component_line_to_ratatui(line: &ComponentLine, max_width: usize, highlighted: bool) -> Line<'static> {
```

At the end of the function, just before the `Line::from(spans)` return (line 179), wrap in highlight logic:

```rust
    if highlighted {
        let spans = spans
            .into_iter()
            .map(|s| {
                let style = s.style.add_modifier(Modifier::REVERSED);
                Span::styled(s.content, style)
            })
            .collect::<Vec<_>>();
        return Line::from(spans);
    }

    Line::from(spans)
```

Also update the truncation return (around line 176) to apply highlight there too:

```rust
        if highlighted {
            truncated = truncated
                .into_iter()
                .map(|s| {
                    let style = s.style.add_modifier(Modifier::REVERSED);
                    Span::styled(s.content, style)
                })
                .collect();
        }
        return Line::from(truncated);
```

- [ ] **Step 4: Update `render_panel_content` to accept and pass `selected_index`**

Change `render_panel_content` (line 108):

```rust
fn render_panel_content(frame: &mut Frame, area: Rect, output: &SidebarOutput, selected_index: Option<usize>) {
    let lines: Vec<Line> = output
        .lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let highlighted = selected_index == Some(i);
            Self::component_line_to_ratatui(line, area.width as usize, highlighted)
        })
        .collect();

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);
}
```

- [ ] **Step 5: Update `render_panel` to pass `selected_index`**

Change `render_panel` (line 69) to accept `selected_index: Option<usize>`:

```rust
fn render_panel(frame: &mut Frame, area: Rect, panel: &SidebarPanelData, selected_index: Option<usize>) {
```

And update the call to `render_panel_content` (line 103):

```rust
Self::render_panel_content(frame, content_area, &panel.output, selected_index);
```

- [ ] **Step 6: Update public `render` method to accept `selected_index`**

Change `render` (line 19):

```rust
pub fn render(frame: &mut Frame, area: Rect, data: &SidebarData, focused: bool, selected_index: Option<usize>) {
```

Pass `selected_index` only to the first panel (the file tree — index 0):

```rust
for (panel_idx, panel) in data.panels.iter().enumerate() {
    // ...
    let panel_selected = if panel_idx == 0 && focused {
        selected_index
    } else {
        None
    };
    Self::render_panel(frame, panel_area, panel, panel_selected);
    y_offset += panel_height;
}
```

- [ ] **Step 7: Fix existing tests and callers**

Update test calls in the same file that call `component_line_to_ratatui` with 2 args to use 3:

```rust
let ratatui_line = SidebarRenderer::component_line_to_ratatui(&line, 80, false);
```

Update any callers in `crates/colony-tui/src/app.rs` that call `SidebarRenderer::render` — add `None` as the last argument:

```rust
SidebarRenderer::render(frame, area, &left_sidebar, focus == Focus::LeftSidebar, None);
SidebarRenderer::render(frame, area, &right_sidebar, focus == Focus::RightSidebar, None);
```

- [ ] **Step 8: Run all tests**

Run: `cargo test -p colony-tui`
Expected: All tests pass.

- [ ] **Step 9: Commit**

```bash
git add crates/colony-tui/src/sidebar/renderer.rs crates/colony-tui/src/app.rs
git commit -m "feat(sidebar): add cursor highlighting to SidebarRenderer"
```

---

## Chunk 2: Integrate sidebar into repl_tui.rs

### Task 2: Add SidebarState and Focus types

**Files:**
- Modify: `crates/colony-shell/src/repl_tui.rs:1-37` (imports and types)

- [ ] **Step 1: Add imports and types to repl_tui.rs**

Add to the imports section at the top of `repl_tui.rs`:

```rust
use colony_tui::SidebarRenderer;
use colony_ui_core::{
    ComponentContext, FileTreePanel, SidebarManager, SidebarPosition, PromptManager,
};
use ratatui::layout::{Constraint, Direction, Layout};
```

(Remove duplicates from existing imports — `ComponentContext` and `PromptManager` are already imported.)

Add these types after the `is_colony_terminal()` function (after line 36):

```rust
/// Focus area in the TUI REPL
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Prompt,
    Sidebar,
}

/// Context menu for file actions
#[derive(Debug, Clone)]
struct ContextMenu {
    node_path: PathBuf,
    selected: usize, // 0=insert, 1=preview, 2=open, 3=copy
}

impl ContextMenu {
    fn new(path: PathBuf) -> Self {
        Self {
            node_path: path,
            selected: 0,
        }
    }

    fn labels() -> &'static [&'static str] {
        &["i  Insert path", "p  Preview", "o  Open $EDITOR", "y  Copy path"]
    }

    fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    fn move_down(&mut self) {
        if self.selected < 3 {
            self.selected += 1;
        }
    }
}

/// Sidebar UI state (cursor, context menu, visibility)
struct SidebarState {
    visible: bool,
    cursor_index: usize,
    context_menu: Option<ContextMenu>,
    show_hidden: bool,
    node_count: usize, // cached from last render
}

impl SidebarState {
    fn new() -> Self {
        Self {
            visible: false,
            cursor_index: 0,
            context_menu: None,
            show_hidden: false,
            node_count: 0,
        }
    }

    fn clamp_cursor(&mut self) {
        if self.node_count == 0 {
            self.cursor_index = 0;
        } else if self.cursor_index >= self.node_count {
            self.cursor_index = self.node_count - 1;
        }
    }

    fn cursor_up(&mut self) {
        self.cursor_index = self.cursor_index.saturating_sub(1);
    }

    fn cursor_down(&mut self) {
        if self.node_count > 0 && self.cursor_index < self.node_count - 1 {
            self.cursor_index += 1;
        }
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p colony-shell --features tui`
Expected: Compiles (types are defined but unused — warnings OK).

- [ ] **Step 3: Commit**

```bash
git add crates/colony-shell/src/repl_tui.rs
git commit -m "feat(sidebar): add SidebarState, Focus, ContextMenu types to repl_tui"
```

### Task 3: Wire SidebarManager into the REPL loop

**Files:**
- Modify: `crates/colony-shell/src/repl_tui.rs:64-216` (run_repl_tui function)

- [ ] **Step 1: Add sidebar state to `run_repl_tui`**

After `let mut prompt_manager = PromptManager::default();` (line 74), add:

```rust
    let mut sidebars = SidebarManager::new();
    sidebars.register(Arc::new(FileTreePanel::new()));
    let mut sidebar_state = SidebarState::new();
    let mut focus = Focus::Prompt;
```

Add `use std::sync::Arc;` to imports if not already present.

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p colony-shell --features tui`
Expected: Compiles with warnings about unused variables.

- [ ] **Step 3: Commit**

```bash
git add crates/colony-shell/src/repl_tui.rs
git commit -m "feat(sidebar): wire SidebarManager into repl_tui loop"
```

### Task 4: Add sidebar to the draw layout

**Files:**
- Modify: `crates/colony-shell/src/repl_tui.rs:218-298` (draw_prompt_and_input function)

- [ ] **Step 1: Update `draw_prompt_and_input` signature**

Change the function signature to accept sidebar data:

```rust
fn draw_prompt_and_input(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    prompt_manager: &PromptManager,
    ctx: &ComponentContext,
    editor: &InputEditor,
    sidebar_data: Option<&colony_ui_core::SidebarData>,
    sidebar_state: &SidebarState,
    focus: Focus,
) -> Result<()> {
```

- [ ] **Step 2: Add horizontal split layout for sidebar**

Replace the terminal draw closure body (inside `terminal.draw(|frame| { ... })`) with:

```rust
    let area = frame.area();

    // Horizontal split: sidebar (if visible) | main
    let (sidebar_area, main_area) = if let Some(sb_data) = sidebar_data {
        if sb_data.visible && area.width > sb_data.width + 10 {
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(sb_data.width),
                    Constraint::Min(10),
                ])
                .split(area);
            (Some(chunks[0]), chunks[1])
        } else {
            (None, area)
        }
    } else {
        (None, area)
    };

    // Render sidebar if visible
    if let (Some(sb_area), Some(sb_data)) = (sidebar_area, sidebar_data) {
        let selected = if focus == Focus::Sidebar {
            Some(sidebar_state.cursor_index)
        } else {
            None
        };
        SidebarRenderer::render(
            frame,
            sb_area,
            sb_data,
            focus == Focus::Sidebar,
            selected,
        );
    }

    // Main layout: prompt+input area + status line
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),    // prompt + input area
            Constraint::Length(1), // status line
        ])
        .split(main_area);

    // ... rest of prompt/input/status rendering uses main_chunks instead of chunks ...
```

The existing prompt + input + status rendering code that uses `chunks[0]` and `chunks[1]` should now use `main_chunks[0]` and `main_chunks[1]`.

- [ ] **Step 3: Update all call sites**

Every call to `draw_prompt_and_input` needs the new parameters. There are calls in:
- `run_repl_tui` (line 96) — the initial draw
- `read_input_line` (multiple calls inside the event loop)

For the initial draw in `run_repl_tui`, before the call, render sidebar data:

```rust
    let sidebar_data = if sidebar_state.visible {
        sidebars.invalidate_all();
        let data = sidebars.render(SidebarPosition::Left, &ctx);
        sidebar_state.node_count = data.panels.first().map_or(0, |p| p.output.lines.len());
        sidebar_state.clamp_cursor();
        Some(data)
    } else {
        None
    };
    draw_prompt_and_input(
        &mut guard.terminal, &prompt_manager, &ctx, &editor,
        sidebar_data.as_ref(), &sidebar_state, focus,
    )?;
```

For `read_input_line`, add the same parameters to its signature and pass through:

```rust
async fn read_input_line(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    editor: &mut InputEditor,
    history: &mut EditorHistory,
    prompt_manager: &PromptManager,
    ctx: &ComponentContext,
    _executor: &Executor,
    sidebars: &mut SidebarManager,
    sidebar_state: &mut SidebarState,
    focus: &mut Focus,
) -> Result<Option<String>> {
```

Inside `read_input_line`, every call to `draw_prompt_and_input` becomes:

```rust
    let sidebar_data = if sidebar_state.visible {
        sidebars.invalidate_all();
        let data = sidebars.render(SidebarPosition::Left, ctx);
        sidebar_state.node_count = data.panels.first().map_or(0, |p| p.output.lines.len());
        sidebar_state.clamp_cursor();
        Some(data)
    } else {
        None
    };
    draw_prompt_and_input(terminal, prompt_manager, ctx, editor, sidebar_data.as_ref(), sidebar_state, *focus)?;
```

Extract this into a helper to avoid repetition:

```rust
fn render_sidebar_data(
    sidebars: &mut SidebarManager,
    sidebar_state: &mut SidebarState,
    ctx: &ComponentContext,
) -> Option<colony_ui_core::SidebarData> {
    if sidebar_state.visible {
        sidebars.invalidate_all();
        let data = sidebars.render(SidebarPosition::Left, ctx);
        sidebar_state.node_count = data.panels.first().map_or(0, |p| p.output.lines.len());
        sidebar_state.clamp_cursor();
        Some(data)
    } else {
        None
    }
}
```

- [ ] **Step 4: Run to verify it compiles and renders**

Run: `cargo check -p colony-shell --features tui`
Expected: Compiles.

- [ ] **Step 5: Commit**

```bash
git add crates/colony-shell/src/repl_tui.rs
git commit -m "feat(sidebar): add sidebar to repl_tui draw layout"
```

### Task 5: Add keyboard routing — Ctrl+B toggle and Tab focus

**Files:**
- Modify: `crates/colony-shell/src/repl_tui.rs` (read_input_line function, key handling)

- [ ] **Step 1: Add Ctrl+B and Tab handling before existing key dispatch**

In `read_input_line`, inside the `Event::Key(key)` match arm, before the existing Ctrl+D check, add:

```rust
    // Global: Ctrl+B toggles sidebar
    if matches!((key.modifiers, key.code), (KeyModifiers::CONTROL, KeyCode::Char('b'))) {
        sidebar_state.visible = !sidebar_state.visible;
        // Sync visibility to SidebarManager so SidebarData.visible matches
        sidebars.set_visible(SidebarPosition::Left, sidebar_state.visible);
        if !sidebar_state.visible {
            *focus = Focus::Prompt;
            sidebar_state.context_menu = None;
        }
        let sb = render_sidebar_data(sidebars, sidebar_state, ctx);
        draw_prompt_and_input(terminal, prompt_manager, ctx, editor, sb.as_ref(), sidebar_state, *focus)?;
        continue;
    }

    // Tab switches focus when sidebar is visible
    if key.code == KeyCode::Tab && sidebar_state.visible {
        *focus = match focus {
            Focus::Prompt => Focus::Sidebar,
            Focus::Sidebar => {
                sidebar_state.context_menu = None;
                Focus::Prompt
            }
        };
        let sb = render_sidebar_data(sidebars, sidebar_state, ctx);
        draw_prompt_and_input(terminal, prompt_manager, ctx, editor, sb.as_ref(), sidebar_state, *focus)?;
        continue;
    }
```

- [ ] **Step 2: Route keys based on focus**

Wrap the existing key handling (Ctrl+D, editor.handle_event, etc.) in a focus check:

```rust
    if *focus == Focus::Sidebar {
        // Sidebar keyboard handling — returns SidebarAction
        let _action = handle_sidebar_key(key, sidebar_state, sidebars, ctx);
        // Task 7 will add action processing here
        let sb = render_sidebar_data(sidebars, sidebar_state, ctx);
        draw_prompt_and_input(terminal, prompt_manager, ctx, editor, sb.as_ref(), sidebar_state, *focus)?;
        continue;
    }

    // Existing prompt key handling below...
```

Add a stub for `handle_sidebar_key` and `SidebarAction` (Task 6 will expand these):

```rust
/// Result of a sidebar key press that needs action from the caller
enum SidebarAction {
    None,
    InsertPath(PathBuf),
    Preview(PathBuf),
    OpenEditor(PathBuf),
    CopyPath(PathBuf),
}

fn handle_sidebar_key(
    key: crossterm::event::KeyEvent,
    sidebar_state: &mut SidebarState,
    _sidebars: &mut SidebarManager,
    _ctx: &ComponentContext,
) -> SidebarAction {
    use crossterm::event::KeyCode;
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => sidebar_state.cursor_up(),
        KeyCode::Down | KeyCode::Char('j') => sidebar_state.cursor_down(),
        _ => {}
    }
    SidebarAction::None
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p colony-shell --features tui`

- [ ] **Step 4: Commit**

```bash
git add crates/colony-shell/src/repl_tui.rs
git commit -m "feat(sidebar): add Ctrl+B toggle, Tab focus, j/k navigation"
```

---

## Chunk 3: Sidebar navigation and file actions

### Task 6: Full sidebar keyboard handling

**Files:**
- Modify: `crates/colony-shell/src/repl_tui.rs` (handle_sidebar_key function)

- [ ] **Step 1: Implement full keyboard navigation**

Replace the `handle_sidebar_key` stub with the complete implementation:

```rust
/// Result of a sidebar key press that needs action from the caller
enum SidebarAction {
    None,
    InsertPath(PathBuf),
    Preview(PathBuf),
    OpenEditor(PathBuf),
    CopyPath(PathBuf),
}

fn handle_sidebar_key(
    key: crossterm::event::KeyEvent,
    sidebar_state: &mut SidebarState,
    sidebars: &mut SidebarManager,
    ctx: &ComponentContext,
) -> SidebarAction {
    use crossterm::event::KeyCode;

    // If context menu is open, handle menu keys
    if let Some(ref mut menu) = sidebar_state.context_menu {
        return match key.code {
            KeyCode::Up | KeyCode::Char('k') => { menu.move_up(); SidebarAction::None }
            KeyCode::Down | KeyCode::Char('j') => { menu.move_down(); SidebarAction::None }
            KeyCode::Enter => {
                let path = menu.node_path.clone();
                let action = match menu.selected {
                    0 => SidebarAction::InsertPath(path),
                    1 => SidebarAction::Preview(path),
                    2 => SidebarAction::OpenEditor(path),
                    3 => SidebarAction::CopyPath(path),
                    _ => SidebarAction::None,
                };
                sidebar_state.context_menu = None;
                action
            }
            KeyCode::Char('i') => {
                let path = menu.node_path.clone();
                sidebar_state.context_menu = None;
                SidebarAction::InsertPath(path)
            }
            KeyCode::Char('p') => {
                let path = menu.node_path.clone();
                sidebar_state.context_menu = None;
                SidebarAction::Preview(path)
            }
            KeyCode::Char('o') => {
                let path = menu.node_path.clone();
                sidebar_state.context_menu = None;
                SidebarAction::OpenEditor(path)
            }
            KeyCode::Char('y') => {
                let path = menu.node_path.clone();
                sidebar_state.context_menu = None;
                SidebarAction::CopyPath(path)
            }
            KeyCode::Esc => { sidebar_state.context_menu = None; SidebarAction::None }
            _ => SidebarAction::None,
        };
    }

    // Normal sidebar navigation
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            sidebar_state.cursor_up();
            SidebarAction::None
        }
        KeyCode::Down | KeyCode::Char('j') => {
            sidebar_state.cursor_down();
            SidebarAction::None
        }
        KeyCode::Enter => {
            // Get the node at cursor — need visible_nodes from the panel
            if let Some(path) = get_node_path_at_cursor(sidebars, sidebar_state, ctx) {
                if path.is_dir() {
                    sidebars.handle_action("file_tree", &format!("toggle:{}", path.display()), ctx);
                    sidebars.invalidate_all();
                } else {
                    sidebar_state.context_menu = Some(ContextMenu::new(path));
                }
            }
            SidebarAction::None
        }
        KeyCode::Right | KeyCode::Char('l') => {
            if let Some(path) = get_node_path_at_cursor(sidebars, sidebar_state, ctx) {
                if path.is_dir() {
                    let expanded = is_node_expanded_at_cursor(sidebars, sidebar_state, ctx);
                    if !expanded {
                        // Collapsed dir: expand it
                        sidebars.handle_action("file_tree", &format!("toggle:{}", path.display()), ctx);
                        sidebars.invalidate_all();
                    } else {
                        // Expanded dir: move cursor to first child
                        sidebar_state.cursor_down();
                    }
                }
                // File: no-op
            }
            SidebarAction::None
        }
        KeyCode::Left | KeyCode::Char('h') => {
            if let Some(path) = get_node_path_at_cursor(sidebars, sidebar_state, ctx) {
                let expanded = is_node_expanded_at_cursor(sidebars, sidebar_state, ctx);
                if path.is_dir() && expanded {
                    // Expanded dir: collapse it
                    sidebars.handle_action("file_tree", &format!("toggle:{}", path.display()), ctx);
                    sidebars.invalidate_all();
                } else {
                    // Collapsed dir or file: move cursor to parent
                    // Find parent by scanning visible nodes for a dir at depth-1
                    // Simple approach: move cursor up until we find a directory
                    if sidebar_state.cursor_index > 0 {
                        sidebar_state.cursor_up();
                    }
                }
            }
            SidebarAction::None
        }
        KeyCode::Char('i') => {
            if let Some(path) = get_node_path_at_cursor(sidebars, sidebar_state, ctx) {
                if !path.is_dir() {
                    return SidebarAction::InsertPath(path);
                }
            }
            SidebarAction::None
        }
        KeyCode::Char('o') => {
            if let Some(path) = get_node_path_at_cursor(sidebars, sidebar_state, ctx) {
                if !path.is_dir() {
                    return SidebarAction::OpenEditor(path);
                }
            }
            SidebarAction::None
        }
        KeyCode::Char('p') => {
            if let Some(path) = get_node_path_at_cursor(sidebars, sidebar_state, ctx) {
                if !path.is_dir() {
                    return SidebarAction::Preview(path);
                }
            }
            SidebarAction::None
        }
        KeyCode::Char('.') => {
            sidebar_state.show_hidden = !sidebar_state.show_hidden;
            // TODO: pass show_hidden to FileTreePanel config
            sidebars.invalidate_all();
            SidebarAction::None
        }
        _ => SidebarAction::None,
    }
}
```

- [ ] **Step 2: Add `get_node_path_at_cursor` helper**

This function gets the file path for the node at the current cursor position. Since `SidebarOutput` contains `ComponentLine`s with interactive config that stores the path, we can extract it:

```rust
fn get_node_path_at_cursor(
    sidebars: &mut SidebarManager,
    sidebar_state: &SidebarState,
    ctx: &ComponentContext,
) -> Option<PathBuf> {
    let data = sidebars.render(SidebarPosition::Left, ctx);
    let panel = data.panels.first()?;
    let line = panel.output.lines.get(sidebar_state.cursor_index)?;
    // InteractiveConfig fields are `click` and `hover` (not action/tooltip)
    let interactive = line.interactive.as_ref()?;
    let path_str = interactive.hover.as_ref().unwrap_or(&interactive.click);
    // Strip "open:" prefix if present
    let path_str = path_str.strip_prefix("open:").unwrap_or(path_str);
    Some(PathBuf::from(path_str))
}

/// Check if the node at cursor is an expanded directory
fn is_node_expanded_at_cursor(
    sidebars: &mut SidebarManager,
    sidebar_state: &SidebarState,
    ctx: &ComponentContext,
) -> bool {
    let data = sidebars.render(SidebarPosition::Left, ctx);
    let Some(panel) = data.panels.first() else { return false };
    let Some(line) = panel.output.lines.get(sidebar_state.cursor_index) else { return false };
    line.expandable && line.expanded
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p colony-shell --features tui`

- [ ] **Step 4: Commit**

```bash
git add crates/colony-shell/src/repl_tui.rs
git commit -m "feat(sidebar): full keyboard navigation with vim keys and context menu"
```

### Task 7: Implement file actions

**Files:**
- Modify: `crates/colony-shell/src/repl_tui.rs` (read_input_line function)

- [ ] **Step 1: Handle SidebarAction results in the key loop**

In `read_input_line`, replace the sidebar key handling block with action processing:

```rust
    if *focus == Focus::Sidebar {
        let action = handle_sidebar_key(key, sidebar_state, sidebars, ctx);
        match action {
            SidebarAction::InsertPath(path) => {
                // Insert relative path into editor and switch to prompt
                let relative = path.strip_prefix(&ctx.cwd).unwrap_or(&path);
                let path_str = relative.display().to_string();
                let current = editor.text();
                editor.set_text(&format!("{}{}", current, path_str));
                *focus = Focus::Prompt;
            }
            SidebarAction::Preview(path) => {
                // Return a cat command to preview the file
                return Ok(Some(format!("head -50 {}", shell_escape(&path))));
            }
            SidebarAction::OpenEditor(path) => {
                // Return $EDITOR command
                let editor_cmd = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
                return Ok(Some(format!("{} {}", editor_cmd, shell_escape(&path))));
            }
            SidebarAction::CopyPath(path) => {
                let relative = path.strip_prefix(&ctx.cwd).unwrap_or(&path);
                let path_str = relative.display().to_string();
                // macOS: pbcopy, Linux: xclip
                if cfg!(target_os = "macos") {
                    let _ = std::process::Command::new("pbcopy")
                        .stdin(std::process::Stdio::piped())
                        .spawn()
                        .and_then(|mut child| {
                            use std::io::Write;
                            if let Some(ref mut stdin) = child.stdin {
                                stdin.write_all(path_str.as_bytes())?;
                            }
                            child.wait()
                        });
                }
            }
            SidebarAction::None => {}
        }
        let sb = render_sidebar_data(sidebars, sidebar_state, ctx);
        draw_prompt_and_input(terminal, prompt_manager, ctx, editor, sb.as_ref(), sidebar_state, *focus)?;
        continue;
    }
```

- [ ] **Step 2: Add `Path` import and shell_escape helper**

Add to imports: `use std::path::{Path, PathBuf};` (replacing the existing `use std::path::PathBuf;`).

```rust
fn shell_escape(path: &Path) -> String {
    let s = path.display().to_string();
    if s.contains(' ') || s.contains('\'') || s.contains('"') {
        format!("'{}'", s.replace('\'', "'\\''"))
    } else {
        s
    }
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p colony-shell --features tui`

- [ ] **Step 4: Commit**

```bash
git add crates/colony-shell/src/repl_tui.rs
git commit -m "feat(sidebar): implement file actions (insert, preview, open, copy)"
```

### Task 7b: Reset cursor on directory change

**Files:**
- Modify: `crates/colony-shell/src/repl_tui.rs` (main loop in run_repl_tui)

- [ ] **Step 1: Track cwd and reset cursor when it changes**

In `run_repl_tui`, after command execution completes (after `execute_shell_command` and before re-enabling raw mode), add:

```rust
    // Reset sidebar cursor if cwd changed (e.g., after cd)
    let new_cwd = std::env::current_dir().unwrap_or_default();
    if new_cwd != ctx.cwd {
        sidebar_state.cursor_index = 0;
        sidebars.invalidate_all();
    }
```

- [ ] **Step 2: Commit**

```bash
git add crates/colony-shell/src/repl_tui.rs
git commit -m "feat(sidebar): reset cursor on directory change"
```

### Task 8: Render context menu overlay

**Files:**
- Modify: `crates/colony-shell/src/repl_tui.rs` (draw_prompt_and_input)

- [ ] **Step 1: Add context menu rendering after sidebar render**

In `draw_prompt_and_input`, after the sidebar render call and before the main area rendering, add:

```rust
    // Render context menu overlay if active
    if let Some(ref menu) = sidebar_state.context_menu {
        if let Some(sb_area) = sidebar_area {
            let menu_width = 20u16.min(sb_area.width.saturating_sub(2));
            let menu_height = 6u16; // 4 options + border
            let menu_x = sb_area.x + 1;
            let menu_y = sb_area.y + sidebar_state.cursor_index as u16 + 2; // below cursor
            let menu_y = menu_y.min(sb_area.y + sb_area.height.saturating_sub(menu_height));

            let menu_area = Rect {
                x: menu_x,
                y: menu_y,
                width: menu_width,
                height: menu_height,
            };

            let menu_lines: Vec<Line> = ContextMenu::labels()
                .iter()
                .enumerate()
                .map(|(i, label)| {
                    let style = if i == menu.selected {
                        Style::default().fg(Color::Black).bg(Color::Cyan)
                    } else {
                        Style::default().fg(Color::White)
                    };
                    Line::from(Span::styled(format!(" {} ", label), style))
                })
                .collect();

            // Clear area first with a background block
            let menu_block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .style(Style::default().bg(Color::Rgb(30, 30, 50)));
            let inner = menu_block.inner(menu_area);
            frame.render_widget(menu_block, menu_area);
            frame.render_widget(Paragraph::new(menu_lines), inner);
        }
    }
```

Add to imports at top of file if not present:

```rust
use ratatui::widgets::{Block, Borders};
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p colony-shell --features tui`

- [ ] **Step 3: Commit**

```bash
git add crates/colony-shell/src/repl_tui.rs
git commit -m "feat(sidebar): render context menu overlay"
```

---

## Chunk 4: Enable TUI by default and test

### Task 9: Enable tui feature by default

**Files:**
- Modify: `crates/colony-shell/Cargo.toml:55-57`

- [ ] **Step 1: Change default features**

In `crates/colony-shell/Cargo.toml`, change:

```toml
[features]
default = ["tui"]
tui = ["dep:colony-tui"]
```

- [ ] **Step 2: Build the binary**

Run: `cargo build --bin colony`
Expected: Compiles successfully. The default binary now uses `repl_tui`.

- [ ] **Step 3: Commit**

```bash
git add crates/colony-shell/Cargo.toml
git commit -m "feat(shell): enable TUI REPL by default"
```

### Task 10: Manual smoke test

- [ ] **Step 1: Run the shell and test sidebar**

Run: `./target/debug/colony`

Test checklist:
1. Shell starts normally, prompt appears
2. Press `Ctrl+B` — sidebar appears on left with file tree
3. Press `Tab` — focus moves to sidebar (cyan border)
4. Press `j`/`k` — cursor moves up/down through files
5. Press `Enter` on a directory — expands/collapses
6. Press `Enter` on a file — context menu appears
7. Press `i` in context menu — path inserted into prompt, focus returns to prompt
8. Press `Esc` — context menu dismisses
9. Press `Tab` — focus returns to prompt, cursor visible
10. Type a command and press Enter — command executes, sidebar reappears at next prompt
11. Press `Ctrl+B` — sidebar closes

- [ ] **Step 2: Fix any issues found**

- [ ] **Step 3: Final commit**

```bash
git add -A
git commit -m "feat(sidebar): file tree sidebar integration complete"
```

---

## Summary

| Task | Description | Est. |
|------|-------------|------|
| 1 | SidebarRenderer cursor highlighting | 5 min |
| 2 | SidebarState and Focus types | 3 min |
| 3 | Wire SidebarManager into REPL loop | 3 min |
| 4 | Sidebar draw layout | 5 min |
| 5 | Ctrl+B toggle, Tab focus, j/k nav | 5 min |
| 6 | Full keyboard navigation + context menu | 5 min |
| 7 | File actions (insert, preview, open, copy) | 5 min |
| 8 | Context menu overlay rendering | 3 min |
| 9 | Enable tui feature by default | 2 min |
| 10 | Manual smoke test | 5 min |
