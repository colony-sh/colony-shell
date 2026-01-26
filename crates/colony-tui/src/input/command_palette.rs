//! Command palette for quick action access
//!
//! Provides a searchable popup of available commands/actions.
//! Triggered by Shift+Command+P (like VS Code).

/// A command palette entry
#[derive(Debug, Clone)]
pub struct PaletteEntry {
    /// Display label
    pub label: String,
    /// Optional description
    pub description: Option<String>,
    /// Keyboard shortcut hint
    pub shortcut: Option<String>,
    /// Category for grouping
    pub category: PaletteCategory,
    /// Action to execute
    pub action: PaletteAction,
}

/// Categories for palette entries
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteCategory {
    /// File operations
    File,
    /// Edit operations
    Edit,
    /// View/UI operations
    View,
    /// Navigation
    Navigation,
    /// Shell operations
    Shell,
    /// Git operations
    Git,
    /// Agent/AI operations
    Agent,
    /// Settings
    Settings,
}

impl PaletteCategory {
    pub fn label(&self) -> &'static str {
        match self {
            Self::File => "File",
            Self::Edit => "Edit",
            Self::View => "View",
            Self::Navigation => "Navigation",
            Self::Shell => "Shell",
            Self::Git => "Git",
            Self::Agent => "Agent",
            Self::Settings => "Settings",
        }
    }
}

/// Actions that can be triggered from the palette
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaletteAction {
    // View actions
    ToggleLeftSidebar,
    ToggleRightSidebar,
    ClearScreen,
    ScrollToTop,
    ScrollToBottom,

    // Mode actions
    EnterNormalMode,
    EnterInsertMode,

    // History actions
    ShowHistory,
    ClearHistory,

    // Shell actions
    Quit,

    // Navigation
    FocusInput,
    FocusContent,

    // Custom command (for extensibility)
    RunCommand(String),
}

/// Command palette state
#[derive(Debug)]
pub struct CommandPalette {
    /// Whether the palette is active
    active: bool,
    /// Current search query
    query: String,
    /// All available entries
    entries: Vec<PaletteEntry>,
    /// Filtered results
    results: Vec<usize>, // indices into entries
    /// Currently selected index
    selected: usize,
}

impl CommandPalette {
    /// Create a new command palette with default entries
    pub fn new() -> Self {
        let entries = Self::default_entries();
        let results: Vec<usize> = (0..entries.len()).collect();

        Self {
            active: false,
            query: String::new(),
            entries,
            results,
            selected: 0,
        }
    }

