//! Ghost completion system for inline suggestions
//!
//! Provides inline "ghost text" completions that appear after the cursor
//! as dimmed text. Users can accept with Tab or right arrow.

use std::collections::HashMap;

/// A ghost completion suggestion
#[derive(Debug, Clone)]
pub struct GhostSuggestion {
    /// The text to insert (everything after current input)
    pub text: String,
    /// Source of the suggestion (for debugging/display)
    pub source: SuggestionSource,
    /// Confidence/priority (higher = better match)
    pub priority: u32,
}

/// Where a suggestion came from
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestionSource {
    /// From command history
    History,
    /// From file path completion
    FilePath,
    /// From command/binary names
    Command,
    /// From git branches/refs
    Git,
    /// From custom/plugin source
    Custom,
}

/// Ghost completion state
#[derive(Debug)]
pub struct GhostCompletion {
    /// Current suggestion (if any)
    current: Option<GhostSuggestion>,
    /// All available suggestions for current input
    suggestions: Vec<GhostSuggestion>,
    /// Current index in suggestions
    index: usize,
    /// Last input text (to detect changes)
    last_input: String,
    /// Command history for history-based suggestions
    history: Vec<String>,
    /// Frequency map for command prefixes (for ranking)
    frequency: HashMap<String, u32>,
}

impl GhostCompletion {
    /// Create a new ghost completion manager
    pub fn new() -> Self {
        Self {
            current: None,
            suggestions: Vec::new(),
            index: 0,
            last_input: String::new(),
            history: Vec::new(),
            frequency: HashMap::new(),
        }
    }

    /// Get the current ghost text to display (if any)
    pub fn ghost_text(&self) -> Option<&str> {
        self.current.as_ref().map(|s| s.text.as_str())
    }

    /// Get current suggestion source
    pub fn source(&self) -> Option<SuggestionSource> {
        self.current.as_ref().map(|s| s.source)
    }

    /// Update suggestions based on current input
    pub fn update(&mut self, input: &str) {
        // Skip if input hasn't changed
        if input == self.last_input {
            return;
        }

        self.last_input = input.to_string();
        self.suggestions.clear();
        self.index = 0;

        // Don't suggest on empty input
        if input.is_empty() {
            self.current = None;
            return;
        }

        // Collect suggestions from all sources
        self.collect_history_suggestions(input);
        self.collect_path_suggestions(input);

        // Sort by priority (higher first)
        self.suggestions
            .sort_by_key(|s| std::cmp::Reverse(s.priority));

        // Set current suggestion
        self.current = self.suggestions.first().cloned();
    }

    /// Add a command to history (called when command is executed)
    pub fn add_to_history(&mut self, cmd: &str) {
        let cmd = cmd.trim();
        if cmd.is_empty() {
            return;
        }

        // Update frequency for the command and its prefixes
        self.increment_frequency(cmd);

        // Add to history if not duplicate of last
        if self.history.last().map(|s| s.as_str()) != Some(cmd) {
            self.history.push(cmd.to_string());
        }

        // Limit history size
        if self.history.len() > 10000 {
            self.history.remove(0);
        }
    }

    /// Load history from an iterator of commands
    pub fn load_history<I: IntoIterator<Item = String>>(&mut self, history: I) {
        for cmd in history {
            self.add_to_history(&cmd);
        }
    }

    /// Accept the current suggestion
    /// Returns the full completed text if there was a suggestion
    pub fn accept(&mut self) -> Option<String> {
        if let Some(suggestion) = self.current.take() {
            let full_text = format!("{}{}", self.last_input, suggestion.text);
            self.suggestions.clear();
            self.index = 0;
            Some(full_text)
        } else {
            None
        }
    }

    /// Accept only the first word of the current suggestion
    /// Returns the text to insert if there was a suggestion
    pub fn accept_word(&mut self) -> Option<String> {
        if let Some(ref suggestion) = self.current {
            // Find first word boundary
            let text = &suggestion.text;
            let first_word_end = text
                .find(|c: char| c.is_whitespace() || c == '/')
                .unwrap_or(text.len());

            if first_word_end > 0 {
                let word = text[..first_word_end].to_string();
                return Some(word);
            }
        }
        None
    }

