//! Recommendation engine -- orchestrates sources and ranks results.

use std::collections::HashSet;
use std::path::PathBuf;

use crate::input::RecommendInput;
use crate::scoring::rank;
use crate::source::{RecommendSource, Recommendation};
use crate::sources::{CommandSource, GitSource, PathSource};

/// Shell builtins for the default engine.
const BUILTINS: &[&str] = &[
    "alias", "bg", "cd", "echo", "eval", "exec", "exit", "export", "fg", "hash", "history",
    "jobs", "kill", "let", "local", "pwd", "read", "readonly", "return", "set", "shift",
    "source", "test", "trap", "type", "ulimit", "umask", "unalias", "unset", "wait",
];

/// Orchestrates multiple recommendation sources, aggregates and ranks results.
///
/// The engine collects recommendations from all registered sources,
/// ranks them by confidence, and returns the top results.
pub struct RecommendationEngine {
    sources: Vec<Box<dyn RecommendSource>>,
    max_results: usize,
}

impl RecommendationEngine {
    /// Create a new engine with no sources.
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
            max_results: 20,
        }
    }

    /// Create a new engine with default sources (commands from PATH, file paths, git).
    ///
    /// History-based completion is intentionally excluded here because the TUI's
    /// existing `GhostCompletion` already provides history matching. The engine
    /// adds command name, path, and git-aware completions on top.
    pub fn with_defaults() -> Self {
        let mut engine = Self::new();

        // Command source: builtins + PATH executables
        let builtins: Vec<String> = BUILTINS.iter().map(|s| (*s).to_string()).collect();
        let path_commands = scan_path_commands();
        engine.add_source(Box::new(CommandSource::new(builtins, path_commands)));

        // Path source
        engine.add_source(Box::new(PathSource));

        // Git source
        engine.add_source(Box::new(GitSource));

        engine
    }

    /// Set the maximum number of results to return.
    pub fn with_max_results(mut self, n: usize) -> Self {
        self.max_results = n;
        self
    }

    /// Add a recommendation source.
    pub fn add_source(&mut self, source: Box<dyn RecommendSource>) {
        self.sources.push(source);
    }

    /// Number of registered sources.
    pub fn source_count(&self) -> usize {
        self.sources.len()
    }

    /// Generate ranked recommendations.
    pub fn recommend(&self, line: &str, cursor: usize, cwd: PathBuf) -> Vec<Recommendation> {
        let input = RecommendInput::parse(line, cursor, cwd);

        let mut all: Vec<Recommendation> = self
            .sources
            .iter()
            .flat_map(|s| s.recommend(&input))
            .collect();

        rank(&mut all);
        all.truncate(self.max_results);
        all
    }

    /// Get the top recommendation as ghost text (text to append after input).
    pub fn ghost_text(&self, line: &str, cursor: usize, cwd: PathBuf) -> Option<String> {
        self.recommend(line, cursor, cwd)
            .into_iter()
            .next()
            .map(|r| r.text)
            .filter(|t| !t.is_empty())
    }
}

/// Scan PATH directories for executable command names.
fn scan_path_commands() -> Vec<String> {
    let Ok(path_var) = std::env::var("PATH") else {
        return Vec::new();
    };

    let mut seen = HashSet::new();
    let mut commands = Vec::new();

    for dir in std::env::split_paths(&path_var) {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            // Check executable bit on unix
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let Ok(meta) = entry.metadata() else {
                    continue;
                };
                if meta.permissions().mode() & 0o111 == 0 {
                    continue;
                }
            }
            if seen.insert(name.clone()) {
                commands.push(name);
            }
        }
    }

    commands
}

impl Default for RecommendationEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::HistorySource;

    #[test]
    fn engine_aggregates_sources() {
        let history = HistorySource::new(vec![
            "git status".to_string(),
            "git commit -m 'test'".to_string(),
        ]);
        let mut engine = RecommendationEngine::new();
        engine.add_source(Box::new(history));

        let results = engine.recommend("git ", 4, PathBuf::from("/tmp"));
        assert!(!results.is_empty());
    }

    #[test]
    fn engine_returns_sorted_by_confidence() {
        let history = HistorySource::new(vec![
            "git status".to_string(),
            "git commit".to_string(),
        ]);
        let mut engine = RecommendationEngine::new();
        engine.add_source(Box::new(history));

        let results = engine.recommend("git ", 4, PathBuf::from("/tmp"));
        for window in results.windows(2) {
            assert!(window[0].confidence >= window[1].confidence);
        }
    }

    #[test]
    fn ghost_text_returns_top() {
        let history = HistorySource::new(vec!["cargo test".to_string()]);
        let mut engine = RecommendationEngine::new();
        engine.add_source(Box::new(history));

        let ghost = engine.ghost_text("cargo ", 6, PathBuf::from("/tmp"));
        assert!(ghost.is_some());
        assert_eq!(ghost.unwrap(), "test");
    }

    #[test]
    fn ghost_text_returns_none_for_empty() {
        let engine = RecommendationEngine::new();
        let ghost = engine.ghost_text("", 0, PathBuf::from("/tmp"));
        assert!(ghost.is_none());
    }

    #[test]
    fn respects_max_results() {
        let entries: Vec<String> = (0..50).map(|i| format!("cmd{i}")).collect();
        let history = HistorySource::new(entries);
        let mut engine = RecommendationEngine::new().with_max_results(5);
        engine.add_source(Box::new(history));

        let results = engine.recommend("cmd", 3, PathBuf::from("/tmp"));
        assert!(results.len() <= 5);
    }

    #[test]
    fn multiple_sources_combine() {
        use crate::sources::CommandSource;

        let history = HistorySource::new(vec!["git push origin main".to_string()]);
        let commands = CommandSource::new(
            vec![],
            vec!["git".to_string(), "grep".to_string()],
        );

        let mut engine = RecommendationEngine::new();
        engine.add_source(Box::new(history));
        engine.add_source(Box::new(commands));

        assert_eq!(engine.source_count(), 2);

        // "gi" should get results from both history and command source
        let results = engine.recommend("gi", 2, PathBuf::from("/tmp"));
        assert!(!results.is_empty());
    }

    #[test]
    fn default_creates_empty_engine() {
        let engine = RecommendationEngine::default();
        assert_eq!(engine.source_count(), 0);
        let results = engine.recommend("anything", 8, PathBuf::from("/tmp"));
        assert!(results.is_empty());
    }

    #[test]
    fn with_defaults_has_sources() {
        let engine = RecommendationEngine::with_defaults();
        // Should have command, path, and git sources
        assert_eq!(engine.source_count(), 3);
    }

    #[test]
    fn scan_path_finds_commands() {
        let commands = scan_path_commands();
        // PATH should have at least some commands on any system
        assert!(!commands.is_empty());
        // No duplicates
        let unique: HashSet<&String> = commands.iter().collect();
        assert_eq!(unique.len(), commands.len());
    }
}
