# File Tree Protocol Integration Guide

This document describes how colony-terminal should integrate with the file tree protocol implemented in colony-shell (F028 + F029).

## Overview

The file tree is a **shell-owned, terminal-rendered** component:
- **Shell** owns the tree state, watches for filesystem changes, tracks git status
- **Terminal** renders the tree UI and sends user actions back to shell
- Communication happens via **OSC 2337** escape sequences with JSON payloads

## Protocol Messages

All messages are JSON wrapped in OSC 2337: `\x1b]2337;<json>\x07`

### Shell → Terminal Messages

#### 1. TreeSnapshot (Full State)

Sent on initial load or after major changes (e.g., `cd` to new directory).

```json
{
  "type": "TreeSnapshot",
  "root": "/Users/user/project",
  "nodes": [
    {
      "id": 12345678901234,
      "name": "project",
      "path": "",
      "node_type": "directory",
      "expanded": true,
      "children": [98765432109876, 11111111111111],
      "depth": 0
    },
    {
      "id": 98765432109876,
      "name": "src",
      "path": "src",
      "node_type": "directory",
      "expanded": false,
      "children": [],
      "depth": 1
    },
    {
      "id": 11111111111111,
      "name": "main.rs",
      "path": "main.rs",
      "node_type": "file",
      "depth": 1,
      "size": 1234,
      "git_status": "modified"
    }
  ],
  "git_branch": "main",
  "git_dirty": true,
  "git_ahead": 2,
  "git_behind": 0
}
```

**Fields:**
- `root`: Absolute path to tree root
- `nodes`: Flattened list of visible nodes (respects expansion state)
- `git_branch`: Current branch name (null if not a git repo)
- `git_dirty`: Has uncommitted changes
- `git_ahead`/`git_behind`: Commits ahead/behind upstream

#### 2. TreeUpdate (Incremental)

Sent when individual nodes change.

**NodeAdded:**
```json
{
  "type": "TreeUpdate",
  "kind": "node_added",
  "node": { /* TreeNode */ },
  "parent_id": 12345678901234,
  "index": 2
}
```

**NodeRemoved:**
```json
{
  "type": "TreeUpdate",
  "kind": "node_removed",
  "id": 98765432109876
}
```

**NodeModified:**
```json
{
  "type": "TreeUpdate",
  "kind": "node_modified",
  "id": 11111111111111,
  "size": 2048,
  "git_status": "staged"
}
```

**NodeToggled:**
```json
{
  "type": "TreeUpdate",
  "kind": "node_toggled",
  "id": 98765432109876,
  "expanded": true,
  "children": [
    { /* TreeNode for child 1 */ },
    { /* TreeNode for child 2 */ }
  ]
}
```

**GitStatusUpdate:**
```json
{
  "type": "TreeUpdate",
  "kind": "git_status_update",
  "updates": [
    [11111111111111, "staged"],
    [22222222222222, "modified"],
    [33333333333333, null]
  ]
}
```

**Reload:**
```json
{
  "type": "TreeUpdate",
  "kind": "reload"
}
```
Terminal should request a new snapshot when receiving this.

#### 3. TreeActionResult

Response to user actions.

**Success:**
```json
{
  "type": "TreeActionResult",
  "success": true,
  "data": {
    "id": 44444444444444,
    "path": "/Users/user/project/src/new_file.rs"
  }
}
```

**Error:**
```json
{
  "type": "TreeActionResult",
  "success": false,
  "error": "Permission denied"
}
```

### Terminal → Shell Messages

#### TreeAction

All user interactions are sent as TreeAction messages.

**Toggle (expand/collapse directory):**
```json
{
  "type": "TreeAction",
  "action": "toggle",
  "id": 98765432109876
}
```

**Expand:**
```json
{
  "type": "TreeAction",
  "action": "expand",
  "id": 98765432109876
}
```

**Collapse:**
```json
{
  "type": "TreeAction",
  "action": "collapse",
  "id": 98765432109876
}
```

**Open (file or directory):**
```json
{
  "type": "TreeAction",
  "action": "open",
  "path": "src/main.rs"
}
```

**Select (highlight without opening):**
```json
{
  "type": "TreeAction",
  "action": "select",
  "id": 11111111111111
}
```

**Create File:**
```json
{
  "type": "TreeAction",
  "action": "create_file",
  "parent_id": 98765432109876,
  "name": "new_file.rs"
}
```

**Create Directory:**
```json
{
  "type": "TreeAction",
  "action": "create_directory",
  "parent_id": 98765432109876,
  "name": "new_folder"
}
```

**Rename:**
```json
{
  "type": "TreeAction",
  "action": "rename",
  "id": 11111111111111,
  "new_name": "renamed_file.rs"
}
```

**Delete:**
```json
{
  "type": "TreeAction",
  "action": "delete",
  "id": 11111111111111,
  "force": false
}
```
Set `force: true` for recursive directory deletion.

**Copy:**
```json
{
  "type": "TreeAction",
  "action": "copy",
  "id": 11111111111111,
  "dest_parent_id": 98765432109876
}
```

