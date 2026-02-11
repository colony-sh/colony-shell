use crate::component::{ComponentOutput, PromptComponent, TuplePattern};
use crate::context::ComponentContext;
use crate::{Color, Style, StyledSpan};
use colony_tuple::Value;
use parking_lot::Mutex;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

/// Cached git status result
#[derive(Clone, Debug)]
struct GitStatusCache {
    status_info: StatusInfo,
    diff_stat: DiffStat,
    in_git: bool,
    cwd: PathBuf,
    fetched_at: Instant,
}

pub struct GitStatusComponent {
    cache_ttl: Duration,
    cache: Mutex<Option<GitStatusCache>>,
}

impl GitStatusComponent {
    pub fn new() -> Self {
        Self {
            cache_ttl: Duration::from_millis(500),
            cache: Mutex::new(None),
        }
    }

    /// Create with a custom cache TTL
    pub fn with_cache_ttl(ttl: Duration) -> Self {
        Self {
            cache_ttl: ttl,
            cache: Mutex::new(None),
        }
    }

    /// Invalidate the cache
    pub fn invalidate_cache(&self) {
        *self.cache.lock() = None;
    }

    fn fetch_status(cwd: &std::path::Path) -> Option<(StatusInfo, DiffStat)> {
        let in_git = Command::new("git")
            .args(["rev-parse", "--is-inside-work-tree"])
            .current_dir(cwd)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if !in_git {
            return None;
        }

        let status_info = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(cwd)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| {
                let stdout = String::from_utf8_lossy(&o.stdout);
                parse_porcelain_status(&stdout)
            })
            .unwrap_or_default();

        let diff_stat = Command::new("git")
            .args(["diff", "--stat", "HEAD"])
            .current_dir(cwd)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| {
                let stdout = String::from_utf8_lossy(&o.stdout);
                parse_diff_stat(&stdout)
            })
            .unwrap_or_default();

        Some((status_info, diff_stat))
    }
}

impl Default for GitStatusComponent {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Default, Clone)]
pub struct DiffStat {
    pub files_changed: usize,
    pub insertions: usize,
    pub deletions: usize,
}

#[derive(Debug, Default, Clone)]
pub struct StatusInfo {
    pub modified: usize,
    pub staged: usize,
    pub untracked: usize,
}

pub fn parse_diff_stat(output: &str) -> DiffStat {
    let mut stat = DiffStat::default();

    for line in output.lines() {
        if line.contains("files changed") || line.contains("file changed") {
            if let Some(ins_match) = line.split("insertion").next() {
                if let Some(num) = ins_match.split_whitespace().last() {
                    stat.insertions = num.parse().unwrap_or(0);
                }
            }
            if let Some(del_part) = line.split("deletion").next() {
                if let Some(num) = del_part
                    .split(',')
                    .next_back()
                    .and_then(|s| s.split_whitespace().last())
                {
                    stat.deletions = num.parse().unwrap_or(0);
                }
            }
            if let Some(num) = line.split_whitespace().next() {
                stat.files_changed = num.parse().unwrap_or(0);
            }
        }
    }

    stat
}

pub fn parse_porcelain_status(output: &str) -> StatusInfo {
    let mut info = StatusInfo::default();

    for line in output.lines() {
        if line.len() < 2 {
            continue;
        }

        let index = line.chars().next().unwrap_or(' ');
        let worktree = line.chars().nth(1).unwrap_or(' ');

        match (index, worktree) {
            ('?', '?') => info.untracked += 1,
            ('A', _) | ('M', ' ') | ('D', ' ') | ('R', _) | ('C', _) => info.staged += 1,
            (_, 'M') | (_, 'D') => info.modified += 1,
            _ => {}
        }
    }

    info
}

impl PromptComponent for GitStatusComponent {
    fn id(&self) -> &str {
        "git_status"
    }

    fn subscriptions(&self) -> Vec<TuplePattern> {
        vec![
            vec![
                Value::Str("dir_change".to_string()),
                Value::Wildcard,
                Value::Wildcard,
            ],
            vec![Value::Str("file_change".to_string()), Value::Wildcard],
            vec![Value::Str("cmd_complete".to_string())],
        ]
    }

