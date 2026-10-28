//! Complexity scoring for X-Ray auto-pacing.
//!
//! Scores statements by structural complexity to determine how long
//! the X-Ray should pause on each line. Simple commands get a brief
//! pause; compound commands and pipelines get longer pauses so the
//! user has time to read the annotations.

use crate::events::Complexity;

/// Score a statement's complexity from its source text and AST kind.
///
/// Returns a complexity level that determines X-Ray pacing.
pub fn score(node_kind: &str, source: &str) -> Complexity {
    // Compound commands are always high
    if matches!(
        node_kind,
        "If" | "While" | "Until" | "For" | "Case" | "Try" | "Match"
    ) {
        return Complexity::High;
    }

    // Pipelines with multiple stages
    if node_kind == "Pipeline" && source.matches('|').count() >= 2 {
        return Complexity::High;
    }
    if node_kind == "Pipeline" {
        return Complexity::Medium;
    }

    // Process substitution is complex
    if source.contains("<(") || source.contains(">(") {
        return Complexity::High;
    }

    // Command substitution
    if source.contains("$(") || source.contains('`') {
        return Complexity::Medium;
    }

    // Parameter expansion with modifiers
    if source.contains("${") {
        return Complexity::Medium;
    }

    // Redirections
    if source.contains(">&") || source.contains("2>&1") {
        return Complexity::Medium;
    }

    // Arithmetic expansion
    if source.contains("$((") {
        return Complexity::Medium;
    }

    Complexity::Low
}

/// Pacing delay in milliseconds based on complexity.
pub fn pacing_ms(complexity: Complexity) -> u64 {
    match complexity {
        Complexity::Low => 400,
        Complexity::Medium => 1200,
        Complexity::High => 2500,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_echo_is_low() {
        assert_eq!(score("SimpleCommand", "echo hello"), Complexity::Low);
    }

    #[test]
    fn pipeline_is_medium() {
        assert_eq!(score("Pipeline", "ls | grep foo"), Complexity::Medium);
    }

    #[test]
    fn multi_pipeline_is_high() {
        assert_eq!(
            score("Pipeline", "cat file | grep pat | sort | uniq"),
            Complexity::High
        );
    }

    #[test]
    fn if_statement_is_high() {
        assert_eq!(score("If", "if [ -f foo ]; then"), Complexity::High);
    }

    #[test]
    fn while_statement_is_high() {
        assert_eq!(score("While", "while true; do"), Complexity::High);
    }

    #[test]
    fn for_statement_is_high() {
        assert_eq!(score("For", "for i in 1 2 3; do"), Complexity::High);
    }

    #[test]
    fn case_statement_is_high() {
        assert_eq!(score("Case", "case $x in"), Complexity::High);
    }

    #[test]
    fn command_sub_is_medium() {
        assert_eq!(score("SimpleCommand", "echo $(date)"), Complexity::Medium);
    }

    #[test]
    fn backtick_sub_is_medium() {
        assert_eq!(score("SimpleCommand", "echo `date`"), Complexity::Medium);
    }

    #[test]
    fn param_expansion_is_medium() {
        assert_eq!(
            score("SimpleCommand", "echo ${var:-default}"),
            Complexity::Medium
        );
    }

    #[test]
    fn process_sub_is_high() {
        assert_eq!(
            score("SimpleCommand", "diff <(sort a) <(sort b)"),
            Complexity::High
        );
    }

    #[test]
    fn arithmetic_is_medium() {
        assert_eq!(
            score("SimpleCommand", "echo $((x + 1))"),
            Complexity::Medium
        );
    }

    #[test]
    fn redirect_is_medium() {
        assert_eq!(score("SimpleCommand", "cmd 2>&1"), Complexity::Medium);
    }

    #[test]
    fn simple_assignment_is_low() {
        assert_eq!(score("SimpleCommand", "x=hello"), Complexity::Low);
    }

    #[test]
    fn pacing_increases_with_complexity() {
        assert!(pacing_ms(Complexity::Low) < pacing_ms(Complexity::Medium));
        assert!(pacing_ms(Complexity::Medium) < pacing_ms(Complexity::High));
    }
}
