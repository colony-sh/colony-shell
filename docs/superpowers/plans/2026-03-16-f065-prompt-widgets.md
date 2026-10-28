# F065: Interactive Prompt Widgets — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add interactive prompt widgets (file picker, branch picker, confirmation dialog, tool approval) to colony-shell's TUI as ratatui popup components, following the existing HistorySearch/CommandPalette pattern.

**Architecture:** Each widget is a struct with `active: bool`, activated via a new `Mode::PromptWidget` variant. Key events route through the mode system. Widgets return results via `accept() -> Option<T>`. When running inside colony-nerve (`COLONY_TERMINAL=1`), widgets also emit OSC 2337 messages for native SwiftUI rendering.

**Tech Stack:** Rust, ratatui 0.29, crossterm, existing colony-tui patterns

**Important:** Follow `RUST_STYLE_GUIDE.md`. Match existing patterns exactly:
- Popups use `active: bool` flag, not trait objects
- Mode enum for key routing (in `keybindings/modes.rs`)
- `accept()` returns `Option<T>`, deactivates internally
- Conditional rendering in `draw()` with `Clear` widget for background
- App stores popup as a struct field

**Key files to study:**
- `crates/colony-tui/src/input/history_search.rs` — popup pattern
- `crates/colony-tui/src/input/command_palette.rs` — popup pattern
- `crates/colony-tui/src/app.rs` — routing + rendering
- `crates/colony-tui/src/keybindings/modes.rs` — Mode enum
- `crates/colony-tui/src/keybindings/actions.rs` — Action enum
- `crates/colony-shell/src/widget_handler.rs` — existing branch_picker handler

---

## File Structure

```
crates/colony-tui/src/widgets/
├── mod.rs              # PromptWidget enum + shared types
├── file_picker.rs      # File/directory picker with fuzzy search
├── branch_picker.rs    # Git branch picker
├── confirm.rs          # Yes/No confirmation dialog
└── tool_approval.rs    # Tool call approval widget (replaces OSC-only path)
```

**Existing files to modify:**
- `crates/colony-tui/src/keybindings/modes.rs` — add `Mode::PromptWidget`
- `crates/colony-tui/src/keybindings/actions.rs` — add widget actions
- `crates/colony-tui/src/app.rs` — add widget field, routing, rendering

---

## Chunk 1: Widget Infrastructure + Confirmation Dialog

### Task 1: Create widgets module with shared types

**Files:**
- Create: `crates/colony-tui/src/widgets/mod.rs`
- Create: `crates/colony-tui/src/widgets/confirm.rs`
- Modify: `crates/colony-tui/src/keybindings/modes.rs`
- Modify: `crates/colony-tui/src/keybindings/actions.rs`

- [ ] **Step 1: Create widgets/mod.rs**

