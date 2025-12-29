//! pest-based parser for Colony Shell
//!
use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "colony.pest"]
pub struct ColonyParser;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_command() {
        let source = "echo hello world";
        let result = ColonyParser::parse(Rule::SimpleCommand, source);
        assert!(result.is_ok(), "Failed to parse simple command: {:?}", result.err());
    }

    #[test]
    fn test_parse_pipeline() {
        let source = "cat file.txt | grep pattern | wc -l";
        let result = ColonyParser::parse(Rule::Pipeline, source);
        assert!(result.is_ok(), "Failed to parse pipeline: {:?}", result.err());
    }

    #[test]
    fn test_parse_if_statement() {
        let source = "if [ 1 -eq 1 ]; then echo yes; fi";
        let result = ColonyParser::parse(Rule::IfStatement, source);
        assert!(result.is_ok(), "Failed to parse POSIX if statement: {:?}", result.err());
    }

    #[test]
    fn test_parse_for_loop() {
        let source = "for ((;;)); do echo hello; done";
        let result = ColonyParser::parse(Rule::ForStatement, source);
        assert!(result.is_ok(), "Failed to parse C-style for loop: {:?}", result.err());
    }

    #[test]
    fn test_parse_posix_for_loop() {
        let source = "for x in a b c; do echo $x; done";
        let result = ColonyParser::parse(Rule::ForStatement, source);
        assert!(result.is_ok(), "Failed to parse POSIX for loop: {:?}", result.err());
    }

    #[test]
    fn test_parse_posix_for_as_statement() {
        let source = "for x in a b c; do echo $x; done";
        let result = ColonyParser::parse(Rule::Statement, source);
        assert!(result.is_ok(), "Failed to parse POSIX for as Statement: {:?}", result.err());
    }

    #[test]
    fn test_parse_variable_decl() {
        let source = "var x = 42";
        let result = ColonyParser::parse(Rule::VarDecl, source);
        assert!(result.is_ok(), "Failed to parse var decl: {:?}", result.err());
    }

    #[test]
    fn test_parse_expression() {
        let source = "x + y * 2";
        let result = ColonyParser::parse(Rule::Expression, source);
        assert!(result.is_ok(), "Failed to parse expression: {:?}", result.err());
    }

    #[test]
    fn test_parse_program() {
        // Start with simple program - one statement at a time
        let source = "echo hello";
        let result = ColonyParser::parse(Rule::Program, source);
        assert!(result.is_ok(), "Failed to parse program: {:?}", result.err());
    }

    // === Pure PEG Tests - No Preprocessing Required ===

    #[test]
    fn test_context_sensitive_barewords() {
        // Bareword "hello" is Word in command context
        let source = "echo hello";
        let result = ColonyParser::parse(Rule::SimpleCommand, source);
        assert!(result.is_ok(), "Bareword should work in command: {:?}", result.err());

        // Bareword "hello" is Identifier in expression context
        let source = "var x = hello";
        let result = ColonyParser::parse(Rule::VarDecl, source);
        assert!(result.is_ok(), "Identifier should work in expression: {:?}", result.err());
    }

    #[test]
    fn test_context_sensitive_operators() {
        // '<' as redirect in command context
        let source = "cat < file.txt";
        let result = ColonyParser::parse(Rule::SimpleCommand, source);
        assert!(result.is_ok(), "< should be redirect in command: {:?}", result.err());

        // '<' as comparison in expression context
        let source = "x < 10";
        let result = ColonyParser::parse(Rule::Expression, source);
        assert!(result.is_ok(), "< should be comparison in expression: {:?}", result.err());
    }

    #[test]
    fn test_variable_expansion_in_command() {
        // pest parses $var directly from source
        let source = "echo $count";
        let result = ColonyParser::parse(Rule::SimpleCommand, source);
        assert!(result.is_ok(), "Should parse $var expansion: {:?}", result.err());
    }

    #[test]
    fn test_quoted_strings() {
        // Single-quoted (no interpolation)
        let source = "echo 'hello world'";
        let result = ColonyParser::parse(Rule::SimpleCommand, source);
        assert!(result.is_ok(), "Should parse single-quoted: {:?}", result.err());

        // Double-quoted (with interpolation)
        let source = r#"echo "Count: $count""#;
        let result = ColonyParser::parse(Rule::SimpleCommand, source);
        assert!(result.is_ok(), "Should parse double-quoted: {:?}", result.err());
    }

    #[test]
    fn test_command_substitution() {
        // pest parses $(cmd) recursively from source
        let source = "echo $(ls)";
        let result = ColonyParser::parse(Rule::SimpleCommand, source);
        assert!(result.is_ok(), "Should parse command substitution: {:?}", result.err());
    }

    #[test]
    fn test_nested_quotes() {
        // Nested quotes in command substitution
        let source = r#"echo "Files: $(ls "*.txt")""#;
        let result = ColonyParser::parse(Rule::SimpleCommand, source);
        assert!(result.is_ok(), "Should handle nested quotes: {:?}", result.err());
    }

    #[test]
    #[ignore = "Colony brace-style if requires future grammar work - POSIX if uses then/fi"]
    fn test_expression_in_if() {
        let source = "if (x > 10 && y < 20) { echo ok }";
        let result = ColonyParser::parse(Rule::IfStatement, source);
        assert!(result.is_ok(), "Should parse expression condition: {:?}", result.err());
    }

    #[test]
    fn test_heredoc_with_preprocessing() {
        use crate::pest_api::parse;

        let source = r#"cat <<EOF
This is content
More content
EOF"#;

        let result = parse(source);
        assert!(result.is_ok(), "Heredoc should parse with preprocessing: {:?}", result.err());
    }

    #[test]
    fn test_extglob_patterns() {
        let source = "@(foo|bar)";
        let result = ColonyParser::parse(Rule::ExtGlob, source);
        assert!(result.is_ok(), "Should parse @() extglob: {:?}", result.err());

        let source = "!(foo)";
        let result = ColonyParser::parse(Rule::ExtGlob, source);
        assert!(result.is_ok(), "Should parse !() extglob: {:?}", result.err());
        
        let source = "?(foo|bar)";
        let result = ColonyParser::parse(Rule::ExtGlob, source);
        assert!(result.is_ok(), "Should parse ?() extglob: {:?}", result.err());
        
        let source = "*(foo)";
        let result = ColonyParser::parse(Rule::ExtGlob, source);
        assert!(result.is_ok(), "Should parse *() extglob: {:?}", result.err());
        
        let source = "+(foo)";
        let result = ColonyParser::parse(Rule::ExtGlob, source);
        assert!(result.is_ok(), "Should parse +() extglob: {:?}", result.err());
    }
    
    #[test]
    fn test_extglob_in_word() {
        let source = "!(foo).txt";
        let result = ColonyParser::parse(Rule::Word, source);
        assert!(result.is_ok(), "Should parse !().txt as Word: {:?}", result.err());
        
        if let Ok(mut pairs) = result {
            let word_pair = pairs.next().unwrap();
            println!("Word contents: {:?}", word_pair);
            for inner in word_pair.into_inner() {
                println!("  Inner rule: {:?}, text: {:?}", inner.as_rule(), inner.as_str());
            }
        }
    }
    
    #[test]
    fn test_extglob_in_simple_command() {
        let source = "echo !(foo).txt";
        let result = ColonyParser::parse(Rule::SimpleCommand, source);
        assert!(result.is_ok(), "Should parse echo with extglob: {:?}", result.err());
        
        if let Ok(mut pairs) = result {
            let cmd_pair = pairs.next().unwrap();
            println!("SimpleCommand contents:");
            for inner in cmd_pair.into_inner() {
                println!("  Rule: {:?}, text: {:?}", inner.as_rule(), inner.as_str());
            }
        }
    }
}