    /// Cycle to next suggestion
    pub fn next(&mut self) {
        if self.suggestions.is_empty() {
            return;
        }
        self.index = (self.index + 1) % self.suggestions.len();
        self.current = self.suggestions.get(self.index).cloned();
    }

    /// Cycle to previous suggestion
    pub fn prev(&mut self) {
        if self.suggestions.is_empty() {
            return;
        }
        self.index = if self.index == 0 {
            self.suggestions.len() - 1
        } else {
            self.index - 1
        };
        self.current = self.suggestions.get(self.index).cloned();
    }

    /// Clear current suggestion
    pub fn clear(&mut self) {
        self.current = None;
        self.suggestions.clear();
        self.index = 0;
        self.last_input.clear();
    }

    /// Check if there's an active suggestion
    pub fn has_suggestion(&self) -> bool {
        self.current.is_some()
    }

    /// Get number of available suggestions
    pub fn suggestion_count(&self) -> usize {
        self.suggestions.len()
    }

    /// Get current index (1-based for display)
    pub fn current_index(&self) -> usize {
        if self.suggestions.is_empty() {
            0
        } else {
            self.index + 1
        }
    }

    // Private methods

    fn collect_history_suggestions(&mut self, input: &str) {
        let input_lower = input.to_lowercase();

        // Find history entries that start with input
        for cmd in self.history.iter().rev() {
            if cmd.len() > input.len() && cmd.to_lowercase().starts_with(&input_lower) {
                let suffix = &cmd[input.len()..];
                let priority = self.frequency.get(cmd).copied().unwrap_or(1);

                // Avoid duplicates
                if !self.suggestions.iter().any(|s| s.text == suffix) {
                    self.suggestions.push(GhostSuggestion {
                        text: suffix.to_string(),
                        source: SuggestionSource::History,
                        priority,
                    });
                }
            }

            // Limit number of history suggestions
            if self.suggestions.len() >= 20 {
                break;
            }
        }
    }

    fn collect_path_suggestions(&mut self, input: &str) {
        // Only suggest paths if input looks like a path or command with path arg
        let path_input = self.extract_path_context(input);

        if let Some((_prefix, path_part)) = path_input {
            if let Some(suggestions) = self.suggest_paths(&path_part) {
                for (suffix, is_dir) in suggestions {
                    // Build the full suggestion (everything after current input)
                    let full_suffix = suffix.clone();

                    self.suggestions.push(GhostSuggestion {
                        text: full_suffix,
                        source: SuggestionSource::FilePath,
                        // Directories get slight boost for tab completion
                        priority: if is_dir { 2 } else { 1 },
                    });
                }
            }
        }
    }

    fn extract_path_context(&self, input: &str) -> Option<(String, String)> {
        // Check if input ends with something that looks like a path
        // e.g., "cd foo/bar" -> ("cd foo/", "bar")
        // e.g., "./scr" -> ("./", "scr")
        // e.g., "~/Doc" -> ("~/", "Doc")

        let input = input.trim_end();

        // Find the last "word" that might be a path
        let last_space = input.rfind(' ').map(|i| i + 1).unwrap_or(0);
        let path_part = &input[last_space..];

        // Check if it looks like a path
        if path_part.starts_with('/')
            || path_part.starts_with("./")
            || path_part.starts_with("../")
            || path_part.starts_with('~')
            || path_part.contains('/')
        {
            let prefix = input[..last_space].to_string();
            return Some((prefix, path_part.to_string()));
        }

        None
    }

