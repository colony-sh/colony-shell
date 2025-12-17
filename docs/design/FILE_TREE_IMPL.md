# Colony File Tree Pane

## Overview

Traditional shells require constant `ls`, `tree`, `find`, and `cd` commands to navigate and understand the filesystem. Colony integrates a **persistent file tree pane** that provides always-visible filesystem context, eliminating repetitive navigation commands and enabling direct manipulation of files.

This isn't just a visual `ls` — it's a fully integrated filesystem interface that:
- Updates reactively as the filesystem changes
- Integrates with git to show repository status
- Enables drag-and-drop and click-to-navigate
- Provides inline preview for files
- Syncs with agent context (AI knows what you're looking at)
- Supports multiple views and filters

---

## Core Concepts

### Pane Layout

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ Colony                                                            [_][□][×] │
├──────────────────────┬──────────────────────────────────────────────────────┤
│ 📁 myapp             │ ~/projects/myapp $ cargo build                       │
│ ├── 📁 src           │   Compiling myapp v0.1.0                             │
│ │   ├── 📄 main.rs   │   Finished dev [unoptimized + debuginfo]             │
│ │   ├── 📄 lib.rs    │                                                      │
│ │   └── 📁 handlers  │ ~/projects/myapp $ █                                 │
│ │       ├── 📄 mod.rs│                                                      │
│ │       └── 📄 api.rs│                                                      │
│ ├── 📁 tests         │                                                      │
│ ├── 📄 Cargo.toml    │                                                      │
│ ├── 📄 README.md     │                                                      │
│ └── 📄 .gitignore    │                                                      │
├──────────────────────┤                                                      │
│ [Filter: *.rs] 🔍    │                                                      │
└──────────────────────┴──────────────────────────────────────────────────────┘
```

### Pane Positions

The tree pane can be positioned in multiple locations:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreePanePosition {
    Left,           // Default, like VS Code
    Right,          // Alternative side
    Bottom,         // Horizontal split below terminal
    Floating,       // Overlay window
    Hidden,         // Collapsed/closed
}

#[derive(Debug, Clone)]
pub struct TreePaneConfig {
    pub position: TreePanePosition,
    pub width_percent: u16,          // For Left/Right (default: 25%)
    pub height_percent: u16,         // For Bottom (default: 30%)
    pub auto_collapse: bool,         // Collapse when terminal needs space
    pub min_width: u16,              // Minimum width in columns
    pub show_hidden: bool,           // Show dotfiles
    pub follow_cwd: bool,            // Auto-expand to match shell cwd
}
```

---

## Data Model

### File Tree Structure

```rust
use std::path::PathBuf;
use std::time::SystemTime;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct FileNode {
    pub path: PathBuf,
    pub name: String,
    pub kind: FileKind,
    pub size: u64,
    pub modified: SystemTime,
    pub permissions: Permissions,
    pub git_status: Option<GitStatus>,
    pub children: Option<Vec<FileNode>>,  // None if not yet loaded (lazy)
    pub expanded: bool,
    pub selected: bool,
    pub marked: bool,                      // For bulk operations
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileKind {
    File { 
        mime_type: Option<String>,
        executable: bool,
    },
    Directory {
        item_count: Option<usize>,  // Cached child count
    },
    Symlink { 
        target: PathBuf,
        broken: bool,
    },
    Special,  // Device files, sockets, etc.
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitStatus {
    Untracked,
    Modified,
    Staged,
    Renamed,
    Deleted,
    Ignored,
    Conflict,
    Clean,
}

#[derive(Debug, Clone)]
pub struct FileTree {
    pub root: PathBuf,
    pub nodes: HashMap<PathBuf, FileNode>,
    pub visible_paths: Vec<PathBuf>,      // Flattened for rendering
    pub cursor: usize,                     // Index in visible_paths
    pub scroll_offset: usize,
    pub filter: Option<TreeFilter>,
    pub sort: TreeSort,
}
```

### Lazy Loading

Large directories load children on-demand:

```rust
impl FileTree {
    /// Load children for a directory node (lazy loading)
    pub async fn expand(&mut self, path: &Path) -> Result<()> {
        let node = self.nodes.get_mut(path).ok_or(Error::NotFound)?;
        
        if !matches!(node.kind, FileKind::Directory { .. }) {
            return Err(Error::NotDirectory);
        }
        
        if node.children.is_some() {
            // Already loaded, just toggle visibility
            node.expanded = !node.expanded;
        } else {
            // Load children from filesystem
            let children = self.load_children(path).await?;
            node.children = Some(children);
            node.expanded = true;
        }
        
        self.rebuild_visible_paths();
        Ok(())
    }
    
    /// Load children with streaming for large directories
    async fn load_children(&self, path: &Path) -> Result<Vec<FileNode>> {
        let mut children = Vec::new();
        let mut entries = tokio::fs::read_dir(path).await?;
        
        while let Some(entry) = entries.next_entry().await? {
            let node = self.create_node(&entry).await?;
            
            // Apply filter early to avoid loading unnecessary data
            if self.filter.as_ref().map(|f| f.matches(&node)).unwrap_or(true) {
                children.push(node);
            }
        }
        
        // Sort according to current sort settings
        self.sort_nodes(&mut children);
        
        Ok(children)
    }
}
```

---

## File System Watching

### Real-Time Updates

The tree pane watches the filesystem and updates reactively:

```rust
use notify::{Watcher, RecursiveMode, Event, EventKind};
use tokio::sync::mpsc;

pub struct TreeWatcher {
    watcher: RecommendedWatcher,
    event_rx: mpsc::UnboundedReceiver<FileSystemEvent>,
    watched_dirs: HashSet<PathBuf>,
    debounce_ms: u64,
}

#[derive(Debug, Clone)]
pub enum FileSystemEvent {
    Created(PathBuf),
    Modified(PathBuf),
    Deleted(PathBuf),
    Renamed { from: PathBuf, to: PathBuf },
    AttributeChanged(PathBuf),
}

impl TreeWatcher {
    /// Watch a directory and its expanded children
    pub fn watch_expanded(&mut self, tree: &FileTree) -> Result<()> {
        // Watch root
        self.watch_dir(&tree.root)?;
        
        // Watch all expanded directories
        for (path, node) in &tree.nodes {
            if node.expanded && matches!(node.kind, FileKind::Directory { .. }) {
                self.watch_dir(path)?;
            }
        }
        
        Ok(())
    }
    
    /// Process filesystem events and update tree
    pub async fn process_events(&mut self, tree: &mut FileTree) {
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                FileSystemEvent::Created(path) => {
                    if let Some(parent) = path.parent() {
                        if tree.is_expanded(parent) {
                            let node = tree.create_node_from_path(&path).await.ok();
                            if let Some(node) = node {
                                tree.insert_child(parent, node);
                            }
                        }
                    }
                }
                FileSystemEvent::Deleted(path) => {
                    tree.remove_node(&path);
                }
                FileSystemEvent::Modified(path) => {
                    tree.refresh_node(&path).await.ok();
                }
                FileSystemEvent::Renamed { from, to } => {
                    tree.rename_node(&from, &to);
                }
                _ => {}
            }
        }
        
        tree.rebuild_visible_paths();
    }
}
```

### Efficient Git Status Updates

```rust
pub struct GitStatusCache {
    repo_root: PathBuf,
    statuses: HashMap<PathBuf, GitStatus>,
    last_update: Instant,
    update_interval: Duration,
}

impl GitStatusCache {
    /// Batch update git status for visible files
    pub async fn refresh(&mut self, visible_paths: &[PathBuf]) -> Result<()> {
        // Run git status --porcelain for efficiency
        let output = Command::new("git")
            .args(["status", "--porcelain", "-uall"])
            .current_dir(&self.repo_root)
            .output()
            .await?;
        
        self.statuses.clear();
        
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            if line.len() < 4 {
                continue;
            }
            
            let status_chars = &line[0..2];
            let file_path = self.repo_root.join(&line[3..]);
            
            let status = match status_chars {
                "??" => GitStatus::Untracked,
                " M" | "MM" => GitStatus::Modified,
                "M " | "A " => GitStatus::Staged,
                "R " => GitStatus::Renamed,
                " D" | "D " => GitStatus::Deleted,
                "!!" => GitStatus::Ignored,
                "UU" => GitStatus::Conflict,
                _ => GitStatus::Clean,
            };
            
            self.statuses.insert(file_path, status);
        }
        
        self.last_update = Instant::now();
        Ok(())
    }
    
    /// Propagate status to parent directories
    pub fn get_directory_status(&self, dir: &Path) -> GitStatus {
        let mut dominated_status = GitStatus::Clean;
        
        for (path, status) in &self.statuses {
            if path.starts_with(dir) {
                dominated_status = dominated_status.merge(*status);
            }
        }
        
        dominated_status
    }
}

impl GitStatus {
    /// Merge status (higher priority wins)
    fn merge(self, other: GitStatus) -> GitStatus {
        use GitStatus::*;
        match (self, other) {
            (Conflict, _) | (_, Conflict) => Conflict,
            (Modified, _) | (_, Modified) => Modified,
            (Staged, _) | (_, Staged) => Staged,
            (Untracked, _) | (_, Untracked) => Untracked,
            (Deleted, _) | (_, Deleted) => Deleted,
            (Renamed, _) | (_, Renamed) => Renamed,
            _ => Clean,
        }
    }
}
```

---

## Interaction Model

### Mouse Interactions

```rust
#[derive(Debug, Clone)]
pub enum TreeMouseAction {
    // Single click
    Select,                    // Click on row: select file
    ToggleExpand,             // Click on arrow: expand/collapse
    
    // Double click
    Open,                      // Open file in $EDITOR or cd into directory
    
    // Right click
    ContextMenu,              // Show context menu
    
    // Drag operations
    DragStart(PathBuf),
    DragOver(PathBuf),
    Drop { 
        source: PathBuf, 
        target: PathBuf, 
        modifier: DragModifier 
    },
}

#[derive(Debug, Clone, Copy)]
pub enum DragModifier {
    None,       // Move
    Ctrl,       // Copy
    Alt,        // Create symlink
}

impl FileTreePane {
    pub fn handle_mouse(&mut self, event: MouseEvent) -> Option<TreeAction> {
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let row = self.row_at_position(event.row);
                if let Some(path) = self.path_at_row(row) {
                    let column = event.column - self.bounds.x;
                    
                    if self.is_expand_arrow_column(column, &path) {
                        Some(TreeAction::ToggleExpand(path))
                    } else {
                        Some(TreeAction::Select(path))
                    }
                } else {
                    None
                }
            }
            MouseEventKind::DoubleClick(MouseButton::Left) => {
                self.selected_path().map(|p| TreeAction::Open(p))
            }
            MouseEventKind::Down(MouseButton::Right) => {
                self.selected_path().map(|p| TreeAction::ContextMenu(p))
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                // Handle drag operation
                self.drag_state.as_ref().map(|d| TreeAction::DragUpdate(d.clone()))
            }
            MouseEventKind::Up(MouseButton::Left) if self.drag_state.is_some() => {
                let drag = self.drag_state.take().unwrap();
                let target = self.path_at_position(event.row)?;
                Some(TreeAction::Drop { 
                    source: drag.source, 
                    target,
                    copy: event.modifiers.contains(KeyModifiers::CONTROL),
                })
            }
            _ => None,
        }
    }
}
```

### Keyboard Navigation

```rust
impl FileTreePane {
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<TreeAction> {
        match key.code {
            // Navigation
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_cursor(-1);
                None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_cursor(1);
                None
            }
            KeyCode::PageUp => {
                self.move_cursor(-(self.visible_rows() as i32));
                None
            }
            KeyCode::PageDown => {
                self.move_cursor(self.visible_rows() as i32);
                None
            }
            KeyCode::Home | KeyCode::Char('g') if key.modifiers.is_empty() => {
                self.cursor = 0;
                self.scroll_to_cursor();
                None
            }
            KeyCode::End | KeyCode::Char('G') => {
                self.cursor = self.visible_paths.len().saturating_sub(1);
                self.scroll_to_cursor();
                None
            }
            
            // Expansion
            KeyCode::Right | KeyCode::Char('l') => {
                self.selected_path().map(TreeAction::Expand)
            }
            KeyCode::Left | KeyCode::Char('h') => {
                self.selected_path().map(TreeAction::Collapse)
            }
            KeyCode::Enter => {
                self.selected_path().map(TreeAction::ToggleExpand)
            }
            
            // Actions
            KeyCode::Char('o') => {
                // Open file/directory
                self.selected_path().map(TreeAction::Open)
            }
            KeyCode::Char('e') => {
                // Edit in $EDITOR
                self.selected_path().map(TreeAction::Edit)
            }
            KeyCode::Char('y') => {
                // Yank (copy path to clipboard)
                self.selected_path().map(TreeAction::YankPath)
            }
            KeyCode::Char('Y') => {
                // Yank absolute path
                self.selected_path().map(TreeAction::YankAbsolutePath)
            }
            
            // Marking for bulk operations
            KeyCode::Char(' ') => {
                self.toggle_mark();
                self.move_cursor(1);
                None
            }
            KeyCode::Char('m') => {
                // Mark all matching filter
                Some(TreeAction::MarkFiltered)
            }
            KeyCode::Char('u') => {
                // Unmark all
                self.clear_marks();
                None
            }
            
            // File operations
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Delete (with confirmation)
                self.get_marked_or_selected().map(TreeAction::Delete)
            }
            KeyCode::Char('r') => {
                // Rename
                self.selected_path().map(TreeAction::Rename)
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Copy
                self.get_marked_or_selected().map(TreeAction::Copy)
            }
            KeyCode::Char('x') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Cut
                self.get_marked_or_selected().map(TreeAction::Cut)
            }
            KeyCode::Char('v') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Paste
                self.selected_path().map(TreeAction::Paste)
            }
            KeyCode::Char('n') => {
                // New file
                self.selected_path().map(TreeAction::NewFile)
            }
            KeyCode::Char('N') => {
                // New directory
                self.selected_path().map(TreeAction::NewDirectory)
            }
            
            // Search/Filter
            KeyCode::Char('/') => {
                Some(TreeAction::StartFilter)
            }
            KeyCode::Escape => {
                if self.filter.is_some() {
                    self.clear_filter();
                }
                None
            }
            
            // Quick navigation
            KeyCode::Char('~') => {
                Some(TreeAction::GoHome)
            }
            KeyCode::Char('-') => {
                Some(TreeAction::GoParent)
            }
            KeyCode::Char('.') => {
                self.toggle_hidden();
                None
            }
            
            _ => None,
        }
    }
}
```

### Context Menu

```rust
#[derive(Debug, Clone)]
pub struct ContextMenu {
    pub items: Vec<ContextMenuItem>,
    pub position: (u16, u16),
    pub selected: usize,
}

#[derive(Debug, Clone)]
pub struct ContextMenuItem {
    pub label: String,
    pub shortcut: Option<String>,
    pub action: TreeAction,
    pub enabled: bool,
    pub separator_after: bool,
}

impl ContextMenu {
    pub fn for_file(path: &Path, node: &FileNode) -> Self {
        let mut items = vec![
            ContextMenuItem::new("Open", Some("o"), TreeAction::Open(path.to_path_buf())),
            ContextMenuItem::new("Edit", Some("e"), TreeAction::Edit(path.to_path_buf())),
            ContextMenuItem::separator(),
            ContextMenuItem::new("Copy", Some("Ctrl+C"), TreeAction::Copy(vec![path.to_path_buf()])),
            ContextMenuItem::new("Cut", Some("Ctrl+X"), TreeAction::Cut(vec![path.to_path_buf()])),
            ContextMenuItem::new("Delete", Some("Ctrl+D"), TreeAction::Delete(vec![path.to_path_buf()])),
            ContextMenuItem::new("Rename", Some("r"), TreeAction::Rename(path.to_path_buf())),
            ContextMenuItem::separator(),
            ContextMenuItem::new("Copy Path", Some("y"), TreeAction::YankPath(path.to_path_buf())),
            ContextMenuItem::new("Copy Absolute Path", Some("Y"), TreeAction::YankAbsolutePath(path.to_path_buf())),
            ContextMenuItem::separator(),
        ];
        
        // Add git-specific options if applicable
        if let Some(git_status) = &node.git_status {
            match git_status {
                GitStatus::Modified | GitStatus::Untracked => {
                    items.push(ContextMenuItem::new("Stage", None, TreeAction::GitStage(path.to_path_buf())));
                }
                GitStatus::Staged => {
                    items.push(ContextMenuItem::new("Unstage", None, TreeAction::GitUnstage(path.to_path_buf())));
                }
                GitStatus::Modified => {
                    items.push(ContextMenuItem::new("Discard Changes", None, TreeAction::GitDiscard(path.to_path_buf())));
                }
                _ => {}
            }
            items.push(ContextMenuItem::separator());
        }
        
        items.push(ContextMenuItem::new("Properties", None, TreeAction::ShowProperties(path.to_path_buf())));
        
        Self {
            items,
            position: (0, 0),
            selected: 0,
        }
    }
    
    pub fn for_directory(path: &Path, node: &FileNode) -> Self {
        let mut items = vec![
            ContextMenuItem::new("Open in Terminal", Some("Enter"), TreeAction::CdTo(path.to_path_buf())),
            ContextMenuItem::new("Open in Finder", None, TreeAction::RevealInFinder(path.to_path_buf())),
            ContextMenuItem::separator(),
            ContextMenuItem::new("New File", Some("n"), TreeAction::NewFile(path.to_path_buf())),
            ContextMenuItem::new("New Folder", Some("N"), TreeAction::NewDirectory(path.to_path_buf())),
            ContextMenuItem::separator(),
            ContextMenuItem::new("Copy", Some("Ctrl+C"), TreeAction::Copy(vec![path.to_path_buf()])),
            ContextMenuItem::new("Cut", Some("Ctrl+X"), TreeAction::Cut(vec![path.to_path_buf()])),
            ContextMenuItem::new("Paste", Some("Ctrl+V"), TreeAction::Paste(path.to_path_buf())),
            ContextMenuItem::new("Delete", Some("Ctrl+D"), TreeAction::Delete(vec![path.to_path_buf()])),
            ContextMenuItem::new("Rename", Some("r"), TreeAction::Rename(path.to_path_buf())),
            ContextMenuItem::separator(),
            ContextMenuItem::new("Find in Folder", Some("/"), TreeAction::SearchInDirectory(path.to_path_buf())),
            ContextMenuItem::separator(),
            ContextMenuItem::new("Copy Path", Some("y"), TreeAction::YankPath(path.to_path_buf())),
        ];
        
        Self {
            items,
            position: (0, 0),
            selected: 0,
        }
    }
}
```

---

## Filtering and Search

### Filter Types

```rust
#[derive(Debug, Clone)]
pub enum TreeFilter {
    /// Simple glob pattern (*.rs, test_*, etc.)
    Glob(glob::Pattern),
    
    /// Regular expression
    Regex(regex::Regex),
    
    /// File type filter
    FileType(FileTypeFilter),
    
    /// Git status filter
    GitStatus(Vec<GitStatus>),
    
    /// Size filter
    Size { min: Option<u64>, max: Option<u64> },
    
    /// Modified time filter
    Modified { after: Option<SystemTime>, before: Option<SystemTime> },
    
    /// Combination of filters
    And(Vec<TreeFilter>),
    Or(Vec<TreeFilter>),
    Not(Box<TreeFilter>),
}

#[derive(Debug, Clone)]
pub enum FileTypeFilter {
    Files,
    Directories,
    Symlinks,
    Executable,
    Hidden,
    // By extension groups
    Code,       // .rs, .py, .js, .ts, .go, etc.
    Config,     // .toml, .yaml, .json, .xml, etc.
    Document,   // .md, .txt, .pdf, .doc, etc.
    Image,      // .png, .jpg, .gif, .svg, etc.
    Video,      // .mp4, .mov, .avi, etc.
    Audio,      // .mp3, .wav, .flac, etc.
}

impl TreeFilter {
    pub fn matches(&self, node: &FileNode) -> bool {
        match self {
            TreeFilter::Glob(pattern) => pattern.matches(&node.name),
            TreeFilter::Regex(re) => re.is_match(&node.name),
            TreeFilter::FileType(ft) => ft.matches(node),
            TreeFilter::GitStatus(statuses) => {
                node.git_status.as_ref()
                    .map(|s| statuses.contains(s))
                    .unwrap_or(false)
            }
            TreeFilter::Size { min, max } => {
                let size = node.size;
                min.map(|m| size >= m).unwrap_or(true) &&
                max.map(|m| size <= m).unwrap_or(true)
            }
            TreeFilter::Modified { after, before } => {
                let modified = node.modified;
                after.map(|t| modified >= t).unwrap_or(true) &&
                before.map(|t| modified <= t).unwrap_or(true)
            }
            TreeFilter::And(filters) => filters.iter().all(|f| f.matches(node)),
            TreeFilter::Or(filters) => filters.iter().any(|f| f.matches(node)),
            TreeFilter::Not(filter) => !filter.matches(node),
        }
    }
    
    /// Parse filter from string (user input)
    pub fn parse(input: &str) -> Result<Self, FilterParseError> {
        let input = input.trim();
        
        // Check for special prefixes
        if let Some(pattern) = input.strip_prefix("git:") {
            return Self::parse_git_filter(pattern);
        }
        if let Some(pattern) = input.strip_prefix("type:") {
            return Self::parse_type_filter(pattern);
        }
        if let Some(pattern) = input.strip_prefix("size:") {
            return Self::parse_size_filter(pattern);
        }
        if let Some(pattern) = input.strip_prefix("re:") {
            return Ok(TreeFilter::Regex(regex::Regex::new(pattern)?));
        }
        
        // Default to glob
        Ok(TreeFilter::Glob(glob::Pattern::new(input)?))
    }
}
```

### Interactive Filter UI

```
┌─────────────────────────────────────────┐
│ 📁 myapp                                │
│ ├── 📁 src                              │
│ │   ├── 📄 main.rs                      │
│ │   ├── 📄 lib.rs                       │
│ │   └── 📄 utils.rs                     │
│ ├── 📁 tests                            │
│ │   └── 📄 integration_test.rs          │
│ └── 📄 build.rs                         │
├─────────────────────────────────────────┤
│ Filter: *.rs                        [×] │
│ [glob] [re:] [git:] [type:] [size:]    │
└─────────────────────────────────────────┘
```

Quick filter commands:

```bash
# In the filter input
*.rs                    # Glob: all Rust files
re:test.*\.rs$         # Regex: test files
git:modified           # Git: modified files
git:staged             # Git: staged files
git:untracked          # Git: untracked files
type:code              # File type: code files
type:config            # File type: config files
size:>1mb              # Size: larger than 1MB
size:<100kb            # Size: smaller than 100KB
```

---

## Preview Pane

### Inline Preview

Show file preview without opening editor:

```
┌──────────────────────┬─────────────────────────────────────────┐
│ 📁 myapp             │ main.rs                            [×]  │
│ ├── 📁 src           ├─────────────────────────────────────────┤
│ │   ├── 📄 main.rs ◀─│  1 │ use std::env;                      │
│ │   ├── 📄 lib.rs    │  2 │                                    │
│ │   └── 📄 utils.rs  │  3 │ fn main() {                        │
│ ├── 📄 Cargo.toml    │  4 │     let args: Vec<String> =        │
│ └── 📄 README.md     │  5 │         env::args().collect();     │
│                      │  6 │                                    │
│                      │  7 │     println!("Hello, {}!",         │
│                      │  8 │         args.get(1).unwrap_or(     │
│                      │  9 │             &"World".to_string()   │
│                      │ 10 │         ));                        │
│                      │ 11 │ }                                  │
└──────────────────────┴─────────────────────────────────────────┘
```

### Preview Types

```rust
#[derive(Debug, Clone)]
pub enum PreviewContent {
    /// Syntax-highlighted text
    Text {
        content: String,
        language: Option<String>,
        line_numbers: bool,
    },
    
    /// Image preview (Kitty/Sixel)
    Image {
        data: Vec<u8>,
        width: u32,
        height: u32,
        format: ImageFormat,
    },
    
    /// Directory listing
    Directory {
        entries: Vec<DirectoryEntry>,
        total_size: u64,
    },
    
    /// Binary file hex view
    Hex {
        bytes: Vec<u8>,
        total_size: u64,
    },
    
    /// Archive contents
    Archive {
        entries: Vec<ArchiveEntry>,
        format: ArchiveFormat,
    },
    
    /// PDF preview (first page as image)
    Pdf {
        page_count: usize,
        first_page: Vec<u8>,
    },
    
    /// Markdown rendered
    Markdown {
        rendered: String,
    },
    
    /// Git diff
    Diff {
        hunks: Vec<DiffHunk>,
    },
    
    /// File too large
    TooLarge {
        size: u64,
        mime_type: Option<String>,
    },
    
    /// Binary, no preview
    Binary {
        mime_type: Option<String>,
        size: u64,
    },
}

impl PreviewContent {
    pub async fn from_path(path: &Path, max_size: u64) -> Result<Self> {
        let metadata = tokio::fs::metadata(path).await?;
        
        if metadata.is_dir() {
            return Self::preview_directory(path).await;
        }
        
        if metadata.len() > max_size {
            return Ok(PreviewContent::TooLarge {
                size: metadata.len(),
                mime_type: mime_guess::from_path(path)
                    .first()
                    .map(|m| m.to_string()),
            });
        }
        
        // Determine preview type by extension/mime
        let mime = mime_guess::from_path(path).first();
        
        match mime.as_ref().map(|m| m.type_().as_str()) {
            Some("text") => Self::preview_text(path).await,
            Some("image") => Self::preview_image(path).await,
            Some("application") => {
                // Check specific application types
                match mime.as_ref().map(|m| m.subtype().as_str()) {
                    Some("pdf") => Self::preview_pdf(path).await,
                    Some("zip") | Some("x-tar") | Some("gzip") => {
                        Self::preview_archive(path).await
                    }
                    Some("json") | Some("toml") | Some("yaml") | Some("xml") => {
                        Self::preview_text(path).await
                    }
                    _ => Self::preview_binary(path).await,
                }
            }
            _ => {
                // Try text preview, fall back to binary
                match Self::preview_text(path).await {
                    Ok(preview) => Ok(preview),
                    Err(_) => Self::preview_binary(path).await,
                }
            }
        }
    }
}
```

---

## Shell Integration

### Syncing with `cd`

The tree pane stays synchronized with the shell's current directory:

```rust
impl Shell {
    pub fn change_directory(&mut self, new_dir: &Path) -> Result<()> {
        // ... existing cd logic ...
        
        // Notify tree pane of directory change
        if self.config.tree_pane.follow_cwd {
            self.tree_pane.reveal_and_focus(new_dir)?;
        }
        
        Ok(())
    }
}

impl FileTreePane {
    /// Expand tree to show path and optionally focus it
    pub fn reveal_and_focus(&mut self, path: &Path) -> Result<()> {
        // Expand all ancestors
        for ancestor in path.ancestors().skip(1).collect::<Vec<_>>().into_iter().rev() {
            if self.tree.is_directory(ancestor) {
                self.tree.expand(ancestor)?;
            }
        }
        
        // Focus the path
        if let Some(index) = self.tree.visible_paths.iter().position(|p| p == path) {
            self.cursor = index;
            self.scroll_to_cursor();
        }
        
        Ok(())
    }
}
```

### Tree Actions to Shell Commands

Clicking/selecting files can inject commands into the shell:

```rust
#[derive(Debug, Clone)]
pub enum TreeToShellAction {
    /// cd into directory
    ChangeDirectory(PathBuf),
    
    /// Open file in $EDITOR
    EditFile(PathBuf),
    
    /// Insert path at cursor position in command line
    InsertPath(PathBuf),
    
    /// Run a command with the file as argument
    RunCommand { command: String, args: Vec<PathBuf> },
}

impl FileTreePane {
    pub fn execute_action(&self, action: TreeAction) -> Option<TreeToShellAction> {
        match action {
            TreeAction::CdTo(path) => {
                Some(TreeToShellAction::ChangeDirectory(path))
            }
            TreeAction::Edit(path) => {
                Some(TreeToShellAction::EditFile(path))
            }
            TreeAction::Open(path) => {
                let node = self.tree.nodes.get(&path)?;
                match &node.kind {
                    FileKind::Directory { .. } => {
                        Some(TreeToShellAction::ChangeDirectory(path))
                    }
                    FileKind::File { executable: true, .. } => {
                        Some(TreeToShellAction::RunCommand {
                            command: path.display().to_string(),
                            args: vec![],
                        })
                    }
                    FileKind::File { .. } => {
                        Some(TreeToShellAction::EditFile(path))
                    }
                    _ => None,
                }
            }
            TreeAction::InsertInCommandLine(path) => {
                Some(TreeToShellAction::InsertPath(path))
            }
            _ => None,
        }
    }
}
```

### Drag-and-Drop to Terminal

Dragging a file to the terminal inserts its path:

```rust
impl Terminal {
    pub fn handle_drop(&mut self, paths: Vec<PathBuf>, position: (u16, u16)) -> Result<()> {
        // Quote paths with spaces or special characters
        let quoted_paths: Vec<String> = paths
            .iter()
            .map(|p| shell_quote(p))
            .collect();
        
        // Insert at cursor position in command line
        let insert_text = quoted_paths.join(" ");
        self.input_buffer.insert_at_cursor(&insert_text);
        
        Ok(())
    }
}

fn shell_quote(path: &Path) -> String {
    let s = path.display().to_string();
    
    // Check if quoting is needed
    if s.chars().any(|c| c.is_whitespace() || "\"'\\$`!*?[]{}()".contains(c)) {
        format!("'{}'", s.replace("'", "'\\''"))
    } else {
        s
    }
}
```

---

## Agent Integration

### Context Awareness

The tree pane provides context to AI agents about the current filesystem state:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct FileTreeContext {
    /// Current working directory
    pub cwd: PathBuf,
    
    /// Currently selected file/directory
    pub selected: Option<PathBuf>,
    
    /// Marked files (for bulk operations)
    pub marked: Vec<PathBuf>,
    
    /// Visible (expanded) structure
    pub visible_tree: TreeStructure,
    
    /// Active filter
    pub filter: Option<String>,
    
    /// Git status summary
    pub git_summary: Option<GitSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TreeStructure {
    pub root: String,
    pub children: Vec<TreeNode>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TreeNode {
    pub name: String,
    pub kind: String,  // "file", "directory", "symlink"
    pub git_status: Option<String>,
    pub children: Option<Vec<TreeNode>>,
}

impl FileTreePane {
    /// Generate context for AI agent
    pub fn get_agent_context(&self) -> FileTreeContext {
        FileTreeContext {
            cwd: self.tree.root.clone(),
            selected: self.selected_path(),
            marked: self.get_marked_paths(),
            visible_tree: self.tree.to_structure(),
            filter: self.filter.as_ref().map(|f| f.to_string()),
            git_summary: self.git_cache.as_ref().map(|c| c.summary()),
        }
    }
}
```

