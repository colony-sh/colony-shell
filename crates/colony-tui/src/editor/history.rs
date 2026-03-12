//! Editor history integration.
//!
//! Provides `EditorHistory` for navigating command history from the input editor.
//! This is a separate struct from the shell's history store; it wraps history
//! entries and provides cursor-based navigation for Up/Down keys and search.

/// History navigation for the editor.
///
/// Maintains a cursor into the history list and saves the "current" unsaved
/// input so the user can return to it by pressing Down past the end.
#[derive(Debug, Clone)]
pub struct EditorHistory {
    /// All history entries, ordered from oldest to newest.
    entries: Vec<String>,
    /// Current navigation cursor. `None` means we're at the live input.
    cursor: Option<usize>,
    /// The saved live input (what the user was typing before navigating history).
    saved_input: String,
}

impl Default for EditorHistory {
    fn default() -> Self {
        Self::new()
    }
}

impl EditorHistory {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            cursor: None,
            saved_input: String::new(),
        }
    }

    /// Create history pre-populated with entries.
    pub fn with_entries(entries: Vec<String>) -> Self {
        Self {
            entries,
            cursor: None,
            saved_input: String::new(),
        }
    }

    /// Add a new entry to the end of history.
    pub fn add(&mut self, entry: String) {
        // Don't add duplicates of the last entry
        if self.entries.last() == Some(&entry) {
            return;
        }
        // Don't add empty entries
        if entry.trim().is_empty() {
            return;
        }
        self.entries.push(entry);
        self.reset_cursor();
    }

    /// Get the total number of history entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Is the history empty?
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get all entries.
    pub fn entries(&self) -> &[String] {
        &self.entries
    }

    /// Reset the navigation cursor (called after submitting a command).
    pub fn reset_cursor(&mut self) {
        self.cursor = None;
        self.saved_input.clear();
    }

    /// Navigate to the previous (older) history entry.
    ///
    /// `current` is the current editor buffer text, which is saved on the
    /// first navigation so the user can return to it.
    ///
    /// Returns `Some(&str)` with the history entry to display, or `None` if
    /// already at the oldest entry.
    pub fn prev(&mut self, current: &str) -> Option<&str> {
        if self.entries.is_empty() {
            return None;
        }

        match self.cursor {
            None => {
                // First navigation: save current input, go to most recent
                self.saved_input = current.to_string();
                let idx = self.entries.len() - 1;
                self.cursor = Some(idx);
                Some(&self.entries[idx])
            }
            Some(idx) => {
                if idx > 0 {
                    let new_idx = idx - 1;
                    self.cursor = Some(new_idx);
                    Some(&self.entries[new_idx])
                } else {
                    None // Already at oldest
                }
            }
        }
    }

    /// Navigate to the next (newer) history entry.
    ///
    /// Returns `Some(&str)` with the entry to display, or `None` if past the
    /// newest entry (returns to saved live input via `saved_input()`).
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<&str> {
        match self.cursor {
            None => None, // Already at live input
            Some(idx) => {
                if idx + 1 < self.entries.len() {
                    let new_idx = idx + 1;
                    self.cursor = Some(new_idx);
                    Some(&self.entries[new_idx])
                } else {
                    // Past newest entry, return to live input
                    self.cursor = None;
                    None
                }
            }
        }
    }

    /// Get the saved input that was being typed before history navigation began.
    pub fn saved_input(&self) -> &str {
        &self.saved_input
    }

    /// Returns true if the cursor is at the live input (not navigating history).
    pub fn at_live_input(&self) -> bool {
        self.cursor.is_none()
    }

    /// Search history entries for those matching the given pattern.
    ///
    /// Returns a vec of `(index, entry)` pairs, most recent first.
    pub fn search(&self, pattern: &str) -> Vec<(usize, &str)> {
        if pattern.is_empty() {
            return Vec::new();
        }

        let pattern_lower = pattern.to_lowercase();
        self.entries
            .iter()
            .enumerate()
            .rev()
            .filter(|(_, entry)| entry.to_lowercase().contains(&pattern_lower))
            .map(|(idx, entry)| (idx, entry.as_str()))
            .collect()
    }

    /// Fuzzy search history entries.
    ///
    /// Returns entries where all characters of the pattern appear in order.
    pub fn fuzzy_search(&self, pattern: &str) -> Vec<(usize, &str)> {
        if pattern.is_empty() {
            return Vec::new();
        }

        let pattern_lower = pattern.to_lowercase();

        self.entries
            .iter()
            .enumerate()
            .rev()
            .filter(|(_, entry)| {
                let entry_lower = entry.to_lowercase();
                let mut entry_chars = entry_lower.chars();
                for pc in pattern_lower.chars() {
                    loop {
                        match entry_chars.next() {
                            Some(ec) if ec == pc => break,
                            Some(_) => continue,
                            None => return false,
                        }
                    }
                }
                true
            })
            .map(|(idx, entry)| (idx, entry.as_str()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_history_is_empty() {
        let h = EditorHistory::new();
        assert!(h.is_empty());
        assert_eq!(h.len(), 0);
        assert!(h.at_live_input());
    }

    #[test]
    fn test_add_entries() {
        let mut h = EditorHistory::new();
        h.add("ls".to_string());
        h.add("pwd".to_string());
        assert_eq!(h.len(), 2);
    }

    #[test]
    fn test_add_skips_duplicates() {
        let mut h = EditorHistory::new();
        h.add("ls".to_string());
        h.add("ls".to_string());
        assert_eq!(h.len(), 1);
    }

    #[test]
    fn test_add_skips_empty() {
        let mut h = EditorHistory::new();
        h.add("".to_string());
        h.add("   ".to_string());
        assert_eq!(h.len(), 0);
    }

    #[test]
    fn test_prev_navigation() {
        let mut h = EditorHistory::with_entries(vec![
            "first".to_string(),
            "second".to_string(),
            "third".to_string(),
        ]);

        let entry = h.prev("current typing");
        assert_eq!(entry, Some("third"));
        assert_eq!(h.saved_input(), "current typing");

        let entry = h.prev("current typing");
        assert_eq!(entry, Some("second"));

        let entry = h.prev("current typing");
        assert_eq!(entry, Some("first"));

        // At oldest, returns None
        let entry = h.prev("current typing");
        assert!(entry.is_none());
    }

    #[test]
    fn test_next_navigation() {
        let mut h = EditorHistory::with_entries(vec![
            "first".to_string(),
            "second".to_string(),
        ]);

        // Navigate to oldest
        h.prev("typing");
        h.prev("typing");

        // Navigate forward
        let entry = h.next();
        assert_eq!(entry, Some("second"));

        // Past newest returns None (live input)
        let entry = h.next();
        assert!(entry.is_none());
        assert!(h.at_live_input());
    }

    #[test]
    fn test_prev_empty_history() {
        let mut h = EditorHistory::new();
        assert!(h.prev("anything").is_none());
    }

    #[test]
    fn test_next_at_live_input() {
        let mut h = EditorHistory::with_entries(vec!["ls".to_string()]);
        assert!(h.next().is_none());
    }

    #[test]
    fn test_reset_cursor() {
        let mut h = EditorHistory::with_entries(vec!["first".to_string()]);
        h.prev("typing");
        assert!(!h.at_live_input());

        h.reset_cursor();
        assert!(h.at_live_input());
        assert_eq!(h.saved_input(), "");
    }

    #[test]
    fn test_search() {
        let h = EditorHistory::with_entries(vec![
            "git status".to_string(),
            "ls -la".to_string(),
            "git push".to_string(),
            "echo hello".to_string(),
        ]);

        let results = h.search("git");
        assert_eq!(results.len(), 2);
        // Most recent first
        assert_eq!(results[0].1, "git push");
        assert_eq!(results[1].1, "git status");
    }

    #[test]
    fn test_search_case_insensitive() {
        let h = EditorHistory::with_entries(vec![
            "Git Status".to_string(),
            "GIT PUSH".to_string(),
        ]);

        let results = h.search("git");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_search_empty_pattern() {
        let h = EditorHistory::with_entries(vec!["ls".to_string()]);
        let results = h.search("");
        assert!(results.is_empty());
    }

    #[test]
    fn test_fuzzy_search() {
        let h = EditorHistory::with_entries(vec![
            "git status".to_string(),
            "git commit -m 'test'".to_string(),
            "ls -la".to_string(),
        ]);

        let results = h.fuzzy_search("gcm");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, "git commit -m 'test'");
    }

    #[test]
    fn test_fuzzy_search_no_match() {
        let h = EditorHistory::with_entries(vec!["ls -la".to_string()]);
        let results = h.fuzzy_search("xyz");
        assert!(results.is_empty());
    }

    #[test]
    fn test_with_entries() {
        let entries = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let h = EditorHistory::with_entries(entries.clone());
        assert_eq!(h.entries(), &entries);
    }

    #[test]
    fn test_saved_input_preserved_during_navigation() {
        let mut h = EditorHistory::with_entries(vec![
            "old".to_string(),
        ]);

        h.prev("my current work");
        assert_eq!(h.saved_input(), "my current work");

        // Even after navigating, saved_input stays
        h.next();
        assert_eq!(h.saved_input(), "my current work");
    }
}