    fn suggest_paths(&self, partial: &str) -> Option<Vec<(String, bool)>> {
        use std::path::Path;

        // Expand ~ to home directory
        let expanded = if partial.starts_with('~') {
            let home = dirs::home_dir()?;
            partial.replacen('~', home.to_str()?, 1)
        } else {
            partial.to_string()
        };

        let path = Path::new(&expanded);

        // Determine the directory to search and the prefix to match
        let (dir, prefix) = if path.is_dir() && partial.ends_with('/') {
            (path.to_path_buf(), String::new())
        } else {
            let parent = path.parent().unwrap_or(Path::new("."));
            let file_prefix = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            (parent.to_path_buf(), file_prefix.to_string())
        };

        // Read directory
        let entries = std::fs::read_dir(&dir).ok()?;

        let mut results = Vec::new();
        let prefix_lower = prefix.to_lowercase();

        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            // Skip hidden files unless explicitly requested
            if name_str.starts_with('.') && !prefix.starts_with('.') {
                continue;
            }

            // Match prefix
            if name_str.to_lowercase().starts_with(&prefix_lower) {
                let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);

                // Calculate the suffix to add (everything after current input)
                let suffix = &name_str[prefix.len()..];
                let suffix_with_slash = if is_dir {
                    format!("{}/", suffix)
                } else {
                    suffix.to_string()
                };

                results.push((suffix_with_slash, is_dir));
            }
        }

        // Sort: directories first, then alphabetically
        results.sort_by(|a, b| match (a.1, b.1) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.0.to_lowercase().cmp(&b.0.to_lowercase()),
        });

        // Limit results
        results.truncate(10);

        if results.is_empty() {
            None
        } else {
            Some(results)
        }
    }

    fn increment_frequency(&mut self, cmd: &str) {
        *self.frequency.entry(cmd.to_string()).or_insert(0) += 1;

        // Also increment frequency for prefix (first word)
        if let Some(first_word) = cmd.split_whitespace().next() {
            *self.frequency.entry(first_word.to_string()).or_insert(0) += 1;
        }
    }
}

impl Default for GhostCompletion {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_history_suggestion() {
        let mut ghost = GhostCompletion::new();
        ghost.add_to_history("git status");
        ghost.add_to_history("git commit -m 'test'");
        ghost.add_to_history("git push origin main");

        ghost.update("git s");
        assert!(ghost.has_suggestion());
        assert_eq!(ghost.ghost_text(), Some("tatus"));
    }

    #[test]
    fn test_frequency_ranking() {
        let mut ghost = GhostCompletion::new();
        ghost.add_to_history("git status");
        ghost.add_to_history("git stash");
        ghost.add_to_history("git status");
        ghost.add_to_history("git status"); // Most frequent

        ghost.update("git st");
        assert!(ghost.has_suggestion());
        // "status" should be ranked higher due to frequency
        assert_eq!(ghost.ghost_text(), Some("atus"));
    }

    #[test]
    fn test_accept_completion() {
        let mut ghost = GhostCompletion::new();
        ghost.add_to_history("cargo build --release");

        ghost.update("cargo b");
        assert!(ghost.has_suggestion());

        let result = ghost.accept();
        assert_eq!(result, Some("cargo build --release".to_string()));
        assert!(!ghost.has_suggestion()); // Cleared after accept
    }

    #[test]
    fn test_cycle_suggestions() {
        let mut ghost = GhostCompletion::new();
        ghost.add_to_history("echo hello");
        ghost.add_to_history("echo world");
        ghost.add_to_history("echo test");

        ghost.update("echo ");
        assert!(ghost.suggestion_count() >= 3);

        let first = ghost.ghost_text().unwrap().to_string();
        ghost.next();
        let second = ghost.ghost_text().unwrap().to_string();
        assert_ne!(first, second);
    }

    #[test]
    fn test_clear_on_empty() {
        let mut ghost = GhostCompletion::new();
        ghost.add_to_history("ls -la");

        ghost.update("ls");
        assert!(ghost.has_suggestion());

        ghost.update("");
        assert!(!ghost.has_suggestion());
    }

    #[test]
    fn test_case_insensitive() {
        let mut ghost = GhostCompletion::new();
        ghost.add_to_history("Git Status");

        ghost.update("git s");
        // Should still find "Git Status" case-insensitively
        assert!(ghost.has_suggestion());
    }
}
