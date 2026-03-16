//! Human-readable explanations for shell constructs.
//!
//! Used in X-Ray teach mode for first-occurrence concept boxes.
//! The `ConceptExplainer` tracks which concepts have already been
//! shown, so each concept is explained at most once per session.

use std::collections::HashSet;

/// Generates first-occurrence explanations for shell concepts.
pub struct ConceptExplainer {
    seen: HashSet<String>,
    teach_mode: bool,
}

impl ConceptExplainer {
    /// Create a new explainer. When `teach_mode` is true, first-occurrence
    /// concept boxes are generated.
    pub fn new(teach_mode: bool) -> Self {
        Self {
            seen: HashSet::new(),
            teach_mode,
        }
    }

    /// Try to explain an expansion phase. Returns `None` if already explained
    /// or teach mode is off.
    pub fn explain_expansion(&mut self, phase: &str, input: &str) -> Option<(String, String)> {
        if !self.teach_mode {
            return None;
        }

        let concept = self.classify_expansion(phase, input)?;
        if !self.seen.insert(concept.clone()) {
            return None;
        }

        let explanation = self.expansion_explanation(&concept, input);
        Some((concept, explanation))
    }

    /// Brief inline explanation (always shown, not just teach mode).
    pub fn brief_expansion(phase: &str, input: &str, output: &str) -> String {
        match phase {
            "parameter" => {
                if input.contains(":-") {
                    format!("{input} -- use default if unset -> \"{output}\"")
                } else if input.contains(":=") {
                    format!("{input} -- assign default if unset -> \"{output}\"")
                } else if input.contains(":+") {
                    format!("{input} -- alternate if set -> \"{output}\"")
                } else if input.contains("##") || input.contains("%%") {
                    format!("{input} -- pattern strip -> \"{output}\"")
                } else {
                    format!("{input} -> \"{output}\"")
                }
            }
            "command_sub" => format!("$({input}) captured -> \"{output}\""),
            "arithmetic" => format!("$(({input})) = {output}"),
            "tilde" => format!("{input} -> {output}"),
            "pathname" => format!("glob {input} matched -> {output}"),
            _ => format!("{input} -> {output}"),
        }
    }

    /// Explain a control flow decision.
    pub fn brief_control_flow(kind: &str, decision: &str) -> String {
        match kind {
            "if" => format!("condition is {decision}"),
            "while" | "until" => format!("loop condition: {decision}"),
            "for" => format!("iterating: {decision}"),
            "case" => format!("matching pattern: {decision}"),
            _ => decision.to_string(),
        }
    }

    fn classify_expansion(&self, phase: &str, input: &str) -> Option<String> {
        match phase {
            "parameter" => {
                if input.contains(":-") {
                    Some("parameter_default".to_string())
                } else if input.contains(":=") {
                    Some("parameter_assign".to_string())
                } else if input.contains(":+") {
                    Some("parameter_alternate".to_string())
                } else if input.contains(":?") {
                    Some("parameter_error".to_string())
                } else if input.contains("##") || input.contains('#') {
                    Some("parameter_prefix_strip".to_string())
                } else if input.contains("%%") || input.contains('%') {
                    Some("parameter_suffix_strip".to_string())
                } else if input.contains("//") {
                    Some("parameter_replace".to_string())
                } else {
                    Some("parameter_simple".to_string())
                }
            }
            "command_sub" => Some("command_substitution".to_string()),
            "arithmetic" => Some("arithmetic_expansion".to_string()),
            "tilde" => Some("tilde_expansion".to_string()),
            "pathname" => Some("pathname_expansion".to_string()),
            "brace" => Some("brace_expansion".to_string()),
            _ => None,
        }
    }

