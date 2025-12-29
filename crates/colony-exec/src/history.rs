//! Shell command history
//!
//! Provides command history storage, persistence, and the `history` builtin.
//! Follows POSIX and bash semantics:
//! - Commands stored in memory during session
//! - Persisted to HISTFILE on exit and loaded on startup
//! - HISTSIZE controls maximum entries in memory
//! - HISTFILESIZE controls maximum entries in file
//!
//! ## Environment Variables
//! - `HISTFILE`: Path to history file (default: ~/.colony_history)
//! - `HISTSIZE`: Max entries in memory (default: 500)
//! - `HISTFILESIZE`: Max entries in file (default: 500)
//! - `HISTCONTROL`: Controls which commands are saved
//!   - `ignorespace`: Don't save commands starting with space
//!   - `ignoredups`: Don't save duplicate consecutive commands
//!   - `ignoreboth`: Both of the above
//!   - `erasedups`: Erase older duplicates when saving

use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

/// Default maximum history size
pub const DEFAULT_HISTSIZE: usize = 500;
/// Default history file name
pub const DEFAULT_HISTFILE: &str = ".colony_history";

/// History control options
#[derive(Debug, Clone, Default)]
pub struct HistoryControl {
    /// Don't save commands starting with space
    pub ignorespace: bool,
    /// Don't save duplicate consecutive commands  
    pub ignoredups: bool,
    /// Erase older duplicates when saving
    pub erasedups: bool,
}

impl HistoryControl {
    /// Parse HISTCONTROL environment variable
    pub fn from_env(value: &str) -> Self {
        let mut control = Self::default();
        for part in value.split(':') {
            match part.trim() {
                "ignorespace" => control.ignorespace = true,
                "ignoredups" => control.ignoredups = true,
                "ignoreboth" => {
                    control.ignorespace = true;
                    control.ignoredups = true;
                }
                "erasedups" => control.erasedups = true,
                _ => {}
            }
        }
        control
    }
}

/// Command history manager
#[derive(Debug)]
pub struct History {
    /// Command entries (newest at end)
    entries: VecDeque<String>,
    /// Maximum entries to keep in memory
    max_size: usize,
    /// Path to history file
    file_path: Option<PathBuf>,
    /// Maximum entries to keep in file
    file_max_size: usize,
    /// History control settings
    control: HistoryControl,
    /// Current position for navigation (for future readline integration)
    position: usize,
}

impl History {
    /// Create new history with default settings
    pub fn new() -> Self {
        Self {
            entries: VecDeque::new(),
            max_size: DEFAULT_HISTSIZE,
            file_path: None,
            file_max_size: DEFAULT_HISTSIZE,
            control: HistoryControl::default(),
            position: 0,
        }
    }

    /// Create history and load from file
    pub fn with_file(path: PathBuf) -> Self {
        let mut history = Self::new();
        history.file_path = Some(path.clone());
        history.load_from_file();
        history
    }

    /// Set maximum history size
    pub fn set_max_size(&mut self, size: usize) {
        self.max_size = size;
        self.truncate();
    }

    /// Set maximum file size
    pub fn set_file_max_size(&mut self, size: usize) {
        self.file_max_size = size;
    }

    /// Set history file path
    pub fn set_file_path(&mut self, path: PathBuf) {
        self.file_path = Some(path);
    }

    /// Set history control options
    pub fn set_control(&mut self, control: HistoryControl) {
        self.control = control;
    }

    /// Configure from environment variables
    pub fn configure_from_env(&mut self, get_env: impl Fn(&str) -> Option<String>) {
        // HISTSIZE
        if let Some(size_str) = get_env("HISTSIZE") {
            if let Ok(size) = size_str.parse::<usize>() {
                self.set_max_size(size);
            }
        }

        // HISTFILESIZE
        if let Some(size_str) = get_env("HISTFILESIZE") {
            if let Ok(size) = size_str.parse::<usize>() {
                self.set_file_max_size(size);
            }
        }

        // HISTFILE
        if let Some(path) = get_env("HISTFILE") {
            self.set_file_path(PathBuf::from(path));
        } else if let Some(home) = get_env("HOME") {
            self.set_file_path(PathBuf::from(home).join(DEFAULT_HISTFILE));
        }

        // HISTCONTROL
        if let Some(control_str) = get_env("HISTCONTROL") {
            self.set_control(HistoryControl::from_env(&control_str));
        }
    }

