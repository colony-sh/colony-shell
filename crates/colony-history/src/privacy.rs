//! Privacy filtering for command history recording.
//!
//! Decides whether a command should be recorded or skipped entirely.
//! Does NOT perform redaction -- that responsibility belongs to
//! `colony-secure`'s `SecureRegistry::redact()` method.

use regex::Regex;

/// Action the recording pipeline should take for a given command.
#[derive(Debug, PartialEq, Eq)]
pub enum PrivacyAction {
    /// Command should not be recorded at all.
    Skip,
    /// Command should be recorded -- caller handles redaction separately.
    Record,
}

/// Filter that decides whether a command should be persisted to history.
pub struct PrivacyFilter {
    exclusion_patterns: Vec<Regex>,
}

impl PrivacyFilter {
    /// Create a new filter with the given regex exclusion patterns.
    pub fn new(exclusion_patterns: Vec<Regex>) -> Self {
        Self { exclusion_patterns }
    }

    /// Check whether a command should be recorded or skipped.
    ///
    /// A command is skipped if:
    /// - It starts with a space (matches bash `HISTCONTROL=ignorespace`)
    /// - It matches any configured regex exclusion pattern
    pub fn should_record(&self, command: &str) -> PrivacyAction {
        // Space-prefix: skip entirely
        if command.starts_with(' ') {
            return PrivacyAction::Skip;
        }

        // Regex exclusions: skip entirely
        for pattern in &self.exclusion_patterns {
            if pattern.is_match(command) {
                return PrivacyAction::Skip;
            }
        }

        PrivacyAction::Record
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_space_prefix_skipped() {
        let filter = PrivacyFilter::new(vec![]);
        assert_eq!(filter.should_record(" secret command"), PrivacyAction::Skip);
    }

    #[test]
    fn test_regex_exclusion_skipped() {
        let filter = PrivacyFilter::new(vec![Regex::new(r"^(export|set)\s+.*PASSWORD").unwrap()]);
        assert_eq!(
            filter.should_record("export DB_PASSWORD=hunter2"),
            PrivacyAction::Skip,
        );
    }

    #[test]
    fn test_normal_command_recorded() {
        let filter = PrivacyFilter::new(vec![]);
        assert_eq!(filter.should_record("ls -la"), PrivacyAction::Record);
    }

    #[test]
    fn test_multiple_exclusion_patterns() {
        let filter = PrivacyFilter::new(vec![
            Regex::new(r"^export\s+.*SECRET").unwrap(),
            Regex::new(r"^vault\s").unwrap(),
        ]);
        assert_eq!(
            filter.should_record("vault read secret/foo"),
            PrivacyAction::Skip,
        );
        assert_eq!(
            filter.should_record("export API_SECRET=xyz"),
            PrivacyAction::Skip,
        );
        assert_eq!(filter.should_record("echo hello"), PrivacyAction::Record,);
    }

    #[test]
    fn test_empty_command_recorded() {
        let filter = PrivacyFilter::new(vec![]);
        assert_eq!(filter.should_record(""), PrivacyAction::Record);
    }

    #[test]
    fn test_only_space_is_skipped() {
        let filter = PrivacyFilter::new(vec![]);
        assert_eq!(filter.should_record(" "), PrivacyAction::Skip);
    }
}
