use crate::component::{
    SidebarComponent, SidebarLine, SidebarOutput, SidebarPosition, TuplePattern,
};
use crate::context::ComponentContext;
use crate::{Color, Style, StyledSpan};
use colony_tuple::Value;
use parking_lot::Mutex;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

/// A git-modified file with its status
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitFileEntry {
    pub path: String,
    pub status: GitFileStatus,
    pub staged: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitFileStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
}

impl GitFileStatus {
    fn indicator(&self) -> &'static str {
        match self {
            GitFileStatus::Modified => "M",
            GitFileStatus::Added => "A",
            GitFileStatus::Deleted => "D",
            GitFileStatus::Renamed => "R",
            GitFileStatus::Untracked => "?",
        }
    }

    fn color(&self) -> Color {
        match self {
            GitFileStatus::Modified => Color::Yellow,
            GitFileStatus::Added => Color::Green,
            GitFileStatus::Deleted => Color::Red,
            GitFileStatus::Renamed => Color::Blue,
            GitFileStatus::Untracked => Color::BrightBlack,
        }
    }
}

#[derive(Clone, Debug)]
struct GitFilesCache {
    entries: Vec<GitFileEntry>,
    cwd: PathBuf,
    fetched_at: Instant,
}

pub struct GitFilesSidebarComponent {
    cache_ttl: Duration,
    cache: Mutex<Option<GitFilesCache>>,
}

impl GitFilesSidebarComponent {
    pub fn new() -> Self {
        Self {
            cache_ttl: Duration::from_millis(500),
            cache: Mutex::new(None),
        }
    }

    pub fn with_cache_ttl(ttl: Duration) -> Self {
        Self {
            cache_ttl: ttl,
            cache: Mutex::new(None),
        }
    }

    pub fn invalidate_cache(&self) {
        *self.cache.lock() = None;
    }

    fn fetch_files(cwd: &std::path::Path) -> Option<Vec<GitFileEntry>> {
        let output = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(cwd)
            .output();

        let Ok(out) = output else {
            return None;
        };

        if !out.status.success() {
            return None;
        }

        let stdout = String::from_utf8_lossy(&out.stdout);
        Some(parse_git_files(&stdout))
    }
}

impl Default for GitFilesSidebarComponent {
    fn default() -> Self {
        Self::new()
    }
}

pub fn parse_git_files(porcelain: &str) -> Vec<GitFileEntry> {
    let mut entries = Vec::new();

    for line in porcelain.lines() {
        if line.len() < 4 {
            continue;
        }

        let index = line.as_bytes()[0];
        let worktree = line.as_bytes()[1];
        let path = line[3..].to_string();

        // Staged changes (index column)
        match index {
            b'M' => entries.push(GitFileEntry {
                path: path.clone(),
                status: GitFileStatus::Modified,
                staged: true,
            }),
            b'A' => entries.push(GitFileEntry {
                path: path.clone(),
                status: GitFileStatus::Added,
                staged: true,
            }),
            b'D' => entries.push(GitFileEntry {
                path: path.clone(),
                status: GitFileStatus::Deleted,
                staged: true,
            }),
            b'R' => entries.push(GitFileEntry {
                path: path.clone(),
                status: GitFileStatus::Renamed,
                staged: true,
            }),
            _ => {}
        }

        // Unstaged changes (worktree column)
        match worktree {
            b'M' => entries.push(GitFileEntry {
                path: path.clone(),
                status: GitFileStatus::Modified,
                staged: false,
            }),
            b'D' => entries.push(GitFileEntry {
                path,
                status: GitFileStatus::Deleted,
                staged: false,
            }),
            b'?' if index == b'?' => entries.push(GitFileEntry {
                path,
                status: GitFileStatus::Untracked,
                staged: false,
            }),
            _ => {}
        }
    }

    entries
}

impl SidebarComponent for GitFilesSidebarComponent {
    fn id(&self) -> &str {
        "git_files"
    }

    fn title(&self) -> &str {
        "Git Files"
    }

    fn position(&self) -> SidebarPosition {
        SidebarPosition::Right
    }

    fn subscriptions(&self) -> Vec<TuplePattern> {
        vec![
            vec![Value::Str("file_change".to_string()), Value::Wildcard],
            vec![
                Value::Str("dir_change".to_string()),
                Value::Wildcard,
                Value::Wildcard,
            ],
            vec![Value::Str("cmd_complete".to_string())],
        ]
    }

    fn render(&self, ctx: &ComponentContext) -> SidebarOutput {
        // Check cache
        {
            let cache = self.cache.lock();
            if let Some(ref cached) = *cache {
                if cached.cwd == ctx.cwd && cached.fetched_at.elapsed() < self.cache_ttl {
                    return render_file_list(&cached.entries);
                }
            }
        }

        let Some(entries) = Self::fetch_files(&ctx.cwd) else {
            *self.cache.lock() = None;
            return SidebarOutput::hidden();
        };

        let output = render_file_list(&entries);

        *self.cache.lock() = Some(GitFilesCache {
            entries,
            cwd: ctx.cwd.clone(),
            fetched_at: Instant::now(),
        });

        output
    }
}