    /// Add a command to history
    ///
    /// Returns true if the command was added, false if skipped due to HISTCONTROL
    pub fn add(&mut self, command: &str) -> bool {
        // HISTCONTROL: ignorespace - check BEFORE trimming
        if self.control.ignorespace && command.starts_with(' ') {
            return false;
        }

        let command = command.trim();

        if command.is_empty() {
            return false;
        }

        // HISTCONTROL: ignoredups - skip if same as last command
        if self.control.ignoredups {
            if let Some(last) = self.entries.back() {
                if last == command {
                    return false;
                }
            }
        }

        // HISTCONTROL: erasedups - remove all previous occurrences
        if self.control.erasedups {
            self.entries.retain(|e| e != command);
        }

        self.entries.push_back(command.to_string());
        self.truncate();

        self.position = self.entries.len();

        true
    }

    /// Truncate history to max size
    fn truncate(&mut self) {
        while self.entries.len() > self.max_size && self.max_size > 0 {
            self.entries.pop_front();
        }
    }

    /// Get all history entries
    pub fn entries(&self) -> impl Iterator<Item = &String> {
        self.entries.iter()
    }

    /// Get history entry by index (1-based, like bash)
    pub fn get(&self, index: usize) -> Option<&String> {
        if index == 0 || index > self.entries.len() {
            None
        } else {
            self.entries.get(index - 1)
        }
    }

    /// Get last N entries
    pub fn last_n(&self, n: usize) -> Vec<&String> {
        let start = if n >= self.entries.len() {
            0
        } else {
            self.entries.len() - n
        };
        self.entries.iter().skip(start).collect()
    }

    /// Get number of entries
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if history is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Clear all history
    pub fn clear(&mut self) {
        self.entries.clear();
        self.position = 0;
    }

    /// Delete entry at index (1-based)
    pub fn delete(&mut self, index: usize) -> bool {
        if index == 0 || index > self.entries.len() {
            false
        } else {
            self.entries.remove(index - 1);
            true
        }
    }

    /// Load history from file
    pub fn load_from_file(&mut self) {
        let path = match &self.file_path {
            Some(p) => p.clone(),
            None => return,
        };

        let file = match File::open(&path) {
            Ok(f) => f,
            Err(_) => return, // File doesn't exist yet
        };

        let reader = BufReader::new(file);
        for line in reader.lines() {
            if let Ok(cmd) = line {
                let cmd = cmd.trim();
                if !cmd.is_empty() {
                    self.entries.push_back(cmd.to_string());
                }
            }
        }

        // Truncate to max size
        self.truncate();
        self.position = self.entries.len();
    }

    /// Save history to file
    pub fn save_to_file(&self) -> std::io::Result<()> {
        let path = match &self.file_path {
            Some(p) => p.clone(),
            None => return Ok(()),
        };

        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut file = File::create(&path)?;

        // Only save last file_max_size entries
        let start = if self.entries.len() > self.file_max_size {
            self.entries.len() - self.file_max_size
        } else {
            0
        };

        for entry in self.entries.iter().skip(start) {
            writeln!(file, "{}", entry)?;
        }

        Ok(())
    }

    /// Append a single command to history file (for immediate persistence)
    pub fn append_to_file(&self, command: &str) -> std::io::Result<()> {
        let path = match &self.file_path {
            Some(p) => p.clone(),
            None => return Ok(()),
        };

        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut file = OpenOptions::new().create(true).append(true).open(&path)?;

        writeln!(file, "{}", command)?;

        Ok(())
    }

