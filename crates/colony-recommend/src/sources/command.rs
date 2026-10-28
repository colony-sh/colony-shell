//! Command name recommendation source (PATH executables + builtins).

use crate::input::RecommendInput;
use crate::scoring::fuzzy_score;
use crate::source::{RecommendKind, RecommendSource, Recommendation};

/// Recommends command names from PATH and builtins.
pub struct CommandSource {
    commands: Vec<(String, RecommendKind)>,
}

impl CommandSource {
    /// Create a new command source from builtins and PATH commands.
    pub fn new(builtins: Vec<String>, path_commands: Vec<String>) -> Self {
        let mut commands: Vec<(String, RecommendKind)> = builtins
            .into_iter()
            .map(|c| (c, RecommendKind::Builtin))
            .collect();
        commands.extend(
            path_commands
                .into_iter()
                .map(|c| (c, RecommendKind::Command)),
        );
        Self { commands }
    }
}

impl RecommendSource for CommandSource {
    fn recommend(&self, input: &RecommendInput) -> Vec<Recommendation> {
        if !input.is_command_position() || input.word.is_empty() {
            return Vec::new();
        }

        self.commands
            .iter()
            .filter_map(|(cmd, kind)| {
                let score = fuzzy_score(&input.word, cmd);
                if score < 0.1 {
                    return None;
                }
                let text = if cmd.starts_with(&input.word) {
                    cmd[input.word.len()..].to_string()
                } else {
                    cmd.clone()
                };
                Some(Recommendation {
                    text,
                    label: cmd.clone(),
                    description: None,
                    kind: *kind,
                    confidence: score,
                    match_indices: Vec::new(),
                })
            })
            .take(20)
            .collect()
    }

    fn name(&self) -> &'static str {
        "command"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn matches_builtins() {
        let source = CommandSource::new(
            vec!["cd".to_string(), "echo".to_string(), "exit".to_string()],
            vec![],
        );
        let input = RecommendInput::parse("ec", 2, PathBuf::from("/tmp"));
        let results = source.recommend(&input);
        assert!(!results.is_empty());
        assert_eq!(results[0].kind, RecommendKind::Builtin);
        assert_eq!(results[0].label, "echo");
        assert_eq!(results[0].text, "ho");
    }

    #[test]
    fn matches_path_commands() {
        let source = CommandSource::new(
            vec![],
            vec!["git".to_string(), "grep".to_string(), "gcc".to_string()],
        );
        let input = RecommendInput::parse("gr", 2, PathBuf::from("/tmp"));
        let results = source.recommend(&input);
        assert!(!results.is_empty());
        assert_eq!(results[0].kind, RecommendKind::Command);
        assert_eq!(results[0].label, "grep");
    }

    #[test]
    fn ignores_arg_position() {
        let source = CommandSource::new(vec!["echo".to_string()], vec!["git".to_string()]);
        let input = RecommendInput::parse("git ec", 6, PathBuf::from("/tmp"));
        let results = source.recommend(&input);
        assert!(results.is_empty());
    }

    #[test]
    fn ignores_empty_word() {
        let source = CommandSource::new(vec!["echo".to_string()], vec![]);
        let input = RecommendInput::parse("", 0, PathBuf::from("/tmp"));
        let results = source.recommend(&input);
        assert!(results.is_empty());
    }
}
