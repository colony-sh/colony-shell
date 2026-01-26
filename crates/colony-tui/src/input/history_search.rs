//! Fuzzy history search popup (fzf-style)
//!
//! Provides a popup overlay for searching command history with fuzzy matching.
//! Triggered by Ctrl+r in insert mode.

/// History search state
#[derive(Debug)]
pub struct HistorySearch {
    /// Whether the search popup is active
    active: bool,
    /// Current search query
    query: String,
    /// Filtered/matched results
    results: Vec<HistoryMatch>,
    /// Currently selected index
    selected: usize,
    /// Full history (reference kept for searching)
    history: Vec<String>,
}

/// A matched history entry
#[derive(Debug, Clone)]
pub struct HistoryMatch {
    /// The full command text
    pub command: String,
    /// Match score (higher = better)
    pub score: i64,
    /// Positions of matched characters (for highlighting)
    pub match_positions: Vec<usize>,
    /// Original index in history
    pub history_index: usize,
}

impl HistorySearch {
    /// Create a new history search
    pub fn new() -> Self {
        Self {
            active: false,
            query: String::new(),
            results: Vec::new(),
            selected: 0,
            history: Vec::new(),
        }
    }

    /// Check if search is active
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Activate the search popup
    pub fn activate(&mut self) {
        self.active = true;
        self.query.clear();
        self.selected = 0;
        self.update_results();
    }

    /// Deactivate the search popup
    pub fn deactivate(&mut self) {
        self.active = false;
        self.query.clear();
        self.results.clear();
    }

    /// Get the current query
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Get filtered results
    pub fn results(&self) -> &[HistoryMatch] {
        &self.results
    }

    /// Get currently selected index
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// Get the selected command (if any)
    pub fn selected_command(&self) -> Option<&str> {
        self.results.get(self.selected).map(|m| m.command.as_str())
    }

    /// Add a character to the query
    pub fn insert_char(&mut self, c: char) {
        self.query.push(c);
        self.update_results();
        self.selected = 0;
    }

    /// Delete the last character from the query
    pub fn delete_char(&mut self) {
        self.query.pop();
        self.update_results();
        self.selected = 0;
    }

    /// Move selection up
    pub fn select_prev(&mut self) {
        if !self.results.is_empty() {
            if self.selected == 0 {
                self.selected = self.results.len() - 1;
            } else {
                self.selected -= 1;
            }
        }
    }

    /// Move selection down
    pub fn select_next(&mut self) {
        if !self.results.is_empty() {
            self.selected = (self.selected + 1) % self.results.len();
        }
    }

    /// Accept the current selection
    /// Returns the selected command and deactivates
    pub fn accept(&mut self) -> Option<String> {
        let result = self.selected_command().map(|s| s.to_string());
        self.deactivate();
        result
    }

    /// Load history from an iterator
    pub fn load_history<I: IntoIterator<Item = String>>(&mut self, history: I) {
        self.history = history.into_iter().collect();
    }

    /// Add a command to history
    pub fn add_to_history(&mut self, cmd: &str) {
        let cmd = cmd.trim();
        if cmd.is_empty() {
            return;
        }
        // Don't add duplicates of the last command
        if self.history.last().map(|s| s.as_str()) != Some(cmd) {
            self.history.push(cmd.to_string());
        }
        // Limit history size
        if self.history.len() > 10000 {
            self.history.remove(0);
        }
    }

    /// Update results based on current query
    fn update_results(&mut self) {
        self.results.clear();

        if self.query.is_empty() {
            // Show all history (most recent first)
            for (idx, cmd) in self.history.iter().enumerate().rev() {
                self.results.push(HistoryMatch {
                    command: cmd.clone(),
                    score: idx as i64, // More recent = higher score
                    match_positions: Vec::new(),
                    history_index: idx,
                });
                if self.results.len() >= 50 {
                    break;
                }
            }
        } else {
            // Fuzzy match
            let query_lower = self.query.to_lowercase();
            let query_chars: Vec<char> = query_lower.chars().collect();

            for (idx, cmd) in self.history.iter().enumerate() {
                if let Some((score, positions)) = fuzzy_match(cmd, &query_chars) {
                    self.results.push(HistoryMatch {
                        command: cmd.clone(),
                        score,
                        match_positions: positions,
                        history_index: idx,
                    });
                }
            }

            // Sort by score (higher first), then by recency
            self.results.sort_by(|a, b| match b.score.cmp(&a.score) {
                std::cmp::Ordering::Equal => b.history_index.cmp(&a.history_index),
                other => other,
            });

            // Limit results
            self.results.truncate(50);
        }
    }
}