    /// Search history for commands containing pattern
    pub fn search(&self, pattern: &str) -> Vec<(usize, &String)> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, cmd)| cmd.contains(pattern))
            .map(|(i, cmd)| (i + 1, cmd)) // Convert to 1-based index
            .collect()
    }

    /// Search history backwards for command starting with prefix
    pub fn search_prefix(&self, prefix: &str) -> Option<&String> {
        self.entries
            .iter()
            .rev()
            .find(|cmd| cmd.starts_with(prefix))
    }

    // Navigation methods for readline integration

    /// Move to previous entry (older)
    pub fn previous(&mut self) -> Option<&String> {
        if self.position > 0 {
            self.position -= 1;
            self.entries.get(self.position)
        } else {
            None
        }
    }

    /// Move to next entry (newer)
    pub fn next(&mut self) -> Option<&String> {
        if self.position < self.entries.len() {
            self.position += 1;
            if self.position < self.entries.len() {
                self.entries.get(self.position)
            } else {
                None // At end, return None for "current line"
            }
        } else {
            None
        }
    }

    /// Reset position to end
    pub fn reset_position(&mut self) {
        self.position = self.entries.len();
    }

    /// Get current position
    pub fn position(&self) -> usize {
        self.position
    }

    /// Expand history references in input line
    ///
    /// Supports:
    /// - `!!` - last command
    /// - `!N` - command number N (1-based)
    /// - `!-N` - Nth previous command (relative)
    /// - `!string` - most recent command starting with string
    ///
    /// Returns (expanded_line, was_expanded)
    pub fn expand(&self, line: &str) -> Result<(String, bool), HistoryExpansionError> {
        if !line.contains('!') {
            return Ok((line.to_string(), false));
        }

        let mut result = String::with_capacity(line.len());
        let mut chars = line.chars().peekable();
        let mut expanded = false;
        let mut in_single_quote = false;

        while let Some(c) = chars.next() {
            // Track single quotes - no expansion inside them
            if c == '\'' {
                in_single_quote = !in_single_quote;
                result.push(c);
                continue;
            }

            if in_single_quote {
                result.push(c);
                continue;
            }

            if c != '!' {
                result.push(c);
                continue;
            }

            // Handle ! - look at next character
            match chars.peek() {
                None => {
                    // Trailing ! - keep as-is
                    result.push('!');
                }
                Some('!') => {
                    // !! - last command
                    chars.next();
                    if self.entries.is_empty() {
                        return Err(HistoryExpansionError::NoHistory);
                    }
                    let last = self.entries.back().unwrap();
                    result.push_str(last);
                    expanded = true;
                }
                Some('-') => {
                    // !-N - relative reference
                    chars.next();
                    let num_str: String =
                        chars.by_ref().take_while(|c| c.is_ascii_digit()).collect();

                    if num_str.is_empty() {
                        return Err(HistoryExpansionError::InvalidReference("!-".to_string()));
                    }

                    let n: usize = num_str.parse().map_err(|_| {
                        HistoryExpansionError::InvalidReference(format!("!-{}", num_str))
                    })?;

                    if n == 0 || n > self.entries.len() {
                        return Err(HistoryExpansionError::OutOfRange(format!("!-{}", n)));
                    }

                    let idx = self.entries.len() - n;
                    result.push_str(&self.entries[idx]);
                    expanded = true;
                }
                Some(c) if c.is_ascii_digit() => {
                    // !N - absolute reference
                    let num_str: String =
                        chars.by_ref().take_while(|c| c.is_ascii_digit()).collect();

                    let n: usize = num_str.parse().map_err(|_| {
                        HistoryExpansionError::InvalidReference(format!("!{}", num_str))
                    })?;

                    if n == 0 || n > self.entries.len() {
                        return Err(HistoryExpansionError::OutOfRange(format!("!{}", n)));
                    }

                    result.push_str(&self.entries[n - 1]);
                    expanded = true;
                }
                Some(c) if c.is_alphabetic() || *c == '_' => {
                    // !string - search for command starting with string
                    let prefix: String = chars
                        .by_ref()
                        .take_while(|c| !c.is_whitespace() && *c != '!')
                        .collect();

                    match self.search_prefix(&prefix) {
                        Some(cmd) => {
                            result.push_str(cmd);
                            expanded = true;
                        }
                        None => {
                            return Err(HistoryExpansionError::NotFound(format!("!{}", prefix)));
                        }
                    }
                }
                Some(' ') | Some('\t') | Some('\n') => {
                    // ! followed by whitespace - not an expansion
                    result.push('!');
                }
                Some(_) => {
                    // Unknown pattern - keep the !
                    result.push('!');
                }
            }
        }

        Ok((result, expanded))
    }
}

/// Errors that can occur during history expansion
#[derive(Debug, Clone, PartialEq)]
pub enum HistoryExpansionError {
    NoHistory,
    OutOfRange(String),
    InvalidReference(String),
    NotFound(String),
}

impl std::fmt::Display for HistoryExpansionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoHistory => write!(f, "!!: event not found"),
            Self::OutOfRange(ref s) => write!(f, "{}: event not found", s),
            Self::InvalidReference(ref s) => write!(f, "{}: invalid history reference", s),
            Self::NotFound(ref s) => write!(f, "{}: event not found", s),
        }
    }
}