**Move:**
```json
{
  "type": "TreeAction",
  "action": "move",
  "id": 11111111111111,
  "dest_parent_id": 98765432109876
}
```

**Reveal (expand to show a path):**
```json
{
  "type": "TreeAction",
  "action": "reveal",
  "path": "src/deeply/nested/file.rs"
}
```

**Refresh:**
```json
{
  "type": "TreeAction",
  "action": "refresh"
}
```

**Request Snapshot:**
```json
{
  "type": "TreeAction",
  "action": "request_snapshot"
}
```
Send this on terminal startup to get initial tree state.

## Data Types

### TreeNode

```typescript
interface TreeNode {
  id: number;           // u64 - hash of full path
  name: string;         // File/directory name
  path: string;         // Relative path from root
  node_type: "file" | "directory" | "symlink" | "other";
  expanded?: boolean;   // Only for directories
  children?: number[];  // Child node IDs (only for directories)
  depth: number;        // 0 = root
  size?: number;        // Bytes (only for files)
  git_status?: GitStatus;
}
```

### GitStatus

```typescript
type GitStatus = 
  | "untracked"   // ? - new file not in git
  | "new"         // + - staged new file
  | "modified"    // M - modified (unstaged)
  | "staged"      // S - modified (staged)
  | "deleted"     // D - deleted
  | "renamed"     // R - renamed
  | "conflicted"  // ! - merge conflict
  | "ignored";    // I - in .gitignore
```

## Terminal Implementation Guide

### 1. State Management

```rust
struct FileTreeState {
    // Core state
    nodes: HashMap<NodeId, TreeNode>,
    root_id: Option<NodeId>,
    
    // Render state
    visible_order: Vec<NodeId>,  // Computed from nodes + expansion
    selected: Option<NodeId>,
    scroll_offset: usize,
    
    // Git info
    git_branch: Option<String>,
    git_dirty: bool,
    git_ahead: u32,
    git_behind: u32,
}

impl FileTreeState {
    fn apply_snapshot(&mut self, snapshot: TreeSnapshot) {
        self.nodes.clear();
        for node in snapshot.nodes {
            self.nodes.insert(node.id, node);
        }
        self.git_branch = snapshot.git_branch;
        self.git_dirty = snapshot.git_dirty;
        self.git_ahead = snapshot.git_ahead;
        self.git_behind = snapshot.git_behind;
        self.rebuild_visible_order();
    }
    
    fn apply_update(&mut self, update: TreeUpdate) {
        match update {
            TreeUpdate::NodeAdded { node, parent_id, index } => {
                let id = node.id;
                self.nodes.insert(id, node);
                if let Some(pid) = parent_id {
                    if let Some(parent) = self.nodes.get_mut(&pid) {
                        parent.children.insert(index, id);
                    }
                }
                self.rebuild_visible_order();
            }
            TreeUpdate::NodeRemoved { id } => {
                self.remove_node_recursive(id);
                self.rebuild_visible_order();
            }
            TreeUpdate::NodeModified { id, size, git_status } => {
                if let Some(node) = self.nodes.get_mut(&id) {
                    if let Some(s) = size { node.size = Some(s); }
                    node.git_status = git_status;
                }
            }
            TreeUpdate::NodeToggled { id, expanded, children } => {
                if let Some(node) = self.nodes.get_mut(&id) {
                    node.expanded = expanded;
                    if expanded {
                        // Add children to nodes map
                        for child in children {
                            self.nodes.insert(child.id, child);
                        }
                    }
                }
                self.rebuild_visible_order();
            }
            TreeUpdate::GitStatusUpdate { updates } => {
                for (id, status) in updates {
                    if let Some(node) = self.nodes.get_mut(&id) {
                        node.git_status = status;
                    }
                }
            }
            TreeUpdate::Reload => {
                // Request new snapshot
                send_tree_action(TreeAction::RequestSnapshot);
            }
        }
    }
    
    fn rebuild_visible_order(&mut self) {
        self.visible_order.clear();
        if let Some(root_id) = self.root_id {
            self.collect_visible(root_id);
        }
    }
    
    fn collect_visible(&mut self, id: NodeId) {
        self.visible_order.push(id);
        if let Some(node) = self.nodes.get(&id) {
            if node.expanded {
                for child_id in &node.children {
                    self.collect_visible(*child_id);
                }
            }
        }
    }
}
```

### 2. Rendering