fn render_file_list(entries: &[GitFileEntry]) -> SidebarOutput {
    if entries.is_empty() {
        return SidebarOutput::visible(vec![SidebarLine::new(StyledSpan::new(
            "Working tree clean",
            Style::default().fg(Color::Green).italic(),
        ))]);
    }

    let mut lines = Vec::new();

    // Staged section
    let staged: Vec<_> = entries.iter().filter(|e| e.staged).collect();
    if !staged.is_empty() {
        lines.push(SidebarLine::new(StyledSpan::new(
            format!("Staged ({})", staged.len()),
            Style::default().fg(Color::Green).bold(),
        )));

        for entry in &staged {
            lines.push(
                SidebarLine::new(StyledSpan::new(
                    entry.path.clone(),
                    Style::default(),
                ))
                .with_icon(StyledSpan::new(
                    entry.status.indicator(),
                    Style::default().fg(entry.status.color()),
                ))
                .with_indent(1),
            );
        }
    }

    // Unstaged section
    let unstaged: Vec<_> = entries.iter().filter(|e| !e.staged).collect();
    if !unstaged.is_empty() {
        // Add a blank separator if there were staged entries
        if !staged.is_empty() {
            lines.push(SidebarLine::new(StyledSpan::plain("")));
        }

        lines.push(SidebarLine::new(StyledSpan::new(
            format!("Unstaged ({})", unstaged.len()),
            Style::default().fg(Color::Yellow).bold(),
        )));

        for entry in &unstaged {
            lines.push(
                SidebarLine::new(StyledSpan::new(
                    entry.path.clone(),
                    Style::default(),
                ))
                .with_icon(StyledSpan::new(
                    entry.status.indicator(),
                    Style::default().fg(entry.status.color()),
                ))
                .with_indent(1),
            );
        }
    }

    let staged_count = staged.len();
    let unstaged_count = unstaged.len();

    SidebarOutput::with_data(
        lines,
        serde_json::json!({
            "staged": staged_count,
            "unstaged": unstaged_count,
            "total": entries.len(),
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_files_id() {
        let component = GitFilesSidebarComponent::new();
        assert_eq!(component.id(), "git_files");
    }

    #[test]
    fn test_git_files_title() {
        let component = GitFilesSidebarComponent::new();
        assert_eq!(component.title(), "Git Files");
    }

    #[test]
    fn test_parse_git_files_modified() {
        let output = " M src/main.rs\n";
        let entries = parse_git_files(output);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "src/main.rs");
        assert_eq!(entries[0].status, GitFileStatus::Modified);
        assert!(!entries[0].staged);
    }

    #[test]
    fn test_parse_git_files_staged() {
        let output = "M  src/lib.rs\n";
        let entries = parse_git_files(output);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].status, GitFileStatus::Modified);
        assert!(entries[0].staged);
    }

    #[test]
    fn test_parse_git_files_untracked() {
        let output = "?? newfile.rs\n";
        let entries = parse_git_files(output);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].status, GitFileStatus::Untracked);
        assert!(!entries[0].staged);
    }

    #[test]
    fn test_parse_git_files_added() {
        let output = "A  added.rs\n";
        let entries = parse_git_files(output);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].status, GitFileStatus::Added);
        assert!(entries[0].staged);
    }

    #[test]
    fn test_parse_git_files_deleted() {
        let output = "D  deleted.rs\n";
        let entries = parse_git_files(output);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].status, GitFileStatus::Deleted);
        assert!(entries[0].staged);
    }

    #[test]
    fn test_parse_git_files_mixed() {
        let output = "M  staged.rs\n M unstaged.rs\n?? new.rs\nA  added.rs\n";
        let entries = parse_git_files(output);
        assert_eq!(entries.len(), 4);
    }

    #[test]
    fn test_parse_git_files_empty() {
        let output = "";
        let entries = parse_git_files(output);
        assert!(entries.is_empty());
    }

    #[test]
    fn test_render_file_list_empty() {
        let output = render_file_list(&[]);
        assert!(output.visible);
        assert_eq!(output.lines.len(), 1);
        assert!(output.lines[0].text.text.contains("clean"));
    }

    #[test]
    fn test_render_file_list_staged_and_unstaged() {
        let entries = vec![
            GitFileEntry {
                path: "staged.rs".to_string(),
                status: GitFileStatus::Modified,
                staged: true,
            },
            GitFileEntry {
                path: "unstaged.rs".to_string(),
                status: GitFileStatus::Modified,
                staged: false,
            },
        ];

        let output = render_file_list(&entries);
        assert!(output.visible);
        // Staged header + staged file + blank + Unstaged header + unstaged file = 5
        assert_eq!(output.lines.len(), 5);
    }

    #[test]
    fn test_git_file_status_indicators() {
        assert_eq!(GitFileStatus::Modified.indicator(), "M");
        assert_eq!(GitFileStatus::Added.indicator(), "A");
        assert_eq!(GitFileStatus::Deleted.indicator(), "D");
        assert_eq!(GitFileStatus::Renamed.indicator(), "R");
        assert_eq!(GitFileStatus::Untracked.indicator(), "?");
    }

    #[test]
    fn test_cache_invalidation() {
        let component = GitFilesSidebarComponent::new();
        *component.cache.lock() = Some(GitFilesCache {
            entries: vec![],
            cwd: PathBuf::from("/test"),
            fetched_at: Instant::now(),
        });
        assert!(component.cache.lock().is_some());

        component.invalidate_cache();
        assert!(component.cache.lock().is_none());
    }
}