impl std::error::Error for HistoryExpansionError {}

pub fn execute_history_builtin(args: &[String], history: &mut History) -> i32 {
    let mut show_count: Option<usize> = None;
    let mut clear = false;
    let mut delete_index: Option<i64> = None;
    let mut append = false;
    let mut read = false;
    let mut write = false;
    let mut suppress_numbers = false;

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];

        if arg == "-c" {
            clear = true;
        } else if arg == "-d" {
            i += 1;
            if i < args.len() {
                if let Ok(n) = args[i].parse::<i64>() {
                    delete_index = Some(n);
                } else {
                    eprintln!("history: {}: invalid number", args[i]);
                    return 1;
                }
            } else {
                eprintln!("history: -d: option requires an argument");
                return 1;
            }
        } else if arg == "-a" {
            append = true;
        } else if arg == "-r" {
            read = true;
        } else if arg == "-w" {
            write = true;
        } else if arg == "-n" {
            suppress_numbers = true;
        } else if arg.starts_with('-') {
            eprintln!("history: {}: invalid option", arg);
            return 1;
        } else if let Ok(n) = arg.parse::<usize>() {
            show_count = Some(n);
        }

        i += 1;
    }

    if clear {
        history.clear();
        return 0;
    }

    if let Some(idx) = delete_index {
        let actual_idx = if idx < 0 {
            let len = history.len() as i64;
            if -idx > len {
                eprintln!("history: {}: history position out of range", idx);
                return 1;
            }
            (len + idx + 1) as usize
        } else {
            idx as usize
        };

        if !history.delete(actual_idx) {
            eprintln!("history: {}: history position out of range", idx);
            return 1;
        }
        return 0;
    }

    if append {
        if let Err(e) = history.save_to_file() {
            eprintln!("history: cannot append: {}", e);
            return 1;
        }
        return 0;
    }

    if read {
        history.load_from_file();
        return 0;
    }

    if write {
        if let Err(e) = history.save_to_file() {
            eprintln!("history: cannot write: {}", e);
            return 1;
        }
        return 0;
    }

    let entries: Vec<_> = if let Some(n) = show_count {
        history.last_n(n).into_iter().collect()
    } else {
        history.entries().collect()
    };

    let total = history.len();
    let start_num = if let Some(n) = show_count {
        if n < total {
            total - n + 1
        } else {
            1
        }
    } else {
        1
    };

    for (i, entry) in entries.iter().enumerate() {
        if suppress_numbers {
            println!("{}", entry);
        } else {
            println!("{:>5}  {}", start_num + i, entry);
        }
    }

    0
}

#[derive(Debug, Clone)]
pub struct HistorySearchResult {
    pub command: String,
    pub index: usize,
    pub match_start: usize,
    pub match_end: usize,
}

#[derive(Debug)]
pub struct InteractiveHistorySearch<'a> {
    history: &'a History,
    query: String,
    results: Vec<HistorySearchResult>,
    selected_index: usize,
}

impl<'a> InteractiveHistorySearch<'a> {
    pub fn new(history: &'a History) -> Self {
        Self {
            history,
            query: String::new(),
            results: Vec::new(),
            selected_index: 0,
        }
    }

    pub fn update_query(&mut self, query: &str) {
        self.query = query.to_string();
        self.results.clear();
        self.selected_index = 0;

        if query.is_empty() {
            return;
        }

        let query_lower = query.to_lowercase();

        for (idx, cmd) in self.history.entries.iter().enumerate().rev() {
            let cmd_lower = cmd.to_lowercase();
            if let Some(pos) = cmd_lower.find(&query_lower) {
                self.results.push(HistorySearchResult {
                    command: cmd.clone(),
                    index: idx + 1,
                    match_start: pos,
                    match_end: pos + query.len(),
                });
            }
        }
    }

    pub fn select_next(&mut self) {
        if !self.results.is_empty() && self.selected_index + 1 < self.results.len() {
            self.selected_index += 1;
        }
    }

