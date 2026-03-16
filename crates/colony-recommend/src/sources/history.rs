//! History-based recommendation source.

use std::collections::{HashMap, HashSet};

use crate::input::RecommendInput;
use crate::scoring::fuzzy_score;
use crate::source::{RecommendKind, RecommendSource, Recommendation};

/// Recommends commands from shell history, weighted by frequency and recency.
pub struct HistorySource {
    entries: Vec<String>,
    frequency: HashMap<String, u32>,
}

impl HistorySource {
    /// Create a new history source from a list of history entries.
    pub fn new(entries: Vec<String>) -> Self {
        let mut source = Self {
            entries,
            frequency: HashMap::new(),
        };
        source.rebuild_frequency();
        source
    }

    /// Rebuild the frequency map from current entries.
    pub fn rebuild_frequency(&mut self) {
        self.frequency.clear();
        for entry in &self.entries {
            *self.frequency.entry(entry.clone()).or_insert(0) += 1;
        }
    }

    /// Add a new history entry.
    pub fn push(&mut self, entry: String) {
        *self.frequency.entry(entry.clone()).or_insert(0) += 1;
        self.entries.push(entry);
    }
}

impl RecommendSource for HistorySource {
    fn recommend(&self, input: &RecommendInput) -> Vec<Recommendation> {
        if input.line.is_empty() {
            return Vec::new();
        }

        let mut seen = HashSet::new();
        let mut results = Vec::new();

        // Iterate in reverse for recency bias
        for (recency_idx, entry) in self.entries.iter().rev().enumerate() {
            if !seen.insert(entry.as_str()) {
                continue; // Skip duplicates
            }

            let score = fuzzy_score(&input.line, entry);
            if score < 0.1 {
                continue;
            }

            let freq = self.frequency.get(entry).copied().unwrap_or(1) as f64;
            let recency_boost = 0.1 / (recency_idx as f64 + 1.0);
            let freq_boost = (freq.ln() + 1.0) * 0.05;
            let confidence = (score + recency_boost + freq_boost).min(1.0);

            // Ghost text: show the part after current input
            let ghost = if entry.starts_with(&input.line) {
                entry[input.line.len()..].to_string()
            } else {
                entry.clone()
            };

            results.push(Recommendation {
                text: ghost,
                label: entry.clone(),
                description: None,
                kind: RecommendKind::History,
                confidence,
                match_indices: Vec::new(),
            });

            if results.len() >= 20 {
                break;
            }
        }

        results
    }

    fn name(&self) -> &'static str {
        "history"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn matches_prefix() {
        let source = HistorySource::new(vec![
            "git commit -m 'fix'".to_string(),
            "git push origin main".to_string(),
            "cargo test".to_string(),
        ]);
        let input = RecommendInput::parse("git c", 5, PathBuf::from("/tmp"));
        let results = source.recommend(&input);
        assert!(!results.is_empty());
        assert_eq!(results[0].kind, RecommendKind::History);
        assert!(results[0].label.contains("commit"));
    }

    #[test]
    fn frequency_boost() {
        let mut source = HistorySource::new(vec![
            "ls".to_string(),
            "ls -la".to_string(),
            "ls -la".to_string(),
            "ls -la".to_string(),
        ]);
        source.rebuild_frequency();
        let input = RecommendInput::parse("ls", 2, PathBuf::from("/tmp"));
        let results = source.recommend(&input);
        // "ls -la" should score higher due to frequency
        assert!(results.len() >= 2);
        assert!(results[0].confidence >= results[1].confidence);
    }

    #[test]
    fn empty_input_returns_empty() {
        let source = HistorySource::new(vec!["git status".to_string()]);
        let input = RecommendInput::parse("", 0, PathBuf::from("/tmp"));
        let results = source.recommend(&input);
        assert!(results.is_empty());
    }

    #[test]
    fn deduplicates_results() {
        let source = HistorySource::new(vec![
            "git status".to_string(),
            "git status".to_string(),
            "git status".to_string(),
        ]);
        let input = RecommendInput::parse("git", 3, PathBuf::from("/tmp"));
        let results = source.recommend(&input);
        // Should only have one entry despite 3 duplicates
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn ghost_text_shows_suffix() {
        let source = HistorySource::new(vec!["cargo build --release".to_string()]);
        let input = RecommendInput::parse("cargo build", 11, PathBuf::from("/tmp"));
        let results = source.recommend(&input);
        assert!(!results.is_empty());
        assert_eq!(results[0].text, " --release");
    }
}