### Agent Commands for Tree

Agents can issue commands to manipulate the tree:

```rust
#[derive(Debug, Clone, Deserialize)]
pub enum AgentTreeCommand {
    /// Navigate to a path
    Navigate { path: String },
    
    /// Expand a directory
    Expand { path: String },
    
    /// Collapse a directory
    Collapse { path: String },
    
    /// Select a file
    Select { path: String },
    
    /// Apply a filter
    Filter { pattern: String },
    
    /// Clear filter
    ClearFilter,
    
    /// Mark files for operation
    Mark { paths: Vec<String> },
    
    /// Create new file
    CreateFile { path: String, content: Option<String> },
    
    /// Create new directory
    CreateDirectory { path: String },
    
    /// Show specific path to user (expand + scroll into view)
    Reveal { path: String },
}

impl FileTreePane {
    pub async fn execute_agent_command(&mut self, cmd: AgentTreeCommand) -> Result<AgentResponse> {
        match cmd {
            AgentTreeCommand::Navigate { path } => {
                let path = self.resolve_path(&path)?;
                self.reveal_and_focus(&path)?;
                Ok(AgentResponse::success("Navigated to path"))
            }
            AgentTreeCommand::Reveal { path } => {
                let path = self.resolve_path(&path)?;
                self.reveal_and_focus(&path)?;
                // Also highlight briefly
                self.highlight_path(&path, Duration::from_secs(2));
                Ok(AgentResponse::success("Revealed path"))
            }
            AgentTreeCommand::CreateFile { path, content } => {
                let path = self.resolve_path(&path)?;
                if let Some(content) = content {
                    tokio::fs::write(&path, content).await?;
                } else {
                    tokio::fs::File::create(&path).await?;
                }
                self.tree.refresh_parent(&path).await?;
                self.reveal_and_focus(&path)?;
                Ok(AgentResponse::success("Created file"))
            }
            // ... other commands
        }
    }
}
```