impl Default for HistorySearch {
    fn default() -> Self {
        Self::new()
    }
}

/// Fuzzy match a string against query characters
/// Returns (score, match_positions) if matched, None if not
fn fuzzy_match(text: &str, query_chars: &[char]) -> Option<(i64, Vec<usize>)> {
    if query_chars.is_empty() {
        return Some((0, Vec::new()));
    }

    let text_lower = text.to_lowercase();
    let text_chars: Vec<char> = text_lower.chars().collect();

    let mut positions = Vec::with_capacity(query_chars.len());
    let mut score: i64 = 0;
    let mut query_idx = 0;
    let mut last_match_pos: Option<usize> = None;

    for (text_idx, text_char) in text_chars.iter().enumerate() {
        if query_idx < query_chars.len() && *text_char == query_chars[query_idx] {
            positions.push(text_idx);

            // Scoring
            // Consecutive matches get a bonus
            if let Some(last_pos) = last_match_pos {
                if text_idx == last_pos + 1 {
                    score += 10; // Consecutive bonus
                }
            }

            // Start of word bonus (after space, /, -, _)
            if text_idx == 0 {
                score += 15; // Start of string bonus
            } else {
                let prev_char = text_chars.get(text_idx - 1);
                if matches!(prev_char, Some(' ') | Some('/') | Some('-') | Some('_')) {
                    score += 10; // Word boundary bonus
                }
            }

            // Exact case match bonus
            let orig_chars: Vec<char> = text.chars().collect();
            if text_idx < orig_chars.len()
                && orig_chars[text_idx] == query_chars[query_idx].to_ascii_uppercase()
            {
                // Note: we're comparing lowercase query to original, so this checks uppercase
            }

            score += 1; // Base score for each match
            last_match_pos = Some(text_idx);
            query_idx += 1;
        }
    }

    // Check if we matched all query characters
    if query_idx == query_chars.len() {
        // Penalty for longer strings (prefer shorter commands)
        score -= (text.len() / 10) as i64;
        Some((score, positions))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fuzzy_match_exact() {
        let result = fuzzy_match("git status", &['g', 'i', 't']);
        assert!(result.is_some());
        let (score, positions) = result.unwrap();
        assert!(score > 0);
        assert_eq!(positions, vec![0, 1, 2]);
    }

    #[test]
    fn test_fuzzy_match_scattered() {
        let result = fuzzy_match("git status", &['g', 's', 't']);
        assert!(result.is_some());
        let (_, positions) = result.unwrap();
        assert_eq!(positions, vec![0, 4, 5]); // g, s(tatus), t(atus)
    }

    #[test]
    fn test_fuzzy_match_no_match() {
        let result = fuzzy_match("git status", &['z', 'z', 'z']);
        assert!(result.is_none());
    }

    #[test]
    fn test_history_search_activate() {
        let mut search = HistorySearch::new();
        search.add_to_history("ls -la");
        search.add_to_history("git status");
        search.add_to_history("cargo build");

        search.activate();
        assert!(search.is_active());
        assert_eq!(search.results().len(), 3);

        // Most recent first
        assert_eq!(search.results()[0].command, "cargo build");
    }

    #[test]
    fn test_history_search_filter() {
        let mut search = HistorySearch::new();
        search.add_to_history("ls -la");
        search.add_to_history("git status");
        search.add_to_history("git commit");
        search.add_to_history("cargo build");

        search.activate();
        search.insert_char('g');
        search.insert_char('i');
        search.insert_char('t');

        // Should only show git commands
        assert!(search.results().iter().all(|r| r.command.contains("git")));
    }

    #[test]
    fn test_history_search_select() {
        let mut search = HistorySearch::new();
        search.add_to_history("cmd1");
        search.add_to_history("cmd2");
        search.add_to_history("cmd3");

        search.activate();
        assert_eq!(search.selected(), 0);

        search.select_next();
        assert_eq!(search.selected(), 1);

        search.select_prev();
        assert_eq!(search.selected(), 0);

        // Wrap around
        search.select_prev();
        assert_eq!(search.selected(), 2);
    }

    #[test]
    fn test_history_search_accept() {
        let mut search = HistorySearch::new();
        search.add_to_history("echo hello");
        search.activate();

        let result = search.accept();
        assert_eq!(result, Some("echo hello".to_string()));
        assert!(!search.is_active());
    }

    #[test]
    fn test_case_insensitive() {
        let result = fuzzy_match("Git Status", &['g', 'i', 't']);
        assert!(result.is_some());
    }
}