    pub fn select_prev(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    pub fn selected(&self) -> Option<&HistorySearchResult> {
        self.results.get(self.selected_index)
    }

    pub fn results(&self) -> &[HistorySearchResult] {
        &self.results
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn selected_index(&self) -> usize {
        self.selected_index
    }
}

impl History {
    pub fn interactive_search(&self) -> InteractiveHistorySearch<'_> {
        InteractiveHistorySearch::new(self)
    }

    pub fn fuzzy_search(&self, query: &str, max_results: usize) -> Vec<HistorySearchResult> {
        if query.is_empty() {
            return self
                .entries
                .iter()
                .enumerate()
                .rev()
                .take(max_results)
                .map(|(idx, cmd)| HistorySearchResult {
                    command: cmd.clone(),
                    index: idx + 1,
                    match_start: 0,
                    match_end: 0,
                })
                .collect();
        }

        let query_lower = query.to_lowercase();
        let mut results = Vec::new();

        for (idx, cmd) in self.history_entries_with_scores(&query_lower) {
            if results.len() >= max_results {
                break;
            }

            let cmd_lower = cmd.to_lowercase();
            let (match_start, match_end) = if let Some(pos) = cmd_lower.find(&query_lower) {
                (pos, pos + query.len())
            } else {
                (0, 0)
            };

            results.push(HistorySearchResult {
                command: cmd.clone(),
                index: idx + 1,
                match_start,
                match_end,
            });
        }

        results
    }

    fn history_entries_with_scores(&self, query: &str) -> Vec<(usize, &String)> {
        let mut scored: Vec<(usize, &String, i32)> = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(idx, cmd)| {
                let score = Self::fuzzy_score(cmd, query);
                if score > 0 {
                    Some((idx, cmd, score))
                } else {
                    None
                }
            })
            .collect();

        scored.sort_by(|a, b| b.2.cmp(&a.2).then(b.0.cmp(&a.0)));

        scored.into_iter().map(|(idx, cmd, _)| (idx, cmd)).collect()
    }

    fn fuzzy_score(haystack: &str, needle: &str) -> i32 {
        let haystack_lower = haystack.to_lowercase();
        let needle_lower = needle.to_lowercase();

        if haystack_lower.contains(&needle_lower) {
            let mut score = 100;

            if haystack_lower.starts_with(&needle_lower) {
                score += 50;
            }

            if let Some(pos) = haystack_lower.find(&needle_lower) {
                if pos == 0
                    || haystack
                        .chars()
                        .nth(pos - 1)
                        .map(|c| c == ' ' || c == '/')
                        .unwrap_or(false)
                {
                    score += 25;
                }
            }

            return score;
        }

        let mut needle_chars = needle_lower.chars().peekable();
        let mut consecutive = 0;
        let mut score = 0;
        let mut last_match_pos: Option<usize> = None;

        for (pos, c) in haystack_lower.chars().enumerate() {
            if needle_chars.peek() == Some(&c) {
                needle_chars.next();

                if last_match_pos.map(|p| p + 1 == pos).unwrap_or(false) {
                    consecutive += 1;
                    score += 5 + consecutive * 2;
                } else {
                    consecutive = 0;
                    score += 5;
                }

                last_match_pos = Some(pos);
            }
        }

        if needle_chars.peek().is_some() {
            0
        } else {
            score
        }
    }
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_and_get() {
        let mut history = History::new();

        history.add("ls -la");
        history.add("cd /tmp");
        history.add("echo hello");

        assert_eq!(history.len(), 3);
        assert_eq!(history.get(1), Some(&"ls -la".to_string()));
        assert_eq!(history.get(2), Some(&"cd /tmp".to_string()));
        assert_eq!(history.get(3), Some(&"echo hello".to_string()));
        assert_eq!(history.get(0), None);
        assert_eq!(history.get(4), None);
    }

    #[test]
    fn test_max_size() {
        let mut history = History::new();
        history.set_max_size(3);

        history.add("cmd1");
        history.add("cmd2");
        history.add("cmd3");
        history.add("cmd4");

        assert_eq!(history.len(), 3);
        assert_eq!(history.get(1), Some(&"cmd2".to_string()));
        assert_eq!(history.get(3), Some(&"cmd4".to_string()));
    }