### MCP Tool Exposure

Expose tree operations as MCP tools:

```rust
pub fn register_tree_tools(server: &mut McpServer, tree_pane: Arc<Mutex<FileTreePane>>) {
    server.register_tool(Tool {
        name: "tree_context".to_string(),
        description: "Get the current file tree context including cwd, selected file, and visible structure".to_string(),
        input_schema: json!({"type": "object", "properties": {}}),
        handler: Box::new(move |_params| {
            let tree = tree_pane.lock().unwrap();
            let context = tree.get_agent_context();
            Ok(serde_json::to_value(context)?)
        }),
    });
    
    server.register_tool(Tool {
        name: "tree_navigate".to_string(),
        description: "Navigate the file tree to a specific path".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Path to navigate to"}
            },
            "required": ["path"]
        }),
        handler: Box::new(move |params| {
            let path = params["path"].as_str().ok_or("Missing path")?;
            let mut tree = tree_pane.lock().unwrap();
            tree.reveal_and_focus(Path::new(path))?;
            Ok(json!({"status": "success"}))
        }),
    });
    
    server.register_tool(Tool {
        name: "tree_create_file".to_string(),
        description: "Create a new file in the filesystem".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "content": {"type": "string"}
            },
            "required": ["path"]
        }),
        handler: Box::new(move |params| {
            // ... implementation
        }),
    });
}
```

