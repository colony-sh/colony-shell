use crate::component::{
    SidebarComponent, SidebarLine, SidebarOutput, TuplePattern,
};
use crate::context::ComponentContext;
use crate::{Color, Style, StyledSpan};
use colony_tuple::Value;
use std::fs;
use std::path::Path;

pub struct FileTreeSidebarComponent {
    max_depth: usize,
    max_entries: usize,
}

impl FileTreeSidebarComponent {
    pub fn new() -> Self {
        Self {
            max_depth: 3,
            max_entries: 50,
        }
    }

    pub fn with_limits(max_depth: usize, max_entries: usize) -> Self {
        Self {
            max_depth,
            max_entries,
        }
    }

    fn walk_directory(
        &self,
        path: &Path,
        depth: usize,
        lines: &mut Vec<SidebarLine>,
    ) {
        if depth > self.max_depth || lines.len() >= self.max_entries {
            return;
        }

        let Ok(entries) = fs::read_dir(path) else {
            return;
        };

        let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| {
            let is_dir = e.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
            // Directories first, then alphabetical
            (!is_dir, e.file_name())
        });

        for entry in entries {
            if lines.len() >= self.max_entries {
                lines.push(
                    SidebarLine::new(StyledSpan::new(
                        "...",
                        Style::default().fg(Color::BrightBlack),
                    ))
                    .with_indent(depth as u16),
                );
                break;
            }

            let name = entry.file_name().to_string_lossy().to_string();

            // Skip hidden files at depth 0, always skip .git
            if name.starts_with('.') {
                if name == ".git" {
                    continue;
                }
                if depth == 0 {
                    continue;
                }
            }

            let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);

            let (icon_text, icon_color) = if is_dir {
                ("\u{f115}", Color::Cyan) // folder icon
            } else {
                file_icon(&name)
            };

            let line = SidebarLine::new(StyledSpan::new(
                name,
                if is_dir {
                    Style::default().fg(Color::Cyan).bold()
                } else {
                    Style::default()
                },
            ))
            .with_icon(StyledSpan::new(icon_text, Style::default().fg(icon_color)))
            .with_indent(depth as u16);

            lines.push(line);

            if is_dir {
                self.walk_directory(&entry.path(), depth + 1, lines);
            }
        }
    }
}

impl Default for FileTreeSidebarComponent {
    fn default() -> Self {
        Self::new()
    }
}

fn file_icon(name: &str) -> (&'static str, Color) {
    let ext = name.rsplit('.').next().unwrap_or("");
    let icon = colony_filetree::file_icon_for_ext(ext);
    let color = match ext {
        "rs" => Color::Red,
        "toml" | "lock" => Color::BrightBlack,
        "md" | "markdown" | "py" => Color::Blue,
        "json" | "js" | "ts" | "jsx" | "tsx" => Color::Yellow,
        "go" => Color::Cyan,
        "sh" | "bash" | "zsh" => Color::Green,
        "yml" | "yaml" => Color::Magenta,
        _ => Color::BrightBlack,
    };
    (icon, color)
}

impl SidebarComponent for FileTreeSidebarComponent {
    fn id(&self) -> &str {
        "file_tree"
    }

    fn title(&self) -> &str {
        "Files"
    }

    fn subscriptions(&self) -> Vec<TuplePattern> {
        vec![
            vec![
                Value::Str("dir_change".to_string()),
                Value::Wildcard,
                Value::Wildcard,
            ],
            vec![Value::Str("file_change".to_string()), Value::Wildcard],
        ]
    }

    fn render_block(&self, ctx: &ComponentContext) -> SidebarOutput {
        let mut lines = Vec::with_capacity(self.max_entries);
        self.walk_directory(&ctx.cwd, 0, &mut lines);

        if lines.is_empty() {
            return SidebarOutput::hidden();
        }

        SidebarOutput::from_sidebar_lines_with_data(
            lines,
            serde_json::json!({
                "cwd": ctx.cwd.to_string_lossy(),
            }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn test_context(cwd: &str) -> ComponentContext {
        ComponentContext {
            cwd: std::path::PathBuf::from(cwd),
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
    fn test_file_tree_id() {
        let component = FileTreeSidebarComponent::new();
        assert_eq!(component.id(), "file_tree");
    }

    #[test]
    fn test_file_tree_title() {
        let component = FileTreeSidebarComponent::new();
        assert_eq!(component.title(), "Files");
    }

    #[test]
    fn test_file_tree_position() {
        let component = FileTreeSidebarComponent::new();
        assert_eq!(component.position(), SidebarPosition::Left);
    }

    #[test]
    fn test_file_tree_collapsible() {
        let component = FileTreeSidebarComponent::new();
        assert!(component.collapsible());
    }

    #[test]
    fn test_file_tree_renders_current_dir() {
        let component = FileTreeSidebarComponent::new();
        let ctx = test_context("/tmp");
        let output = component.render(&ctx);
        // /tmp should have some files; output may or may not be visible
        // depending on the system, but the function should not panic
        assert_eq!(output.visible, !output.lines.is_empty());
    }

    #[test]
    fn test_file_tree_nonexistent_dir() {
        let component = FileTreeSidebarComponent::new();
        let ctx = test_context("/nonexistent_dir_12345");
        let output = component.render(&ctx);
        assert!(!output.visible);
    }

    #[test]
    fn test_file_icon_rust() {
        let (icon, color) = file_icon("main.rs");
        assert_eq!(icon, "\u{e7a8}");
        assert_eq!(color, Color::Red);
    }

    #[test]
    fn test_file_icon_unknown() {
        let (icon, color) = file_icon("readme.xyz");
        assert_eq!(icon, "\u{f15b}");
        assert_eq!(color, Color::BrightBlack);
    }

    #[test]
    fn test_with_limits() {
        let component = FileTreeSidebarComponent::with_limits(1, 5);
        assert_eq!(component.max_depth, 1);
        assert_eq!(component.max_entries, 5);
    }
}
