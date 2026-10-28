//! Complexity scoring for tutorial pacing
//!
//! Analyzes AST nodes to determine how much attention they deserve
//! during tutorial playback.

use crate::tutorial::events::ComplexityLevel;
use colony_parser::{
    AssignmentValue, CaseStatement, ForStatement, IfStatement, Pipeline, SimpleCommand, Statement,
    UntilStatement, WhileStatement, Word, WordPart,
};

/// Scores the complexity of a statement for pacing purposes
#[derive(Debug, Clone, Copy, Default)]
pub struct ComplexityScorer;

impl ComplexityScorer {
    /// Score a statement's complexity
    pub fn score(stmt: &Statement) -> ComplexityLevel {
        match stmt {
            Statement::SimpleCommand(cmd) => Self::score_simple_command(cmd),
            Statement::Pipeline(pipeline) => Self::score_pipeline(pipeline),
            Statement::If(if_stmt) => Self::score_if(if_stmt),
            Statement::For(for_stmt) => Self::score_for(for_stmt),
            Statement::While(while_stmt) => Self::score_while(while_stmt),
            Statement::Until(until_stmt) => Self::score_until(until_stmt),
            Statement::Case(case_stmt) => Self::score_case(case_stmt),
            Statement::Match(_) => ComplexityLevel::High,
            Statement::Try(_) => ComplexityLevel::High,
            Statement::FunctionDef(_) => ComplexityLevel::High,
            Statement::Subshell(_) => ComplexityLevel::Medium,
            Statement::BraceGroup(_) => ComplexityLevel::Medium,
            Statement::List(list) => {
                // List complexity is the max of its parts
                let first_score = Self::score(&list.first);
                let rest_max = list
                    .rest
                    .iter()
                    .map(|(_, stmt)| Self::score(stmt))
                    .max()
                    .unwrap_or(ComplexityLevel::Low);
                first_score.max(rest_max)
            }
            Statement::Empty => ComplexityLevel::Low,
            Statement::Return(_) => ComplexityLevel::Low,
            Statement::Coproc(_) => ComplexityLevel::High,
            _ => ComplexityLevel::Low,
        }
    }

    fn score_simple_command(cmd: &SimpleCommand) -> ComplexityLevel {
        let mut score = ComplexityLevel::Low;

        // Check command name for simple builtins
        if let Some(name) = cmd.name.as_literal() {
            match name {
                // Very simple commands
                "true" | "false" | ":" | "echo" | "printf" | "cd" | "pwd" => {
                    // Keep low unless there are complex args
                }
                // Assignment builtins
                "export" | "local" | "declare" | "readonly" | "typeset" => {
                    score = score.max(ComplexityLevel::Medium);
                }
                // More complex builtins
                "eval" | "source" | "." | "exec" => {
                    score = ComplexityLevel::High;
                }
                _ => {}
            }
        }

        // Check for complex expansions in arguments
        for word in std::iter::once(&cmd.name).chain(cmd.args.iter()) {
            score = score.max(Self::score_word(word));
        }

        // Redirections add complexity
        if !cmd.redirects.is_empty() {
            score = score.max(ComplexityLevel::Medium);
        }

        // Environment assignments, plus any expansions in their values
        for assignment in &cmd.env_assignments {
            score = score.max(ComplexityLevel::Medium);
            let words = match &assignment.value {
                AssignmentValue::Word(word) => std::slice::from_ref(word),
                AssignmentValue::Array(words) => words.as_slice(),
                AssignmentValue::AssocArray(_) => &[],
            };
            for word in words {
                score = score.max(Self::score_word(word));
            }
        }

        score
    }

    fn score_pipeline(pipeline: &Pipeline) -> ComplexityLevel {
        if pipeline.elements.len() == 1 {
            // Single command pipeline
            Self::score(&pipeline.elements[0].command)
        } else {
            // Multi-command pipeline is always at least Medium
            let element_max = pipeline
                .elements
                .iter()
                .map(|e| Self::score(&e.command))
                .max()
                .unwrap_or(ComplexityLevel::Low);

            // Boost by one level for pipelines
            match element_max {
                ComplexityLevel::Low => ComplexityLevel::Medium,
                ComplexityLevel::Medium | ComplexityLevel::High => ComplexityLevel::High,
            }
        }
    }

    fn score_if(if_stmt: &IfStatement) -> ComplexityLevel {
        // If statements are inherently complex
        let mut score = ComplexityLevel::High;

        // Even more complex with elif/else branches
        if !if_stmt.elif_branches.is_empty() || if_stmt.else_body.is_some() {
            score = ComplexityLevel::High;
        }

        score
    }

    fn score_for(_for_stmt: &ForStatement) -> ComplexityLevel {
        // Loops are always high complexity (first entry)
        ComplexityLevel::High
    }