---

## Visual Design

### Icons and Colors

```rust
pub struct TreeTheme {
    // Icons (nerd font / unicode)
    pub icon_file: &'static str,           // 📄 or 
    pub icon_directory: &'static str,      // 📁 or 
    pub icon_directory_open: &'static str, // 📂 or 
    pub icon_symlink: &'static str,        // 🔗 or 
    pub icon_git_modified: &'static str,   // ● or M
    pub icon_git_staged: &'static str,     // ✓ or S
    pub icon_git_untracked: &'static str,  // ? or U
    
    // Extension-specific icons
    pub extension_icons: HashMap<&'static str, &'static str>,
    
    // Colors
    pub color_file: Color,
    pub color_directory: Color,
    pub color_symlink: Color,
    pub color_executable: Color,
    pub color_hidden: Color,
    pub color_git_modified: Color,
    pub color_git_staged: Color,
    pub color_git_untracked: Color,
    pub color_git_ignored: Color,
    pub color_selected_bg: Color,
    pub color_cursor_bg: Color,
    pub color_marked: Color,
}

impl Default for TreeTheme {
    fn default() -> Self {
        Self {
            icon_file: "📄",
            icon_directory: "📁",
            icon_directory_open: "📂",
            icon_symlink: "🔗",
            icon_git_modified: "●",
            icon_git_staged: "✓",
            icon_git_untracked: "?",
            
            extension_icons: [
                ("rs", "🦀"),
                ("py", "🐍"),
                ("js", ""),
                ("ts", ""),
                ("go", ""),
                ("md", ""),
                ("json", ""),
                ("toml", ""),
                ("yaml", ""),
                ("yml", ""),
                ("lock", "🔒"),
            ].into_iter().collect(),
            
            color_file: Color::White,
            color_directory: Color::Blue,
            color_symlink: Color::Cyan,
            color_executable: Color::Green,
            color_hidden: Color::DarkGray,
            color_git_modified: Color::Yellow,
            color_git_staged: Color::Green,
            color_git_untracked: Color::Red,
            color_git_ignored: Color::DarkGray,
            color_selected_bg: Color::Rgb(40, 40, 60),
            color_cursor_bg: Color::Rgb(60, 60, 90),
            color_marked: Color::Magenta,
        }
    }
}
```