    /// Default palette entries
    fn default_entries() -> Vec<PaletteEntry> {
        vec![
            // View
            PaletteEntry {
                label: "Toggle Left Sidebar".to_string(),
                description: Some("Show/hide the left sidebar".to_string()),
                shortcut: Some("Ctrl+B".to_string()),
                category: PaletteCategory::View,
                action: PaletteAction::ToggleLeftSidebar,
            },
            PaletteEntry {
                label: "Toggle Right Sidebar".to_string(),
                description: Some("Show/hide the right sidebar".to_string()),
                shortcut: Some("Ctrl+N".to_string()),
                category: PaletteCategory::View,
                action: PaletteAction::ToggleRightSidebar,
            },
            PaletteEntry {
                label: "Clear Screen".to_string(),
                description: Some("Clear the terminal output".to_string()),
                shortcut: None,
                category: PaletteCategory::View,
                action: PaletteAction::ClearScreen,
            },
            PaletteEntry {
                label: "Scroll to Top".to_string(),
                description: Some("Jump to the beginning of output".to_string()),
                shortcut: Some("g".to_string()),
                category: PaletteCategory::View,
                action: PaletteAction::ScrollToTop,
            },
            PaletteEntry {
                label: "Scroll to Bottom".to_string(),
                description: Some("Jump to the end of output".to_string()),
                shortcut: Some("G".to_string()),
                category: PaletteCategory::View,
                action: PaletteAction::ScrollToBottom,
            },
            // Edit
            PaletteEntry {
                label: "Normal Mode".to_string(),
                description: Some("Enter vim normal mode".to_string()),
                shortcut: Some("Esc".to_string()),
                category: PaletteCategory::Edit,
                action: PaletteAction::EnterNormalMode,
            },
            PaletteEntry {
                label: "Insert Mode".to_string(),
                description: Some("Enter vim insert mode".to_string()),
                shortcut: Some("i".to_string()),
                category: PaletteCategory::Edit,
                action: PaletteAction::EnterInsertMode,
            },
            // Navigation
            PaletteEntry {
                label: "Focus Input".to_string(),
                description: Some("Move focus to command input".to_string()),
                shortcut: None,
                category: PaletteCategory::Navigation,
                action: PaletteAction::FocusInput,
            },
            PaletteEntry {
                label: "Focus Content".to_string(),
                description: Some("Move focus to output content".to_string()),
                shortcut: None,
                category: PaletteCategory::Navigation,
                action: PaletteAction::FocusContent,
            },
            // Shell
            PaletteEntry {
                label: "Search History".to_string(),
                description: Some("Fuzzy search command history".to_string()),
                shortcut: Some("Ctrl+R".to_string()),
                category: PaletteCategory::Shell,
                action: PaletteAction::ShowHistory,
            },
            PaletteEntry {
                label: "Clear History".to_string(),
                description: Some("Clear command history".to_string()),
                shortcut: None,
                category: PaletteCategory::Shell,
                action: PaletteAction::ClearHistory,
            },
            PaletteEntry {
                label: "Quit".to_string(),
                description: Some("Exit the shell".to_string()),
                shortcut: Some(":q".to_string()),
                category: PaletteCategory::Shell,
                action: PaletteAction::Quit,
            },
            // Git (common commands)
            PaletteEntry {
                label: "Git Status".to_string(),
                description: Some("Show git status".to_string()),
                shortcut: None,
                category: PaletteCategory::Git,
                action: PaletteAction::RunCommand("git status".to_string()),
            },
            PaletteEntry {
                label: "Git Diff".to_string(),
                description: Some("Show unstaged changes".to_string()),
                shortcut: None,
                category: PaletteCategory::Git,
                action: PaletteAction::RunCommand("git diff".to_string()),
            },
            PaletteEntry {
                label: "Git Log".to_string(),
                description: Some("Show commit history".to_string()),
                shortcut: None,
                category: PaletteCategory::Git,
                action: PaletteAction::RunCommand("git log --oneline -20".to_string()),
            },
            PaletteEntry {
                label: "Git Branch".to_string(),
                description: Some("List branches".to_string()),
                shortcut: None,
                category: PaletteCategory::Git,
                action: PaletteAction::RunCommand("git branch -a".to_string()),
            },
        ]
    }

    /// Check if palette is active
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Activate the palette
    pub fn activate(&mut self) {
        self.active = true;
        self.query.clear();
        self.selected = 0;
        self.update_results();
    }

    /// Deactivate the palette
    pub fn deactivate(&mut self) {
        self.active = false;
        self.query.clear();
        self.results = (0..self.entries.len()).collect();
    }

    /// Get the current query
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Get filtered results
    pub fn results(&self) -> Vec<&PaletteEntry> {
        self.results
            .iter()
            .filter_map(|&idx| self.entries.get(idx))
            .collect()
    }

    /// Get currently selected index
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// Get the selected entry
    pub fn selected_entry(&self) -> Option<&PaletteEntry> {
        self.results
            .get(self.selected)
            .and_then(|&idx| self.entries.get(idx))
    }

    /// Insert a character into the query
    pub fn insert_char(&mut self, c: char) {
        self.query.push(c);
        self.update_results();
        self.selected = 0;
    }

    /// Delete the last character
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
    /// Returns the action to execute
    pub fn accept(&mut self) -> Option<PaletteAction> {
        let action = self.selected_entry().map(|e| e.action.clone());
        self.deactivate();
        action
    }

