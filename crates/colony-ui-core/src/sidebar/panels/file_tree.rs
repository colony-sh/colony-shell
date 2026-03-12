//! File tree sidebar panel

use crate::component::{
    Color, ComponentLine, SidebarComponent, SidebarOutput, Style, StyledSpan, TuplePattern,
};
use crate::context::ComponentContext;
use colony_filetree::{FileNode, GitFileStatus, TreeState};
use colony_tuple::Value;
use parking_lot::RwLock;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::warn;

/// File tree sidebar panel showing directory structure
/// Uses colony-filetree's TreeState for efficient state management
pub struct FileTreePanel {
    /// Root directory (if None, uses cwd)
    root: Option<PathBuf>,
    /// Tree state (lazy-initialized)
    tree_state: Arc<RwLock<Option<TreeState>>>,
    /// Current root path (for detecting cwd changes)
    current_root: Arc<RwLock<Option<PathBuf>>>,
}

impl FileTreePanel {
    pub fn new() -> Self {
        Self {
            root: None,
            tree_state: Arc::new(RwLock::new(None)),
            current_root: Arc::new(RwLock::new(None)),
        }
    }

    pub fn with_root(mut self, root: PathBuf) -> Self {
        self.root = Some(root);
        self
    }

    /// Get or initialize tree state for the given root
    fn ensure_tree_state(&self, root: &Path) -> Option<TreeState> {
        let mut current = self.current_root.write();
        let mut state = self.tree_state.write();

        // Check if we need to reinitialize (root changed)
        let needs_init = !matches!(current.as_ref(), Some(curr) if curr == root);

        if needs_init {
            let new_state = TreeState::new(root);
            match new_state.load() {
                Ok(()) => {
                    *state = Some(new_state.clone());
                    *current = Some(root.to_path_buf());
                    return Some(new_state);
                }
                Err(e) => {
                    warn!(
                        root = %root.display(),
                        error = %e,
                        "failed to load file tree state"
                    );
                }
            }
        }

        state.clone()
    }

    /// Render a node from tree state
    fn render_node(&self, node: &FileNode, tree: &TreeState, lines: &mut Vec<ComponentLine>) {
        let (icon, style) = if node.is_dir() {
            let dir_icon = if node.expanded { "\u{f07c} " } else { "\u{f07b} " };
            (dir_icon, Style::default().fg(Color::Blue).bold())
        } else {
            let (icon, color) = self.file_style(&node.extension);
            (icon, Style::default().fg(color))
        };

        // Add git status indicator
        let git_status = tree.git_status(&node.path);
        let (git_icon, git_color): (&str, Option<Color>) = match git_status {
            Some(GitFileStatus::Modified) => ("M", Some(Color::Yellow)),
            Some(GitFileStatus::Staged) => ("S", Some(Color::Green)),
            Some(GitFileStatus::Untracked) => ("?", Some(Color::BrightBlack)),
            Some(GitFileStatus::New) => ("+", Some(Color::Green)),
            Some(GitFileStatus::Deleted) => ("D", Some(Color::Red)),
            Some(GitFileStatus::Conflicted) => ("!", Some(Color::Red)),
            _ => ("", None),
        };

        // Build the line with name and optional git status
        let mut spans = vec![StyledSpan::new(node.name.clone(), style)];
        if !git_icon.is_empty() {
            if let Some(color) = git_color {
                spans.push(StyledSpan::new(
                    format!(" {}", git_icon),
                    Style::default().fg(color),
                ));
            }
        }

        let line = ComponentLine::new(spans)
            .with_indent(node.depth)
            .with_icon(icon)
            .with_interactive(
                format!("open:{}", node.path.display()),
                Some(node.path.display().to_string()),
            );

        let line = if node.is_dir() {
            line.expandable(node.expanded)
        } else {
            line
        };

        lines.push(line);
    }

    /// Get icon and color for file extension
    fn file_style(&self, ext: &Option<String>) -> (&'static str, Color) {
        match ext.as_deref() {
            Some("rs") => (" ", Color::Yellow),
            Some("toml" | "json" | "yaml" | "yml") => (" ", Color::Green),
            Some("md" | "txt") => (" ", Color::White),
            Some("sh" | "bash" | "zsh") => (" ", Color::Green),
            Some("py") => (" ", Color::Yellow),
            Some("js" | "ts") => (" ", Color::Yellow),
            Some("jsx" | "tsx") => (" ", Color::Cyan),
            Some("go") => (" ", Color::Cyan),
            Some("c" | "cpp" | "h" | "hpp") => (" ", Color::Blue),
            Some("html" | "htm") => (" ", Color::Magenta),
            Some("css" | "scss" | "sass") => (" ", Color::Blue),
            Some("lock") => (" ", Color::BrightBlack),
            _ => (" ", Color::White),
        }
    }