    fn expansion_explanation(&self, concept: &str, input: &str) -> String {
        match concept {
            "parameter_default" => format!(
                "${{var:-word}} is parameter expansion with default.\n\
                 If the variable is unset or null, the word after :- is used.\n\
                 Here: {input}"
            ),
            "parameter_assign" => format!(
                "${{var:=word}} assigns a default if the variable is unset.\n\
                 Unlike :-, this also sets the variable for later use.\n\
                 Here: {input}"
            ),
            "parameter_alternate" => format!(
                "${{var:+word}} uses the word only if the variable IS set.\n\
                 Useful for conditional flags: ${{verbose:+--verbose}}\n\
                 Here: {input}"
            ),
            "parameter_error" => format!(
                "${{var:?word}} prints an error and exits if var is unset.\n\
                 Useful for required parameters in scripts.\n\
                 Here: {input}"
            ),
            "parameter_prefix_strip" => format!(
                "${{var#pattern}} removes the shortest prefix match.\n\
                 ${{var##pattern}} removes the longest prefix match.\n\
                 Here: {input}"
            ),
            "parameter_suffix_strip" => format!(
                "${{var%pattern}} removes the shortest suffix match.\n\
                 ${{var%%pattern}} removes the longest suffix match.\n\
                 Here: {input}"
            ),
            "parameter_replace" => format!(
                "${{var/pattern/replacement}} replaces the first match.\n\
                 ${{var//pattern/replacement}} replaces all matches.\n\
                 Here: {input}"
            ),
            "parameter_simple" => format!(
                "$var or ${{var}} expands to the variable's value.\n\
                 Braces are needed when followed by characters that could be part of the name.\n\
                 Here: {input}"
            ),
            "command_substitution" => format!(
                "$(...) runs a command and captures its stdout.\n\
                 The output replaces the $() in the parent command.\n\
                 Here: $({input})"
            ),
            "arithmetic_expansion" => format!(
                "$((...)) evaluates arithmetic expressions.\n\
                 Supports +, -, *, /, %, ** and C-style operators.\n\
                 Here: $(({input}))"
            ),
            "tilde_expansion" => {
                "~ expands to $HOME. ~user expands to that user's home directory.".to_string()
            }
            "pathname_expansion" => format!(
                "Glob patterns match filenames:\n\
                 * = any characters, ? = one character, [...] = character class\n\
                 Here: {input}"
            ),
            "brace_expansion" => format!(
                "{{a,b,c}} generates multiple words: a b c.\n\
                 {{1..5}} generates a numeric sequence.\n\
                 Here: {input}"
            ),
            _ => format!("Shell expansion: {input}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn teach_mode_explains_first_occurrence() {
        let mut exp = ConceptExplainer::new(true);
        let result = exp.explain_expansion("parameter", "${x:-default}");
        assert!(result.is_some());
        let (concept, _) = result.expect("explanation exists");
        assert_eq!(concept, "parameter_default");
    }

    #[test]
    fn second_occurrence_returns_none() {
        let mut exp = ConceptExplainer::new(true);
        exp.explain_expansion("parameter", "${x:-default}");
        let result = exp.explain_expansion("parameter", "${y:-other}");
        assert!(result.is_none());
    }

    #[test]
    fn teach_mode_off_returns_none() {
        let mut exp = ConceptExplainer::new(false);
        let result = exp.explain_expansion("parameter", "${x:-default}");
        assert!(result.is_none());
    }

    #[test]
    fn brief_expansion_default() {
        let result = ConceptExplainer::brief_expansion("parameter", "${x:-default}", "default");
        assert!(result.contains("use default if unset"));
    }

    #[test]
    fn brief_expansion_assign() {
        let result = ConceptExplainer::brief_expansion("parameter", "${x:=val}", "val");
        assert!(result.contains("assign default if unset"));
    }

    #[test]
    fn brief_expansion_alternate() {
        let result = ConceptExplainer::brief_expansion("parameter", "${x:+yes}", "yes");
        assert!(result.contains("alternate if set"));
    }

    #[test]
    fn brief_expansion_strip() {
        let result = ConceptExplainer::brief_expansion("parameter", "${x##*/}", "file");
        assert!(result.contains("pattern strip"));
    }

    #[test]
    fn brief_expansion_command_sub() {
        let result = ConceptExplainer::brief_expansion("command_sub", "date", "Mon Mar 16");
        assert!(result.contains("captured"));
    }

    #[test]
    fn brief_expansion_arithmetic() {
        let result = ConceptExplainer::brief_expansion("arithmetic", "1+2", "3");
        assert!(result.contains("= 3"));
    }

    #[test]
    fn brief_expansion_tilde() {
        let result = ConceptExplainer::brief_expansion("tilde", "~", "/home/user");
        assert!(result.contains("/home/user"));
    }

    #[test]
    fn brief_control_flow_if() {
        let result = ConceptExplainer::brief_control_flow("if", "true");
        assert!(result.contains("condition is true"));
    }

    #[test]
    fn brief_control_flow_for() {
        let result = ConceptExplainer::brief_control_flow("for", "i=1");
        assert!(result.contains("iterating: i=1"));
    }

    #[test]
    fn different_concepts_both_explained() {
        let mut exp = ConceptExplainer::new(true);
        let r1 = exp.explain_expansion("parameter", "${x:-default}");
        assert!(r1.is_some());
        let r2 = exp.explain_expansion("command_sub", "date");
        assert!(r2.is_some());
    }

    #[test]
    fn classify_parameter_replace() {
        let mut exp = ConceptExplainer::new(true);
        let result = exp.explain_expansion("parameter", "${x//old/new}");
        assert!(result.is_some());
        let (concept, _) = result.expect("explanation exists");
        assert_eq!(concept, "parameter_replace");
    }

    #[test]
    fn classify_unknown_phase_returns_none() {
        let mut exp = ConceptExplainer::new(true);
        let result = exp.explain_expansion("unknown_phase", "something");
        assert!(result.is_none());
    }

    #[test]
    fn explain_brace_expansion() {
        let mut exp = ConceptExplainer::new(true);
        let result = exp.explain_expansion("brace", "{a,b,c}");
        assert!(result.is_some());
        let (concept, explanation) = result.expect("explanation exists");
        assert_eq!(concept, "brace_expansion");
        assert!(explanation.contains("generates multiple words"));
    }
}