    #[test]
    fn test_ignorespace() {
        let mut history = History::new();
        history.set_control(HistoryControl {
            ignorespace: true,
            ..Default::default()
        });

        assert!(history.add("normal command"));
        assert!(!history.add(" secret command"));
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn test_ignoredups() {
        let mut history = History::new();
        history.set_control(HistoryControl {
            ignoredups: true,
            ..Default::default()
        });

        history.add("ls");
        history.add("ls");
        history.add("ls");

        assert_eq!(history.len(), 1);

        history.add("pwd");
        history.add("ls");

        assert_eq!(history.len(), 3); // ls, pwd, ls
    }

    #[test]
    fn test_erasedups() {
        let mut history = History::new();
        history.set_control(HistoryControl {
            erasedups: true,
            ..Default::default()
        });

        history.add("ls");
        history.add("pwd");
        history.add("ls");

        assert_eq!(history.len(), 2);
        assert_eq!(history.get(1), Some(&"pwd".to_string()));
        assert_eq!(history.get(2), Some(&"ls".to_string()));
    }

    #[test]
    fn test_clear() {
        let mut history = History::new();
        history.add("cmd1");
        history.add("cmd2");

        history.clear();

        assert!(history.is_empty());
    }

    #[test]
    fn test_delete() {
        let mut history = History::new();
        history.add("cmd1");
        history.add("cmd2");
        history.add("cmd3");

        assert!(history.delete(2));
        assert_eq!(history.len(), 2);
        assert_eq!(history.get(1), Some(&"cmd1".to_string()));
        assert_eq!(history.get(2), Some(&"cmd3".to_string()));

        assert!(!history.delete(0));
        assert!(!history.delete(10));
    }

    #[test]
    fn test_last_n() {
        let mut history = History::new();
        history.add("cmd1");
        history.add("cmd2");
        history.add("cmd3");
        history.add("cmd4");

        let last2: Vec<_> = history.last_n(2).into_iter().cloned().collect();
        assert_eq!(last2, vec!["cmd3", "cmd4"]);

        let last10: Vec<_> = history.last_n(10).into_iter().cloned().collect();
        assert_eq!(last10.len(), 4);
    }

    #[test]
    fn test_search() {
        let mut history = History::new();
        history.add("ls -la");
        history.add("cd /tmp");
        history.add("ls /home");
        history.add("echo hello");

        let results = history.search("ls");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, 1);
        assert_eq!(results[1].0, 3);
    }

    #[test]
    fn test_search_prefix() {
        let mut history = History::new();
        history.add("ls -la");
        history.add("cd /tmp");
        history.add("ls /home");

        assert_eq!(history.search_prefix("ls"), Some(&"ls /home".to_string()));
        assert_eq!(history.search_prefix("cd"), Some(&"cd /tmp".to_string()));
        assert_eq!(history.search_prefix("xyz"), None);
    }

    #[test]
    fn test_navigation() {
        let mut history = History::new();
        history.add("cmd1");
        history.add("cmd2");
        history.add("cmd3");

        // Position starts at end
        assert_eq!(history.position(), 3);

        // Go back
        assert_eq!(history.previous(), Some(&"cmd3".to_string()));
        assert_eq!(history.previous(), Some(&"cmd2".to_string()));
        assert_eq!(history.previous(), Some(&"cmd1".to_string()));
        assert_eq!(history.previous(), None);

        // Go forward
        assert_eq!(history.next(), Some(&"cmd2".to_string()));
        assert_eq!(history.next(), Some(&"cmd3".to_string()));
        assert_eq!(history.next(), None); // At end
    }

    #[test]
    fn test_histcontrol_from_env() {
        let control = HistoryControl::from_env("ignorespace:ignoredups");
        assert!(control.ignorespace);
        assert!(control.ignoredups);
        assert!(!control.erasedups);

        let control = HistoryControl::from_env("ignoreboth");
        assert!(control.ignorespace);
        assert!(control.ignoredups);

        let control = HistoryControl::from_env("erasedups");
        assert!(!control.ignorespace);
        assert!(!control.ignoredups);
        assert!(control.erasedups);
    }

    #[test]
    fn test_empty_commands_skipped() {
        let mut history = History::new();
        assert!(!history.add(""));
        assert!(!history.add("   "));
        assert_eq!(history.len(), 0);
    }

    #[test]
    fn test_expand_double_bang() {
        let mut history = History::new();
        history.add("ls -la");
        history.add("echo hello");

        let (expanded, was_expanded) = history.expand("!!").unwrap();
        assert!(was_expanded);
        assert_eq!(expanded, "echo hello");

        let (expanded, was_expanded) = history.expand("sudo !!").unwrap();
        assert!(was_expanded);
        assert_eq!(expanded, "sudo echo hello");
    }