### Rendering

```rust
impl FileTreePane {
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .title(format!(" {} ", self.tree.root.display()))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));
        
        let inner_area = block.inner(area);
        frame.render_widget(block, area);
        
        // Render visible rows
        for (i, row_index) in (self.scroll_offset..self.scroll_offset + inner_area.height as usize)
            .enumerate()
        {
            if row_index >= self.tree.visible_paths.len() {
                break;
            }
            
            let path = &self.tree.visible_paths[row_index];
            let node = self.tree.nodes.get(path).unwrap();
            let depth = self.get_depth(path);
            
            let y = inner_area.y + i as u16;
            
            // Build styled spans for the row
            let mut spans = Vec::new();
            
            // Indentation
            spans.push(Span::raw("  ".repeat(depth)));
            
            // Expand arrow for directories
            if matches!(node.kind, FileKind::Directory { .. }) {
                let arrow = if node.expanded { "▼ " } else { "▶ " };
                spans.push(Span::styled(arrow, Style::default().fg(Color::DarkGray)));
            } else {
                spans.push(Span::raw("  "));
            }
            
            // Icon
            let icon = self.get_icon(node);
            let icon_color = self.get_icon_color(node);
            spans.push(Span::styled(format!("{} ", icon), Style::default().fg(icon_color)));
            
            // Filename
            let name_style = self.get_name_style(node, row_index == self.cursor);
            spans.push(Span::styled(&node.name, name_style));
            
            // Git status indicator
            if let Some(status) = &node.git_status {
                let (indicator, color) = self.get_git_indicator(status);
                spans.push(Span::styled(format!(" {}", indicator), Style::default().fg(color)));
            }
            
            // Mark indicator
            if node.marked {
                spans.push(Span::styled(" ◆", Style::default().fg(self.theme.color_marked)));
            }
            
            let line = Line::from(spans);
            
            // Background for cursor/selection
            let bg = if row_index == self.cursor {
                self.theme.color_cursor_bg
            } else if node.selected {
                self.theme.color_selected_bg
            } else {
                Color::Reset
            };
            
            let paragraph = Paragraph::new(line)
                .style(Style::default().bg(bg));
            
            frame.render_widget(
                paragraph,
                Rect::new(inner_area.x, y, inner_area.width, 1),
            );
        }
        
        // Render scrollbar if needed
        if self.tree.visible_paths.len() > inner_area.height as usize {
            self.render_scrollbar(frame, area);
        }
    }
}
```