```rust
//! Interactive prompt widgets rendered as TUI popups.

pub mod confirm;
pub mod file_picker;
pub mod branch_picker;
pub mod tool_approval;

/// Result returned when a widget completes.
#[derive(Debug, Clone)]
pub enum WidgetResult {
    /// User accepted with a value.
    Accept(String),
    /// User cancelled.
    Cancel,
}

/// The kind of prompt widget currently active.
#[derive(Debug, Clone)]
pub enum ActiveWidget {
    Confirm(confirm::ConfirmWidget),
    FilePicker(file_picker::FilePickerWidget),
    BranchPicker(branch_picker::BranchPickerWidget),
    ToolApproval(tool_approval::ToolApprovalWidget),
}

/// Container for the currently active widget (if any).
#[derive(Debug, Default)]
pub struct PromptWidgetState {
    active: Option<ActiveWidget>,
    last_result: Option<WidgetResult>,
}

impl PromptWidgetState {
    pub fn is_active(&self) -> bool {
        self.active.is_some()
    }

    pub fn activate(&mut self, widget: ActiveWidget) {
        self.active = Some(widget);
        self.last_result = None;
    }

    pub fn dismiss(&mut self) {
        self.last_result = Some(WidgetResult::Cancel);
        self.active = None;
    }

    pub fn take_result(&mut self) -> Option<WidgetResult> {
        self.last_result.take()
    }

    pub fn active_widget(&self) -> Option<&ActiveWidget> {
        self.active.as_ref()
    }

    pub fn active_widget_mut(&mut self) -> Option<&mut ActiveWidget> {
        self.active.as_mut()
    }

    /// Handle a character input, routing to the active widget.
    pub fn insert_char(&mut self, c: char) {
        match self.active.as_mut() {
            Some(ActiveWidget::FilePicker(w)) => w.insert_char(c),
            Some(ActiveWidget::BranchPicker(w)) => w.insert_char(c),
            Some(ActiveWidget::ToolApproval(w)) => {} // No text input
            Some(ActiveWidget::Confirm(w)) => {}      // No text input
            None => {}
        }
    }

    /// Handle accept, returning the result.
    pub fn accept(&mut self) -> Option<WidgetResult> {
        let result = match self.active.as_mut() {
            Some(ActiveWidget::Confirm(w)) => w.accept(),
            Some(ActiveWidget::FilePicker(w)) => w.accept(),
            Some(ActiveWidget::BranchPicker(w)) => w.accept(),
            Some(ActiveWidget::ToolApproval(w)) => w.accept(),
            None => None,
        };

        if let Some(ref r) = result {
            self.last_result = Some(r.clone());
            self.active = None;
        }
        result
    }

    /// Move selection up.
    pub fn prev(&mut self) {
        match self.active.as_mut() {
            Some(ActiveWidget::FilePicker(w)) => w.prev(),
            Some(ActiveWidget::BranchPicker(w)) => w.prev(),
            Some(ActiveWidget::Confirm(w)) => w.toggle(),
            Some(ActiveWidget::ToolApproval(w)) => w.toggle(),
            None => {}
        }
    }

    /// Move selection down.
    pub fn next(&mut self) {
        match self.active.as_mut() {
            Some(ActiveWidget::FilePicker(w)) => w.next(),
            Some(ActiveWidget::BranchPicker(w)) => w.next(),
            Some(ActiveWidget::Confirm(w)) => w.toggle(),
            Some(ActiveWidget::ToolApproval(w)) => w.toggle(),
            None => {}
        }
    }
}
```

- [ ] **Step 2: Create confirm.rs**

```rust
//! Yes/No confirmation dialog.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use super::WidgetResult;

#[derive(Debug, Clone)]
pub struct ConfirmWidget {
    pub prompt: String,
    pub detail: Option<String>,
    selected_yes: bool,
}

impl ConfirmWidget {
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            detail: None,
            selected_yes: true,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn toggle(&mut self) {
        self.selected_yes = !self.selected_yes;
    }

    pub fn accept(&self) -> Option<WidgetResult> {
        if self.selected_yes {
            Some(WidgetResult::Accept("yes".to_string()))
        } else {
            Some(WidgetResult::Cancel)
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let width = 50u16.min(area.width.saturating_sub(4));
        let height = if self.detail.is_some() { 7 } else { 5 };
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        let popup = Rect::new(x, y, width, height);

        frame.render_widget(Clear, popup);

        let yes_style = if self.selected_yes {
            Style::default().fg(Color::Black).bg(Color::Green).bold()
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let no_style = if !self.selected_yes {
            Style::default().fg(Color::Black).bg(Color::Red).bold()
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let mut lines = vec![
            Line::from(""),
            Line::from(Span::styled(&self.prompt, Style::default().bold())),
        ];

        if let Some(ref detail) = self.detail {
            lines.push(Line::from(Span::styled(
                detail.as_str(),
                Style::default().fg(Color::DarkGray),
            )));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(" Yes ", yes_style),
            Span::raw("  "),
            Span::styled(" No ", no_style),
        ]));

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow))
            .title(" Confirm ");

        let paragraph = Paragraph::new(lines).block(block).alignment(Alignment::Center);
        frame.render_widget(paragraph, popup);
    }
}
```

- [ ] **Step 3: Add Mode::PromptWidget and actions**

In `crates/colony-tui/src/keybindings/modes.rs`, add `PromptWidget` to the Mode enum.

In `crates/colony-tui/src/keybindings/actions.rs`, add:
```rust
PromptWidgetAccept,
PromptWidgetCancel,
PromptWidgetNext,
PromptWidgetPrev,
```

- [ ] **Step 4: Create stub files for file_picker, branch_picker, tool_approval**

Each with a minimal struct + `accept()` returning `None` for now.

- [ ] **Step 5: Verify compile**

Run: `cargo check -p colony-tui`

- [ ] **Step 6: Commit**

```bash
git add crates/colony-tui/src/widgets/
git commit -m "feat(F065): prompt widget infrastructure + confirmation dialog"
```

---

## Chunk 2: File Picker + Branch Picker

### Task 2: FilePickerWidget

**Files:**
- Modify: `crates/colony-tui/src/widgets/file_picker.rs`

