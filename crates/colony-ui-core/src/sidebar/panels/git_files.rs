//! Git modified files sidebar panel

use crate::component::{
    Color, ComponentLine, SidebarComponent, SidebarOutput, Style, StyledSpan, TuplePattern,
};
use crate::context::ComponentContext;
use colony_tuple::Value;
use std::process::Command;

/// Git file status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitFileStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
    Staged,
    Conflict,
}

/// Information about a modified file
#[derive(Debug, Clone)]
pub struct GitFileInfo {
    pub path: String,
    pub status: GitFileStatus,
    pub staged: bool,
    pub additions: Option<u32>,
    pub deletions: Option<u32>,
}

/// Git modified files sidebar panel
pub struct GitFilesPanel {
    /// Show untracked files
    show_untracked: bool,
    /// Show diff stats
    show_stats: bool,
}

impl GitFilesPanel {
    pub fn new() -> Self {
        Self {
            show_untracked: true,
            show_stats: true,
        }
    }

    pub fn with_untracked(mut self, show: bool) -> Self {
        self.show_untracked = show;
        self
    }

    pub fn with_stats(mut self, show: bool) -> Self {
        self.show_stats = show;
        self
    }

    /// Get modified files from git
    fn get_modified_files(&self, cwd: &std::path::Path) -> Vec<GitFileInfo> {
        let mut files = Vec::new();

        // Get status with porcelain format
        let output = Command::new("git")
            .args(["status", "--porcelain", "-uall"])
            .current_dir(cwd)
            .output();

        let output = match output {
            Ok(o) if o.status.success() => o,
            _ => return files,
        };

        let stdout = String::from_utf8_lossy(&output.stdout);

        for line in stdout.lines() {
            if line.len() < 4 {
                continue;
            }

            let staged_status = line.chars().next().unwrap_or(' ');
            let unstaged_status = line.chars().nth(1).unwrap_or(' ');
            let path = line[3..].to_string();

            let (status, staged) = match (staged_status, unstaged_status) {
                ('M', _) => (GitFileStatus::Modified, true),
                (_, 'M') => (GitFileStatus::Modified, false),
                ('A', _) => (GitFileStatus::Added, true),
                ('D', _) | (_, 'D') => (GitFileStatus::Deleted, staged_status == 'D'),
                ('R', _) => (GitFileStatus::Renamed, true),
                ('?', '?') => {
                    if self.show_untracked {
                        (GitFileStatus::Untracked, false)
                    } else {
                        continue;
                    }
                }
                ('U', _) | (_, 'U') => (GitFileStatus::Conflict, false),
                _ => continue,
            };

            files.push(GitFileInfo {
                path,
                status,
                staged,
                additions: None,
                deletions: None,
            });
        }

        // Get diff stats if enabled
        if self.show_stats && !files.is_empty() {
            if let Ok(output) = Command::new("git")
                .args(["diff", "--numstat"])
                .current_dir(cwd)
                .output()
            {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    let parts: Vec<&str> = line.split('\t').collect();
                    if parts.len() >= 3 {
                        let additions = parts[0].parse().ok();
                        let deletions = parts[1].parse().ok();
                        let path = parts[2];

                        if let Some(file) = files.iter_mut().find(|f| f.path == path) {
                            file.additions = additions;
                            file.deletions = deletions;
                        }
                    }
                }
            }
        }

        files
    }
}

impl Default for GitFilesPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl SidebarComponent for GitFilesPanel {
    fn id(&self) -> &str {
        "git_files"
    }

    fn title(&self) -> &str {
        "Changes"
    }

    fn default_width(&self) -> u16 {
        30
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
            vec![Value::Str("git_change".to_string()), Value::Wildcard],
        ]
    }

    fn render_block(&self, ctx: &ComponentContext) -> SidebarOutput {
        let mut lines = Vec::new();

        let files = self.get_modified_files(&ctx.cwd);

        if files.is_empty() {
            lines.push(ComponentLine::new(vec![StyledSpan::new(
                "No changes",
                Style::default().dim().italic(),
            )]));
            return SidebarOutput::visible(lines);
        }

        // Group by staged/unstaged
        let staged: Vec<_> = files.iter().filter(|f| f.staged).collect();
        let unstaged: Vec<_> = files.iter().filter(|f| !f.staged).collect();

        // Staged changes
        if !staged.is_empty() {
            lines.push(ComponentLine::new(vec![StyledSpan::new(
                format!("Staged ({})", staged.len()),
                Style::default().fg(Color::Green).bold(),
            )]));

            for file in staged {
                lines.push(self.render_file(file));
            }

            if !unstaged.is_empty() {
                lines.push(ComponentLine::plain(""));
            }
        }

        // Unstaged changes
        if !unstaged.is_empty() {
            lines.push(ComponentLine::new(vec![StyledSpan::new(
                format!("Modified ({})", unstaged.len()),
                Style::default().fg(Color::Yellow).bold(),
            )]));

            for file in unstaged {
                lines.push(self.render_file(file));
            }
        }

        SidebarOutput::visible(lines)
    }

    fn handle_action(&self, action: &str, _ctx: &ComponentContext) {
        if let Some(path) = action.strip_prefix("stage:") {
            tracing::debug!("Git files action: stage {}", path);
        } else if let Some(path) = action.strip_prefix("unstage:") {
            tracing::debug!("Git files action: unstage {}", path);
        } else if let Some(path) = action.strip_prefix("diff:") {
            tracing::debug!("Git files action: diff {}", path);
        }
    }
}

impl GitFilesPanel {
    fn render_file(&self, file: &GitFileInfo) -> ComponentLine {
        let (icon, color) = match file.status {
            GitFileStatus::Modified => (" ", Color::Yellow),
            GitFileStatus::Added => (" ", Color::Green),
            GitFileStatus::Deleted => (" ", Color::Red),
            GitFileStatus::Renamed => (" ", Color::Blue),
            GitFileStatus::Untracked => (" ", Color::BrightBlack),
            GitFileStatus::Staged => (" ", Color::Green),
            GitFileStatus::Conflict => (" ", Color::Red),
        };

        let mut spans = vec![StyledSpan::new(
            file.path.clone(),
            Style::default().fg(color),
        )];

        // Add diff stats if available
        if let (Some(add), Some(del)) = (file.additions, file.deletions) {
            spans.push(StyledSpan::new(" ", Style::default()));
            if add > 0 {
                spans.push(StyledSpan::new(
                    format!("+{}", add),
                    Style::default().fg(Color::Green),
                ));
            }
            if del > 0 {
                if add > 0 {
                    spans.push(StyledSpan::new(" ", Style::default()));
                }
                spans.push(StyledSpan::new(
                    format!("-{}", del),
                    Style::default().fg(Color::Red),
                ));
            }
        }

        let action = if file.staged {
            format!("unstage:{}", file.path)
        } else {
            format!("stage:{}", file.path)
        };

        ComponentLine::new(spans)
            .with_icon(icon)
            .with_indent(1)
            .with_interactive(action, Some(format!("diff:{}", file.path)))
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
    fn test_git_files_renders() {
        let panel = GitFilesPanel::new();
        let ctx = test_context();
        let output = panel.render_block(&ctx);
        assert!(output.visible);
    }
}