---

## Commands

### Toggle and Configuration

```bash
# Toggle tree pane
tree toggle              # Show/hide
tree show                # Show
tree hide                # Hide

# Position
tree position left       # Left side (default)
tree position right      # Right side
tree position bottom     # Bottom
tree position float      # Floating overlay

# Size
tree width 30            # Set width (columns or percentage)
tree width 25%
tree height 40%          # For bottom position

# Behavior
tree follow on           # Follow shell cwd
tree follow off
tree hidden on           # Show hidden files
tree hidden off

# Focus
tree focus               # Focus tree pane (keyboard nav)
Ctrl+\                   # Default keybinding to toggle focus
```

### Tree-Specific Commands

```bash
# Navigate
tree cd ~/projects       # Change tree root
tree up                  # Go to parent directory
tree home                # Go to home directory

# Expand/Collapse
tree expand              # Expand selected
tree collapse            # Collapse selected
tree expand-all          # Expand all directories
tree collapse-all        # Collapse to root only

# Filter
tree filter "*.rs"       # Set filter
tree filter git:modified # Show only modified files
tree filter clear        # Clear filter

# Mark
tree mark                # Mark selected
tree unmark              # Unmark selected
tree mark-all            # Mark all visible
tree unmark-all          # Unmark all

# Operations on marked
tree copy /dest          # Copy marked to destination
tree move /dest          # Move marked to destination
tree delete              # Delete marked (with confirmation)

# Misc
tree refresh             # Refresh tree from filesystem
tree sync                # Sync tree cursor to shell cwd
```