    fn score_while(_while_stmt: &WhileStatement) -> ComplexityLevel {
        ComplexityLevel::High
    }

    fn score_until(_until_stmt: &UntilStatement) -> ComplexityLevel {
        ComplexityLevel::High
    }

    fn score_case(_case_stmt: &CaseStatement) -> ComplexityLevel {
        ComplexityLevel::High
    }

    fn score_word(word: &Word) -> ComplexityLevel {
        let mut score = ComplexityLevel::Low;

        for part in &word.parts {
            score = score.max(Self::score_word_part(part));
        }

        score
    }

    fn score_word_part(part: &WordPart) -> ComplexityLevel {
        match part {
            WordPart::Literal(_) => ComplexityLevel::Low,
            WordPart::SingleQuoted(_) => ComplexityLevel::Low,

            WordPart::Variable(var_ref) => {
                if var_ref.modifier.is_some() {
                    ComplexityLevel::Medium
                } else {
                    ComplexityLevel::Low
                }
            }

            WordPart::CommandSubRaw(_) | WordPart::CommandSub(_) => ComplexityLevel::High,

            WordPart::ArithRaw(_) => ComplexityLevel::Medium,
            WordPart::ExprSub(_) => ComplexityLevel::Medium,

            WordPart::ProcessSub { .. } => ComplexityLevel::High,

            WordPart::DoubleQuoted(parts) => parts
                .iter()
                .map(Self::score_word_part)
                .max()
                .unwrap_or(ComplexityLevel::Low),

            WordPart::ExtGlob { .. } => ComplexityLevel::Medium,

            WordPart::AnsiCQuoted(_) => ComplexityLevel::Medium,

            WordPart::BraceExpansion(_) => ComplexityLevel::Medium,

            WordPart::Tilde(_) => ComplexityLevel::Low,
        }
    }

    /// Get recommended pause duration in milliseconds based on complexity
    #[allow(dead_code)]
    pub fn pause_duration_ms(level: ComplexityLevel) -> u64 {
        match level {
            ComplexityLevel::Low => 500,
            ComplexityLevel::Medium => 1500,
            ComplexityLevel::High => 3000,
        }
    }

    /// Check if this complexity level should auto-pause
    pub fn should_auto_pause(level: ComplexityLevel) -> bool {
        matches!(level, ComplexityLevel::High)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use colony_parser::pest_api::parse;

    fn parse_stmt(input: &str) -> Statement {
        let program = parse(input).expect("parse failed");
        program.statements.into_iter().next().unwrap()
    }

    #[test]
    fn test_simple_echo() {
        let stmt = parse_stmt("echo hello");
        assert_eq!(ComplexityScorer::score(&stmt), ComplexityLevel::Low);
    }

    #[test]
    fn test_variable_expansion() {
        let stmt = parse_stmt("echo \"$name\"");
        assert_eq!(ComplexityScorer::score(&stmt), ComplexityLevel::Low);
    }

    #[test]
    fn test_parameter_expansion_with_default() {
        let stmt = parse_stmt("echo \"${name:-default}\"");
        assert_eq!(ComplexityScorer::score(&stmt), ComplexityLevel::Medium);
    }

    #[test]
    fn test_command_substitution() {
        let stmt = parse_stmt("files=$(ls)");
        assert_eq!(ComplexityScorer::score(&stmt), ComplexityLevel::High);
    }

    #[test]
    fn test_pipeline() {
        // Simple stages: pipeline boosts Low to Medium
        let stmt = parse_stmt("ls | grep foo | wc -l");
        assert_eq!(ComplexityScorer::score(&stmt), ComplexityLevel::Medium);
    }

    #[test]
    fn test_pipeline_with_complex_stage() {
        // A Medium stage is boosted to High
        let stmt = parse_stmt("ls | grep \"${pat:-foo}\"");
        assert_eq!(ComplexityScorer::score(&stmt), ComplexityLevel::High);
    }

    #[test]
    fn test_for_loop() {
        let stmt = parse_stmt("for i in 1 2 3; do echo $i; done");
        assert_eq!(ComplexityScorer::score(&stmt), ComplexityLevel::High);
    }

    #[test]
    fn test_redirect() {
        let stmt = parse_stmt("echo hello > file.txt");
        assert_eq!(ComplexityScorer::score(&stmt), ComplexityLevel::Medium);
    }

    #[test]
    fn test_pause_duration() {
        assert_eq!(
            ComplexityScorer::pause_duration_ms(ComplexityLevel::Low),
            500
        );
        assert_eq!(
            ComplexityScorer::pause_duration_ms(ComplexityLevel::Medium),
            1500
        );
        assert_eq!(
            ComplexityScorer::pause_duration_ms(ComplexityLevel::High),
            3000
        );
    }
}
