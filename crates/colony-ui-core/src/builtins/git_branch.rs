use crate::component::{ComponentOutput, PromptComponent, TuplePattern};
use crate::context::ComponentContext;
use crate::{Color, Style, StyledSpan};
use colony_tuple::Value;
use parking_lot::Mutex;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

/// Cached git branch result with timestamp and directory
#[derive(Clone, Debug)]
struct GitBranchCache {
    branch: String,
    detached: bool,
    cwd: PathBuf,
    fetched_at: Instant,
}

pub struct GitBranchComponent {
    cache_ttl: Duration,
    cache: Mutex<Option<GitBranchCache>>,
}

impl GitBranchComponent {
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

    /// Invalidate the cache (called on dir_change events)
    pub fn invalidate_cache(&self) {
        *self.cache.lock() = None;
    }

    fn fetch_branch(cwd: &std::path::Path) -> Option<(String, bool)> {
        let output = Command::new("git").arg("branch").current_dir(cwd).output();

        match output {
            Ok(out) if out.status.success() => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                parse_git_branch(&stdout)
            }
            _ => None,
        }
    }
}

impl Default for GitBranchComponent {
    fn default() -> Self {
        Self::new()
    }
}

pub fn parse_git_branch(output: &str) -> Option<(String, bool)> {
    for line in output.lines() {
        let Some(rest) = line.strip_prefix('*') else {
            continue;
        };
        let branch_part = rest.trim();

        if branch_part.starts_with("(HEAD detached at ") {
            let hash = branch_part
                .strip_prefix("(HEAD detached at ")
                .and_then(|s| s.strip_suffix(')'))
                .unwrap_or(branch_part);
            return Some((hash.to_string(), true));
        }

        if branch_part.starts_with('(') {
            return Some((branch_part.to_string(), true));
        }

        return Some((branch_part.to_string(), false));
    }
    None
}

impl PromptComponent for GitBranchComponent {
    fn id(&self) -> &str {
        "git_branch"
    }

    fn subscriptions(&self) -> Vec<TuplePattern> {
        vec![vec![
            Value::Str("dir_change".to_string()),
            Value::Wildcard,
            Value::Wildcard,
        ]]
    }

    fn render(&self, ctx: &ComponentContext) -> ComponentOutput {
        // Check cache: valid if same directory and within TTL
        {
            let cache = self.cache.lock();
            if let Some(ref cached) = *cache {
                if cached.cwd == ctx.cwd && cached.fetched_at.elapsed() < self.cache_ttl {
                    return render_branch_output(&cached.branch, cached.detached);
                }
            }
        }

        // Cache miss or stale: fetch fresh data
        let Some((branch, detached)) = Self::fetch_branch(&ctx.cwd) else {
            *self.cache.lock() = None;
            return ComponentOutput::hidden();
        };

        // Update cache
        *self.cache.lock() = Some(GitBranchCache {
            branch: branch.clone(),
            detached,
            cwd: ctx.cwd.clone(),
            fetched_at: Instant::now(),
        });

        render_branch_output(&branch, detached)
    }
}

fn render_branch_output(branch: &str, detached: bool) -> ComponentOutput {
    let icon = "\u{e0a0}";
    let text = format!("{} {}", icon, branch);

    let color = if detached {
        Color::Yellow
    } else {
        Color::Green
    };

    let spans = vec![StyledSpan::new(text, Style::default().fg(color))];

    ComponentOutput::with_data(
        spans,
        serde_json::json!({
            "branch": branch,
            "detached": detached,
        }),
    )
    .with_interactive(
        "branch_picker",
        Some("Click to switch branches".to_string()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_git_branch_normal() {
        let output = "* main\n  feature-x\n";
        let result = parse_git_branch(output);
        assert_eq!(result, Some(("main".to_string(), false)));
    }

    #[test]
    fn test_parse_git_branch_detached() {
        let output = "* (HEAD detached at abc1234)\n  main\n";
        let result = parse_git_branch(output);
        assert_eq!(result, Some(("abc1234".to_string(), true)));
    }

    #[test]
    fn test_parse_git_branch_empty() {
        let output = "";
        let result = parse_git_branch(output);
        assert_eq!(result, None);
    }

    #[test]
    fn test_cache_invalidation() {
        let component = GitBranchComponent::new();
        // Manually set cache
        *component.cache.lock() = Some(GitBranchCache {
            branch: "main".to_string(),
            detached: false,
            cwd: PathBuf::from("/test"),
            fetched_at: Instant::now(),
        });
        assert!(component.cache.lock().is_some());

        component.invalidate_cache();
        assert!(component.cache.lock().is_none());
    }

    #[test]
    fn test_cache_ttl() {
        let component = GitBranchComponent::with_cache_ttl(Duration::from_millis(10));
        assert_eq!(component.cache_ttl, Duration::from_millis(10));
    }
}