    #[test]
    fn test_expand_number() {
        let mut history = History::new();
        history.add("cmd1");
        history.add("cmd2");
        history.add("cmd3");

        let (expanded, _) = history.expand("!1").unwrap();
        assert_eq!(expanded, "cmd1");

        let (expanded, _) = history.expand("!2").unwrap();
        assert_eq!(expanded, "cmd2");

        let (expanded, _) = history.expand("!3").unwrap();
        assert_eq!(expanded, "cmd3");
    }

    #[test]
    fn test_expand_relative() {
        let mut history = History::new();
        history.add("cmd1");
        history.add("cmd2");
        history.add("cmd3");

        let (expanded, _) = history.expand("!-1").unwrap();
        assert_eq!(expanded, "cmd3");

        let (expanded, _) = history.expand("!-2").unwrap();
        assert_eq!(expanded, "cmd2");

        let (expanded, _) = history.expand("!-3").unwrap();
        assert_eq!(expanded, "cmd1");
    }

    #[test]
    fn test_expand_prefix() {
        let mut history = History::new();
        history.add("ls -la");
        history.add("cd /tmp");
        history.add("ls /home");

        let (expanded, _) = history.expand("!ls").unwrap();
        assert_eq!(expanded, "ls /home");

        let (expanded, _) = history.expand("!cd").unwrap();
        assert_eq!(expanded, "cd /tmp");
    }

    #[test]
    fn test_expand_no_expansion_in_single_quotes() {
        let mut history = History::new();
        history.add("echo hello");

        let (expanded, was_expanded) = history.expand("echo '!!'").unwrap();
        assert!(!was_expanded);
        assert_eq!(expanded, "echo '!!'");
    }

    #[test]
    fn test_expand_errors() {
        let history = History::new();

        assert!(matches!(
            history.expand("!!"),
            Err(HistoryExpansionError::NoHistory)
        ));

        let mut history = History::new();
        history.add("cmd1");

        assert!(matches!(
            history.expand("!0"),
            Err(HistoryExpansionError::OutOfRange(_))
        ));

        assert!(matches!(
            history.expand("!99"),
            Err(HistoryExpansionError::OutOfRange(_))
        ));

        assert!(matches!(
            history.expand("!notfound"),
            Err(HistoryExpansionError::NotFound(_))
        ));
    }

    #[test]
    fn test_expand_no_expansion_needed() {
        let mut history = History::new();
        history.add("cmd1");

        let (expanded, was_expanded) = history.expand("echo hello").unwrap();
        assert!(!was_expanded);
        assert_eq!(expanded, "echo hello");
    }

    #[test]
    fn test_expand_trailing_bang() {
        let mut history = History::new();
        history.add("cmd1");

        let (expanded, was_expanded) = history.expand("echo wow!").unwrap();
        assert!(!was_expanded);
        assert_eq!(expanded, "echo wow!");
    }

    #[test]
    fn test_expand_bang_space() {
        let mut history = History::new();
        history.add("cmd1");

        let (expanded, was_expanded) = history.expand("echo ! hello").unwrap();
        assert!(!was_expanded);
        assert_eq!(expanded, "echo ! hello");
    }

    #[test]
    fn test_interactive_search() {
        let mut history = History::new();
        history.add("ls -la");
        history.add("cd /tmp");
        history.add("ls /home");
        history.add("echo hello");
        history.add("git status");

        let mut search = history.interactive_search();

        search.update_query("ls");
        assert_eq!(search.results().len(), 2);
        assert_eq!(search.selected().unwrap().command, "ls /home");

        search.select_next();
        assert_eq!(search.selected().unwrap().command, "ls -la");

        search.select_prev();
        assert_eq!(search.selected().unwrap().command, "ls /home");
    }

    #[test]
    fn test_fuzzy_search() {
        let mut history = History::new();
        history.add("git commit -m 'fix bug'");
        history.add("git push origin main");
        history.add("cargo build --release");
        history.add("git log --oneline");

        let results = history.fuzzy_search("git", 10);
        assert_eq!(results.len(), 3);

        let results = history.fuzzy_search("gco", 10);
        assert!(!results.is_empty());
    }

    #[test]
    fn test_fuzzy_score() {
        assert!(History::fuzzy_score("git commit", "git") > 0);
        assert!(History::fuzzy_score("git commit", "git") > History::fuzzy_score("digit", "git"));

        assert!(History::fuzzy_score("git commit", "gc") > 0);

        assert_eq!(History::fuzzy_score("abc", "xyz"), 0);
    }
}