    /// Update results based on query
    fn update_results(&mut self) {
        if self.query.is_empty() {
            self.results = (0..self.entries.len()).collect();
            return;
        }

        let query_lower = self.query.to_lowercase();
        let query_chars: Vec<char> = query_lower.chars().collect();

        let mut scored: Vec<(usize, i64)> = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(idx, entry)| {
                // Check label
                if let Some(score) = fuzzy_score(&entry.label, &query_chars) {
                    return Some((idx, score + 10)); // Label match bonus
                }
                // Check description
                if let Some(ref desc) = entry.description {
                    if let Some(score) = fuzzy_score(desc, &query_chars) {
                        return Some((idx, score));
                    }
                }
                // Check category
                if let Some(score) = fuzzy_score(entry.category.label(), &query_chars) {
                    return Some((idx, score - 5)); // Category match penalty
                }
                None
            })
            .collect();

        // Sort by score (higher first)
        scored.sort_by(|a, b| b.1.cmp(&a.1));

        self.results = scored.into_iter().map(|(idx, _)| idx).collect();
    }

    /// Add a custom entry
    pub fn add_entry(&mut self, entry: PaletteEntry) {
        self.entries.push(entry);
        if !self.active {
            self.results.push(self.entries.len() - 1);
        }
    }
}

impl Default for CommandPalette {
    fn default() -> Self {
        Self::new()
    }
}

/// Fuzzy score a string against query characters
fn fuzzy_score(text: &str, query_chars: &[char]) -> Option<i64> {
    if query_chars.is_empty() {
        return Some(0);
    }

    let text_lower = text.to_lowercase();
    let text_chars: Vec<char> = text_lower.chars().collect();

    let mut score: i64 = 0;
    let mut query_idx = 0;
    let mut last_match_pos: Option<usize> = None;

    for (text_idx, text_char) in text_chars.iter().enumerate() {
        if query_idx < query_chars.len() && *text_char == query_chars[query_idx] {
            // Consecutive match bonus
            if let Some(last_pos) = last_match_pos {
                if text_idx == last_pos + 1 {
                    score += 10;
                }
            }

            // Word boundary bonus
            if text_idx == 0 {
                score += 15;
            } else {
                let prev_char = text_chars.get(text_idx - 1);
                if matches!(prev_char, Some(' ') | Some('/') | Some('-') | Some('_')) {
                    score += 10;
                }
            }

            score += 1;
            last_match_pos = Some(text_idx);
            query_idx += 1;
        }
    }

    if query_idx == query_chars.len() {
        // Penalty for longer strings
        score -= (text.len() / 10) as i64;
        Some(score)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_palette_activate() {
        let mut palette = CommandPalette::new();
        assert!(!palette.is_active());

        palette.activate();
        assert!(palette.is_active());
        assert!(palette.results().len() > 0);
    }

    #[test]
    fn test_palette_filter() {
        let mut palette = CommandPalette::new();
        palette.activate();

        palette.insert_char('g');
        palette.insert_char('i');
        palette.insert_char('t');

        // Should filter to git-related entries
        let results = palette.results();
        assert!(results
            .iter()
            .any(|e| e.label.to_lowercase().contains("git")));
    }

    #[test]
    fn test_palette_select() {
        let mut palette = CommandPalette::new();
        palette.activate();

        assert_eq!(palette.selected(), 0);

        palette.select_next();
        assert_eq!(palette.selected(), 1);

        palette.select_prev();
        assert_eq!(palette.selected(), 0);

        // Wrap around
        palette.select_prev();
        assert!(palette.selected() > 0);
    }

    #[test]
    fn test_palette_accept() {
        let mut palette = CommandPalette::new();
        palette.activate();

        let action = palette.accept();
        assert!(action.is_some());
        assert!(!palette.is_active());
    }

    #[test]
    fn test_fuzzy_score() {
        // Exact prefix should score high
        let score1 = fuzzy_score("Toggle Left Sidebar", &['t', 'o', 'g']);
        assert!(score1.is_some());

        // Scattered match should also work
        let score2 = fuzzy_score("Toggle Left Sidebar", &['t', 'l', 's']);
        assert!(score2.is_some());

        // No match
        let score3 = fuzzy_score("Toggle Left Sidebar", &['z', 'z', 'z']);
        assert!(score3.is_none());
    }
}
