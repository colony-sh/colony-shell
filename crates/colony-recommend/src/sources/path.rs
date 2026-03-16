//! File and directory path recommendation source.

use std::fs;

use crate::input::RecommendInput;
use crate::source::{RecommendKind, RecommendSource, Recommendation};

/// Recommends file and directory paths.
pub struct PathSource;

impl RecommendSource for PathSource {
    fn recommend(&self, input: &RecommendInput) -> Vec<Recommendation> {
        // Only complete when in argument position or word looks like a path
        let word = &input.word;
        if word.is_empty() && input.arg_position == 0 {
            return Vec::new();
        }

        let (dir, prefix) = if word.contains('/') {
            let last_slash = word.rfind('/').expect("contains('/') was true");
            let dir_part = &word[..=last_slash];
            let file_part = &word[last_slash + 1..];

            let resolved = if dir_part.starts_with('~') {
                dirs::home_dir()
                    .map(|h| h.join(&dir_part[2..]))
                    .unwrap_or_else(|| input.cwd.join(dir_part))
            } else if dir_part.starts_with('/') {
                std::path::PathBuf::from(dir_part)
            } else {
                input.cwd.join(dir_part)
            };

            (resolved, file_part.to_string())
        } else {
            (input.cwd.clone(), word.clone())
        };

        let Ok(entries) = fs::read_dir(&dir) else {
            return Vec::new();
        };

        let mut results = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();

            // Skip hidden files unless prefix starts with '.'
            if name.starts_with('.') && !prefix.starts_with('.') {
                continue;
            }

            if !prefix.is_empty() && !name.starts_with(&prefix) {
                continue;
            }

            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            let suffix = if is_dir { "/" } else { "" };
            let completion = format!("{}{}", &name[prefix.len()..], suffix);

            let confidence = if name.starts_with(&prefix) {
                0.7 + (0.2 * prefix.len() as f64 / name.len().max(1) as f64)
            } else {
                0.3
            };

            // Sort directories first
            let dir_boost = if is_dir { 0.05 } else { 0.0 };

            results.push(Recommendation {
                text: completion,
                label: format!("{name}{suffix}"),
                description: None,
                kind: if is_dir {
                    RecommendKind::Directory
                } else {
                    RecommendKind::File
                },
                confidence: confidence + dir_boost,
                match_indices: Vec::new(),
            });
        }

        results.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(20);
        results
    }

    fn name(&self) -> &'static str {
        "path"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn completes_from_cwd() {
        // Use /tmp which should always exist and have entries
        let input = RecommendInput::parse("ls ", 3, PathBuf::from("/tmp"));
        let source = PathSource;
        let results = source.recommend(&input);
        // /tmp should have at least something
        // Don't assert non-empty since /tmp could be empty in CI,
        // but verify no panic
        for r in &results {
            assert!(
                matches!(r.kind, RecommendKind::File | RecommendKind::Directory),
                "Expected File or Directory kind"
            );
        }
    }

    #[test]
    fn skips_command_position_with_empty_word() {
        let input = RecommendInput::parse("", 0, PathBuf::from("/tmp"));
        let source = PathSource;
        let results = source.recommend(&input);
        assert!(results.is_empty());
    }

    #[test]
    fn handles_nonexistent_directory() {
        let input = RecommendInput::parse(
            "ls /nonexistent_dir_xyz/foo",
            27,
            PathBuf::from("/nonexistent_dir_xyz"),
        );
        let source = PathSource;
        let results = source.recommend(&input);
        assert!(results.is_empty());
    }
}
