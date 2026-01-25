//! File tree sidebar panel

use crate::component::{
    Color, ComponentLine, SidebarComponent, SidebarOutput, Style, StyledSpan, TuplePattern,
};
use crate::context::ComponentContext;
use colony_tuple::Value;
use std::path::PathBuf;

/// File tree sidebar panel showing directory structure
pub struct FileTreePanel {
    /// Root directory (if None, uses cwd)
    root: Option<PathBuf>,
    /// Maximum depth to display
    max_depth: usize,
    /// Show hidden files
    show_hidden: bool,
}

impl FileTreePanel {
    pub fn new() -> Self {
        Self {
            root: None,
            max_depth: 3,
            show_hidden: false,
        }
    }

    pub fn with_root(mut self, root: PathBuf) -> Self {
        self.root = Some(root);
        self
    }

    pub fn with_max_depth(mut self, depth: usize) -> Self {
        self.max_depth = depth;
        self
    }

    pub fn with_hidden(mut self, show: bool) -> Self {
        self.show_hidden = show;
        self
    }

    fn render_directory(&self, path: &PathBuf, depth: usize, lines: &mut Vec<ComponentLine>) {
        if depth > self.max_depth {
            return;
        }

        let entries = match std::fs::read_dir(path) {
            Ok(entries) => entries,
            Err(_) => return,
        };

        let mut items: Vec<_> = entries
            .filter_map(|e| e.ok())
            .filter(|e| {
                if self.show_hidden {
                    true
                } else {
                    !e.file_name().to_string_lossy().starts_with('.')
                }
            })
            .collect();

        // Sort: directories first, then alphabetically
        items.sort_by(|a, b| {
            let a_is_dir = a.file_type().map(|t| t.is_dir()).unwrap_or(false);
            let b_is_dir = b.file_type().map(|t| t.is_dir()).unwrap_or(false);
            match (a_is_dir, b_is_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.file_name().cmp(&b.file_name()),
            }
        });

        for entry in items {
            let name = entry.file_name().to_string_lossy().to_string();
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);

            let (icon, style) = if is_dir {
                (" ", Style::default().fg(Color::Blue).bold())
            } else {
                let path = entry.path();
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                let (icon, color) = match ext {
                    "rs" => (" ", Color::Yellow),
                    "toml" | "json" | "yaml" | "yml" => (" ", Color::Green),
                    "md" | "txt" => (" ", Color::White),
                    "sh" | "bash" | "zsh" => (" ", Color::Green),
                    "py" => (" ", Color::Yellow),
                    "js" | "ts" => (" ", Color::Yellow),
                    "go" => (" ", Color::Cyan),
                    "c" | "cpp" | "h" | "hpp" => (" ", Color::Blue),
                    _ => (" ", Color::White),
                };
                (icon, Style::default().fg(color))
            };

            let line = ComponentLine::new(vec![StyledSpan::new(name.clone(), style)])
                .with_indent(depth)
                .with_icon(icon)
                .with_interactive(
                    format!("open:{}", entry.path().display()),
                    Some(entry.path().display().to_string()),
                );

            let line = if is_dir {
                line.expandable(true) // Default expanded for first level
            } else {
                line
            };

            lines.push(line);

            // Recurse into directories (only first level by default)
            if is_dir && depth < 1 {
                self.render_directory(&entry.path(), depth + 1, lines);
            }
        }
    }
}

impl Default for FileTreePanel {
    fn default() -> Self {
        Self::new()
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
        ]
    }

    fn render_block(&self, ctx: &ComponentContext) -> SidebarOutput {
        let root = self.root.clone().unwrap_or_else(|| ctx.cwd.clone());
        let mut lines = Vec::new();

        // Add root directory header
        let root_name = root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| root.display().to_string());

        lines.push(
            ComponentLine::new(vec![StyledSpan::new(
                root_name,
                Style::default().fg(Color::Cyan).bold(),
            )])
            .with_icon(" ")
            .expandable(true),
        );

        // Render directory tree
        self.render_directory(&root, 0, &mut lines);

        SidebarOutput::visible(lines)
    }

    fn handle_action(&self, action: &str, _ctx: &ComponentContext) {
        if let Some(path) = action.strip_prefix("open:") {
            // In a real implementation, this would emit a tuple or callback
            tracing::debug!("File tree action: open {}", path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn test_context() -> ComponentContext {
        ComponentContext {
            cwd: std::env::current_dir().unwrap_or_default(),
            env: HashMap::new(),
            last_exit_code: 0,
            running_jobs: 0,
            stopped_jobs: 0,
            model: None,
            last_command_duration: None,
            hostname: None,
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
}