    /// Toggle expansion of a directory node by path
    pub fn toggle(&self, path: &Path) {
        if let Some(tree) = self.tree_state.read().as_ref() {
            let id = colony_filetree::FileNode::hash_path(path);
            tree.toggle_expanded(id);
        }
    }
}

impl Default for FileTreePanel {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for FileTreePanel {
    fn clone(&self) -> Self {
        Self {
            root: self.root.clone(),
            tree_state: self.tree_state.clone(),
            current_root: self.current_root.clone(),
        }
    }
}

impl SidebarComponent for FileTreePanel {
    fn id(&self) -> &str {
        "file_tree"
    }

    fn title(&self) -> &str {
        "Files"
    }

    fn default_width(&self) -> u16 {
        25
    }

    fn subscriptions(&self) -> Vec<TuplePattern> {
        vec![
            vec![
                Value::Str("dir_change".to_string()),
                Value::Wildcard,
                Value::Wildcard,
                Value::Wildcard,
            ],
            vec![Value::Str("file_change".to_string()), Value::Wildcard],
            // Subscribe to tree events for reactive updates
            vec![Value::Str("tree_event".to_string()), Value::Wildcard],
        ]
    }

    fn render_block(&self, ctx: &ComponentContext) -> SidebarOutput {
        let root = self.root.clone().unwrap_or_else(|| ctx.cwd.clone());
        let mut lines = Vec::new();

        // Add ".." entry to navigate to parent directory
        if root.parent().is_some() {
            lines.push(
                ComponentLine::new(vec![StyledSpan::new(
                    "..".to_string(),
                    Style::default().fg(Color::BrightBlack),
                )])
                .with_icon(" ")
                .with_interactive("cd_parent", None),
            );
        }

        // Get or initialize tree state
        if let Some(tree) = self.ensure_tree_state(&root) {
            // Get visible nodes (respects expansion state)
            for node in tree.visible_nodes() {
                self.render_node(&node, &tree, &mut lines);
            }
        } else {
            // Fallback: simple header if tree load failed
            let root_name = root
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| root.display().to_string());

            lines.push(
                ComponentLine::new(vec![StyledSpan::new(
                    format!("{} (failed to load)", root_name),
                    Style::default().fg(Color::Red),
                )])
                .with_icon(" "),
            );
        }

        SidebarOutput::visible(lines)
    }

    fn handle_action(&self, action: &str, _ctx: &ComponentContext) {
        if let Some(path_str) = action.strip_prefix("open:") {
            let path = Path::new(path_str);

            // Check if it's a directory - toggle expansion
            if path.is_dir() {
                self.toggle(path);
                tracing::debug!("File tree: toggled directory {}", path_str);
            } else {
                // For files, emit open event
                tracing::debug!("File tree action: open {}", path_str);
            }
        } else if let Some(path_str) = action.strip_prefix("toggle:") {
            let path = Path::new(path_str);
            self.toggle(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn test_context() -> ComponentContext {
        ComponentContext {
            cwd: std::env::current_dir().unwrap_or_default(),
            ..Default::default()
        }
    }

    fn test_context_with_dir(dir: &Path) -> ComponentContext {
        ComponentContext {
            cwd: dir.to_path_buf(),
            ..Default::default()
        }
    }

    #[test]
    fn test_file_tree_renders() {
        let panel = FileTreePanel::new();
        let ctx = test_context();
        let output = panel.render_block(&ctx);
        assert!(output.visible);
        assert!(!output.lines.is_empty());
    }

    #[test]
    fn test_file_tree_with_temp_dir() {
        let temp = TempDir::new().unwrap();

        // Create some test files
        fs::write(temp.path().join("main.rs"), "fn main() {}").unwrap();
        fs::write(temp.path().join("Cargo.toml"), "[package]").unwrap();
        fs::create_dir(temp.path().join("src")).unwrap();
        fs::write(temp.path().join("src/lib.rs"), "pub fn lib() {}").unwrap();

        let panel = FileTreePanel::new();
        let ctx = test_context_with_dir(temp.path());
        let output = panel.render_block(&ctx);

        assert!(output.visible);
        // Should have root + files
        assert!(output.lines.len() >= 3);

        // Check root is first
        assert!(output.lines[0].spans[0]
            .text
            .contains(temp.path().file_name().unwrap().to_str().unwrap()));
    }

    #[test]
    fn test_file_tree_toggle() {
        let temp = TempDir::new().unwrap();
        fs::create_dir(temp.path().join("src")).unwrap();
        fs::write(temp.path().join("src/lib.rs"), "pub fn lib() {}").unwrap();

        let panel = FileTreePanel::new();
        let ctx = test_context_with_dir(temp.path());

        // Initial render loads tree
        let _ = panel.render_block(&ctx);

        // Toggle src directory
        panel.toggle(&temp.path().join("src"));

        // Render again
        let output = panel.render_block(&ctx);
        assert!(output.visible);
    }
}