- [ ] **Step 1: Implement file picker**

```rust
//! Interactive file/directory picker with fuzzy search.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use std::fs;
use std::path::{Path, PathBuf};

use super::WidgetResult;

#[derive(Debug, Clone)]
pub struct FilePickerWidget {
    query: String,
    entries: Vec<FileEntry>,
    filtered: Vec<usize>,
    selected: usize,
    cwd: PathBuf,
    show_hidden: bool,
}

#[derive(Debug, Clone)]
struct FileEntry {
    name: String,
    path: PathBuf,
    is_dir: bool,
}

impl FilePickerWidget {
    pub fn new(cwd: PathBuf) -> Self {
        let mut picker = Self {
            query: String::new(),
            entries: Vec::new(),
            filtered: Vec::new(),
            selected: 0,
            cwd,
            show_hidden: false,
        };
        picker.scan_directory();
        picker.update_filter();
        picker
    }

    pub fn insert_char(&mut self, c: char) {
        self.query.push(c);
        self.update_filter();
        self.selected = 0;
    }

    pub fn backspace(&mut self) {
        self.query.pop();
        self.update_filter();
        self.selected = 0;
    }

    pub fn next(&mut self) {
        if !self.filtered.is_empty() {
            self.selected = (self.selected + 1) % self.filtered.len();
        }
    }

    pub fn prev(&mut self) {
        if !self.filtered.is_empty() {
            self.selected = self.selected.checked_sub(1).unwrap_or(self.filtered.len() - 1);
        }
    }

    pub fn accept(&self) -> Option<WidgetResult> {
        let idx = *self.filtered.get(self.selected)?;
        let entry = &self.entries[idx];

        if entry.is_dir {
            // Could navigate into directory — for now, return the path
            Some(WidgetResult::Accept(entry.path.to_string_lossy().to_string()))
        } else {
            Some(WidgetResult::Accept(entry.path.to_string_lossy().to_string()))
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let width = 60u16.min(area.width.saturating_sub(4));
        let height = 20u16.min(area.height.saturating_sub(4));
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        let popup = Rect::new(x, y, width, height);

        frame.render_widget(Clear, popup);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1)])
            .split(popup);

        // Search input
        let input = Paragraph::new(Line::from(vec![
            Span::styled("> ", Style::default().fg(Color::Cyan)),
            Span::raw(&self.query),
            Span::styled("_", Style::default().fg(Color::DarkGray)),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(format!(" Files: {} ", self.cwd.display())),
        );
        frame.render_widget(input, chunks[0]);

        // File list
        let items: Vec<ListItem> = self
            .filtered
            .iter()
            .enumerate()
            .take(height as usize - 4)
            .map(|(i, &idx)| {
                let entry = &self.entries[idx];
                let icon = if entry.is_dir { "📁" } else { "  " };
                let style = if i == self.selected {
                    Style::default().bg(Color::DarkGray).fg(Color::White)
                } else if entry.is_dir {
                    Style::default().fg(Color::Blue)
                } else {
                    Style::default()
                };
                ListItem::new(format!(" {icon} {}", entry.name)).style(style)
            })
            .collect();

        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        );
        frame.render_widget(list, chunks[1]);
    }

    fn scan_directory(&mut self) {
        self.entries.clear();

        // Parent directory
        if let Some(parent) = self.cwd.parent() {
            self.entries.push(FileEntry {
                name: "..".to_string(),
                path: parent.to_path_buf(),
                is_dir: true,
            });
        }

        let Ok(read_dir) = fs::read_dir(&self.cwd) else {
            return;
        };

        let mut dirs = Vec::new();
        let mut files = Vec::new();

        for entry in read_dir.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !self.show_hidden && name.starts_with('.') {
                continue;
            }
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            let fe = FileEntry {
                name,
                path: entry.path(),
                is_dir,
            };
            if is_dir {
                dirs.push(fe);
            } else {
                files.push(fe);
            }
        }

        dirs.sort_by(|a, b| a.name.cmp(&b.name));
        files.sort_by(|a, b| a.name.cmp(&b.name));

        self.entries.extend(dirs);
        self.entries.extend(files);
    }

    fn update_filter(&mut self) {
        if self.query.is_empty() {
            self.filtered = (0..self.entries.len()).collect();
        } else {
            let query_lower = self.query.to_lowercase();
            self.filtered = self
                .entries
                .iter()
                .enumerate()
                .filter(|(_, e)| e.name.to_lowercase().contains(&query_lower))
                .map(|(i, _)| i)
                .collect();
        }
    }
}
```