```rust
fn render_file_tree(state: &FileTreeState, area: Rect, buf: &mut Buffer) {
    // Header with git info
    if let Some(branch) = &state.git_branch {
        let status = if state.git_dirty { "*" } else { "" };
        let ahead_behind = format_ahead_behind(state.git_ahead, state.git_behind);
        render_line(format!(" {} {}{}", branch, status, ahead_behind));
    }
    
    // Tree nodes
    for (i, &id) in state.visible_order.iter().enumerate() {
        if i < state.scroll_offset { continue; }
        if i >= state.scroll_offset + area.height { break; }
        
        if let Some(node) = state.nodes.get(&id) {
            let indent = "  ".repeat(node.depth as usize);
            let icon = get_icon(node);
            let git_indicator = get_git_indicator(node.git_status);
            let selected = state.selected == Some(id);
            
            render_tree_line(indent, icon, &node.name, git_indicator, selected);
        }
    }
}

fn get_icon(node: &TreeNode) -> &'static str {
    match node.node_type {
        TreeNodeType::Directory => {
            if node.expanded { "" } else { "" }
        }
        TreeNodeType::File => get_file_icon(&node.name),
        TreeNodeType::Symlink => "",
        TreeNodeType::Other => "",
    }
}

fn get_git_indicator(status: Option<GitStatus>) -> (&'static str, Color) {
    match status {
        Some(GitStatus::Modified) => ("M", Color::Yellow),
        Some(GitStatus::Staged) => ("S", Color::Green),
        Some(GitStatus::Untracked) => ("?", Color::Gray),
        Some(GitStatus::New) => ("+", Color::Green),
        Some(GitStatus::Deleted) => ("D", Color::Red),
        Some(GitStatus::Conflicted) => ("!", Color::Red),
        _ => ("", Color::Default),
    }
}
```

### 3. Input Handling

```rust
fn handle_file_tree_input(key: KeyEvent, state: &mut FileTreeState) {
    match key.code {
        // Navigation
        KeyCode::Char('j') | KeyCode::Down => {
            state.move_selection(1);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            state.move_selection(-1);
        }
        KeyCode::Char('g') => {
            state.select_first();
        }
        KeyCode::Char('G') => {
            state.select_last();
        }
        
        // Expand/Collapse
        KeyCode::Enter | KeyCode::Char('l') => {
            if let Some(id) = state.selected {
                if let Some(node) = state.nodes.get(&id) {
                    if node.node_type == TreeNodeType::Directory {
                        send_tree_action(TreeAction::Toggle { id });
                    } else {
                        send_tree_action(TreeAction::Open { 
                            path: node.path.clone() 
                        });
                    }
                }
            }
        }
        KeyCode::Char('h') => {
            if let Some(id) = state.selected {
                send_tree_action(TreeAction::Collapse { id });
            }
        }
        
        // File operations
        KeyCode::Char('a') => {
            if let Some(id) = state.get_parent_directory() {
                let name = prompt_input("File name: ");
                send_tree_action(TreeAction::CreateFile { 
                    parent_id: id, 
                    name 
                });
            }
        }
        KeyCode::Char('A') => {
            if let Some(id) = state.get_parent_directory() {
                let name = prompt_input("Directory name: ");
                send_tree_action(TreeAction::CreateDirectory { 
                    parent_id: id, 
                    name 
                });
            }
        }
        KeyCode::Char('r') => {
            if let Some(id) = state.selected {
                let new_name = prompt_input("New name: ");
                send_tree_action(TreeAction::Rename { id, new_name });
            }
        }
        KeyCode::Char('d') => {
            if let Some(id) = state.selected {
                if confirm("Delete? (y/n)") {
                    send_tree_action(TreeAction::Delete { id, force: false });
                }
            }
        }
        KeyCode::Char('D') => {
            if let Some(id) = state.selected {
                if confirm("Force delete (recursive)? (y/n)") {
                    send_tree_action(TreeAction::Delete { id, force: true });
                }
            }
        }
        
        // Refresh
        KeyCode::Char('R') => {
            send_tree_action(TreeAction::Refresh);
        }
        
        _ => {}
    }
}
```

### 4. Startup Sequence

```rust
fn init_file_tree() {
    // Request initial tree state from shell
    send_tree_action(TreeAction::RequestSnapshot);
}
```

## Message Size Considerations

- **Small updates** (< 4KB): Safe to send via OSC 2337
- **Large snapshots**: The `TreeSnapshot::fits_in_osc()` method estimates size
  - If too large, shell may send in chunks or use a side channel (future enhancement)
  - For now, assume snapshots fit (typical projects have < 1000 visible nodes)

## Error Handling

1. **TreeActionResult errors**: Display in status bar or notification
2. **Missing nodes**: Log warning, request refresh
3. **Parse errors**: Log and ignore malformed messages

## Testing

To test without the full shell:

```rust
// Simulate shell sending a snapshot
let test_snapshot = r#"{
  "type": "TreeSnapshot",
  "root": "/test",
  "nodes": [
    {"id": 1, "name": "test", "path": "", "node_type": "directory", "expanded": true, "depth": 0}
  ]
}"#;

// Parse and apply
let msg: ProtocolMessage = serde_json::from_str(test_snapshot)?;
if let ProtocolMessage::TreeSnapshot(snapshot) = msg {
    state.apply_snapshot(snapshot);
}
```

## Reference Implementation

See colony-shell crates for reference:
- `colony-protocol/src/tree.rs` - Protocol message definitions
- `colony-filetree/src/protocol.rs` - TreeProtocol bridge (shell side)
- `colony-filetree/src/state.rs` - TreeState management
- `colony-ui-core/src/sidebar/panels/file_tree.rs` - FileTreePanel component