---

## Configuration

```toml
[tree]
# Show tree pane on startup
enabled = true

# Default position
position = "left"        # left, right, bottom, float

# Size
width = "25%"            # For left/right
height = "30%"           # For bottom
min_width = 20           # Minimum columns

# Behavior
follow_cwd = true        # Auto-navigate when shell cd's
show_hidden = false      # Show dotfiles by default
auto_refresh = true      # Watch filesystem for changes
refresh_interval_ms = 1000

# Preview
preview_enabled = true
preview_position = "right"  # right, bottom, float
preview_max_size = "1mb"    # Max file size to preview

# Git integration
show_git_status = true
git_status_colors = true

# Icons (requires nerd font)
use_nerd_fonts = true

# Sorting
sort_by = "name"         # name, size, modified, type
sort_dirs_first = true
sort_case_sensitive = false

# Filter persistence
remember_filter = false  # Keep filter when changing directories

[tree.keybindings]
toggle = "ctrl+\\"
focus = "ctrl+t"
expand = "l"
collapse = "h"
open = "o"
edit = "e"
delete = "ctrl+d"
rename = "r"
new_file = "n"
new_directory = "N"
mark = "space"
filter = "/"
refresh = "R"
toggle_hidden = "."
```

---

## Future Enhancements

### Multi-Root Workspaces