- [ ] **Step 2: Run compile check**

Run: `cargo check -p colony-tui`

- [ ] **Step 3: Commit**

```bash
git add crates/colony-tui/src/widgets/file_picker.rs
git commit -m "feat(F065): FilePickerWidget with fuzzy search and directory scanning"
```

### Task 3: BranchPickerWidget

**Files:**
- Modify: `crates/colony-tui/src/widgets/branch_picker.rs`

- [ ] **Step 1: Implement branch picker**

Similar to file picker but reads git branches via `git branch --format=%(refname:short)`. Shows current branch with a marker. Filter by query. Accept returns the selected branch name.

Key struct:
```rust
pub struct BranchPickerWidget {
    query: String,
    branches: Vec<BranchEntry>,
    filtered: Vec<usize>,
    selected: usize,
    current_branch: Option<String>,
}

struct BranchEntry {
    name: String,
    is_current: bool,
}
```

Constructor calls `git branch` to populate. Render shows branches with `*` for current. Accept returns branch name.

- [ ] **Step 2: Implement ToolApprovalWidget**

Similar to ConfirmWidget but with tool name, arguments, and risk level display.

```rust
pub struct ToolApprovalWidget {
    tool_name: String,
    arguments: String,
    component_id: String,
    approved: bool,
}
```

Render shows tool name, arguments, and Yes/No buttons.

- [ ] **Step 3: Verify compile**

Run: `cargo check -p colony-tui`

- [ ] **Step 4: Commit**

```bash
git add crates/colony-tui/src/widgets/
git commit -m "feat(F065): BranchPickerWidget + ToolApprovalWidget"
```

---

## Chunk 3: Wire into App

### Task 4: Integrate widgets into App routing and rendering

**Files:**
- Modify: `crates/colony-tui/src/app.rs`

- [ ] **Step 1: Add widget state to App**

Add field to App struct:
```rust
prompt_widget: widgets::PromptWidgetState,
```

Initialize in constructor with `PromptWidgetState::default()`.

- [ ] **Step 2: Add key routing for PromptWidget mode**

In the `handle_action()` match:
```rust
Action::InsertChar(c) => match self.keys.mode() {
    // ... existing ...
    Mode::PromptWidget => self.prompt_widget.insert_char(c),
    // ...
},

Action::PromptWidgetAccept => {
    if let Some(result) = self.prompt_widget.accept() {
        self.keys.set_mode(Mode::Insert);
        self.handle_widget_result(result);
    }
}

Action::PromptWidgetCancel => {
    self.prompt_widget.dismiss();
    self.keys.set_mode(Mode::Insert);
}

Action::PromptWidgetNext => self.prompt_widget.next(),
Action::PromptWidgetPrev => self.prompt_widget.prev(),
```

- [ ] **Step 3: Add rendering in draw()**

After existing popup renders:
```rust
if self.prompt_widget.is_active() {
    match self.prompt_widget.active_widget() {
        Some(ActiveWidget::Confirm(w)) => w.render(frame, main_area),
        Some(ActiveWidget::FilePicker(w)) => w.render(frame, main_area),
        Some(ActiveWidget::BranchPicker(w)) => w.render(frame, main_area),
        Some(ActiveWidget::ToolApproval(w)) => w.render(frame, main_area),
        None => {}
    }
}
```

- [ ] **Step 4: Add keybindings for PromptWidget mode**

In the keybinding mapper, when Mode is PromptWidget:
- `Enter` → `PromptWidgetAccept`
- `Escape` → `PromptWidgetCancel`
- `Tab` / `Down` → `PromptWidgetNext`
- `Shift+Tab` / `Up` → `PromptWidgetPrev`
- `Left` / `Right` → toggle (for confirm/approval)
- Other chars → `InsertChar` (routed to widget)

- [ ] **Step 5: Add activation actions**

Add actions to trigger widgets:
- `Ctrl+P` → activate file picker
- Git-aware contexts → activate branch picker
- Tool approval events → activate tool approval

- [ ] **Step 6: Build and test**

Run: `cargo check -p colony-tui`
Run: `cargo build -p colony-tui`

- [ ] **Step 7: Commit**

```bash
git add crates/colony-tui/
git commit -m "feat(F065): wire prompt widgets into App routing, rendering, and keybindings"
```

- [ ] **Step 8: Update features.json**

Set F065 status to `"complete"`.
