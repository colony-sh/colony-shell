//! Tree-sitter grammar for Colony Shell
//!
//! This crate provides Tree-sitter bindings for parsing Colony Shell syntax.
//! It's primarily used for editor features like syntax highlighting, folding,
//! and navigation - not for execution (use colony-parser for that).

use tree_sitter::Language;

extern "C" {
    fn tree_sitter_colony() -> Language;
}

/// Get the Tree-sitter language for Colony Shell
pub fn language() -> Language {
    unsafe { tree_sitter_colony() }
}

/// Content of the node-types.json file
pub const NODE_TYPES: &str = include_str!("../../src/node-types.json");

/// Content of the highlights.scm file
pub const HIGHLIGHTS_QUERY: &str = include_str!("../../queries/highlights.scm");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_can_load_grammar() {
        let language = language();
        assert!(language.version() > 0);
    }

    #[test]
    fn test_parse_simple_command() {
        use tree_sitter::Parser;

        let mut parser = Parser::new();
        parser.set_language(language()).unwrap();

        let source = "echo hello world";
        let tree = parser.parse(source, None).unwrap();
        let root = tree.root_node();

        assert_eq!(root.kind(), "program");
        assert!(root.child_count() > 0);
    }

    #[test]
    fn test_parse_pipeline() {
        use tree_sitter::Parser;

        let mut parser = Parser::new();
        parser.set_language(language()).unwrap();

        let source = "echo foo | grep f";
        let tree = parser.parse(source, None).unwrap();
        let root = tree.root_node();

        assert_eq!(root.kind(), "program");
    }

    #[test]
    fn test_incremental_parse() {
        use tree_sitter::Parser;

        let mut parser = Parser::new();
        parser.set_language(language()).unwrap();

        // Initial parse
        let source1 = "echo hello";
        let tree1 = parser.parse(source1, None).unwrap();

        // Edit: change "hello" to "world"
        let source2 = "echo world";
        let tree2 = parser.parse(source2, Some(&tree1)).unwrap();

        // Both should successfully parse
        assert_eq!(tree1.root_node().kind(), "program");
        assert_eq!(tree2.root_node().kind(), "program");
    }
}