    fn render(&self, ctx: &ComponentContext) -> ComponentOutput {
        // Check cache: valid if same directory and within TTL
        {
            let cache = self.cache.lock();
            if let Some(ref cached) = *cache {
                if cached.cwd == ctx.cwd && cached.fetched_at.elapsed() < self.cache_ttl {
                    if !cached.in_git {
                        return ComponentOutput::hidden();
                    }
                    return render_status_output(&cached.status_info, &cached.diff_stat);
                }
            }
        }

        // Cache miss or stale: fetch fresh data
        let Some((status_info, diff_stat)) = Self::fetch_status(&ctx.cwd) else {
            *self.cache.lock() = Some(GitStatusCache {
                status_info: StatusInfo::default(),
                diff_stat: DiffStat::default(),
                in_git: false,
                cwd: ctx.cwd.clone(),
                fetched_at: Instant::now(),
            });
            return ComponentOutput::hidden();
        };

        let output = render_status_output(&status_info, &diff_stat);

        // Update cache
        *self.cache.lock() = Some(GitStatusCache {
            status_info,
            diff_stat,
            in_git: true,
            cwd: ctx.cwd.clone(),
            fetched_at: Instant::now(),
        });

        output
    }
}

fn render_status_output(status_info: &StatusInfo, diff_stat: &DiffStat) -> ComponentOutput {
    let mut spans = Vec::new();
    let mut has_content = false;

    let total_modified = status_info.modified + diff_stat.files_changed;
    if total_modified > 0 {
        spans.push(StyledSpan::new(
            format!("{}", total_modified),
            Style::default().fg(Color::Yellow),
        ));
        has_content = true;
    }

    if diff_stat.insertions > 0 || diff_stat.deletions > 0 {
        if has_content {
            spans.push(StyledSpan::plain(" "));
        }

        if diff_stat.insertions > 0 {
            spans.push(StyledSpan::new(
                format!("+{}", diff_stat.insertions),
                Style::default().fg(Color::Green),
            ));
        }

        if diff_stat.deletions > 0 {
            if diff_stat.insertions > 0 {
                spans.push(StyledSpan::plain(" "));
            }
            spans.push(StyledSpan::new(
                format!("-{}", diff_stat.deletions),
                Style::default().fg(Color::Red),
            ));
        }
        has_content = true;
    }

    if status_info.staged > 0 {
        if has_content {
            spans.push(StyledSpan::plain(" "));
        }
        spans.push(StyledSpan::new(
            format!("\u{25cf}{}", status_info.staged),
            Style::default().fg(Color::Green),
        ));
        has_content = true;
    }

    if status_info.untracked > 0 {
        if has_content {
            spans.push(StyledSpan::plain(" "));
        }
        spans.push(StyledSpan::new(
            format!("?{}", status_info.untracked),
            Style::default().fg(Color::BrightBlack),
        ));
        has_content = true;
    }

    if !has_content {
        return ComponentOutput::hidden();
    }

    ComponentOutput::with_data(
        spans,
        serde_json::json!({
            "modified": status_info.modified,
            "staged": status_info.staged,
            "untracked": status_info.untracked,
            "insertions": diff_stat.insertions,
            "deletions": diff_stat.deletions,
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_diff_stat_summary() {
        let output = " file1.rs |  10 ++++------
 file2.rs |   5 +++++
 3 files changed, 12 insertions(+), 6 deletions(-)
";
        let result = parse_diff_stat(output);
        assert_eq!(result.files_changed, 3);
        assert_eq!(result.insertions, 12);
        assert_eq!(result.deletions, 6);
    }

    #[test]
    fn test_parse_diff_stat_empty() {
        let output = "";
        let result = parse_diff_stat(output);
        assert_eq!(result.files_changed, 0);
    }

    #[test]
    fn test_parse_porcelain_status() {
        let output = " M file1.rs
?? newfile.rs
A  staged.rs
";
        let result = parse_porcelain_status(output);
        assert_eq!(result.modified, 1);
        assert_eq!(result.untracked, 1);
        assert_eq!(result.staged, 1);
    }

    #[test]
    fn test_cache_invalidation() {
        let component = GitStatusComponent::new();
        *component.cache.lock() = Some(GitStatusCache {
            status_info: StatusInfo::default(),
            diff_stat: DiffStat::default(),
            in_git: true,
            cwd: PathBuf::from("/test"),
            fetched_at: Instant::now(),
        });
        assert!(component.cache.lock().is_some());

        component.invalidate_cache();
        assert!(component.cache.lock().is_none());
    }
}