Support multiple root directories in a single tree view:

```
┌─────────────────────────────────┐
│ WORKSPACE                       │
├─────────────────────────────────┤
│ 📁 frontend (~/projects/web)    │
│ ├── 📁 src                      │
│ └── 📄 package.json             │
│ 📁 backend (~/projects/api)     │
│ ├── 📁 src                      │
│ └── 📄 Cargo.toml               │
│ 📁 shared (~/projects/common)   │
│ └── 📁 types                    │
└─────────────────────────────────┘
```

### Remote Filesystems

Browse remote filesystems via SSH/SFTP:

```bash
tree remote add prod user@prod.example.com:/app
tree remote add staging user@staging.example.com:/app
```

### Bookmarks

Quick navigation to frequently used directories:

```bash
tree bookmark add myapp ~/projects/myapp
tree bookmark list
tree bookmark go myapp
```

### Integration with fzf/fuzzy finder

```bash
# Fuzzy find in tree
tree find              # Opens fuzzy finder scoped to tree

# Results update tree view in real-time as you type
```

### Tabs/Multiple Trees

Multiple tree views in tabs for different contexts:

```
┌────────┬────────┬────────┐
│ myapp  │ config │ notes  │
├────────┴────────┴────────┤
│ 📁 src                   │
│ ...                      │
```