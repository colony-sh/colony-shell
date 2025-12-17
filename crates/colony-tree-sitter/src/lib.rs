//! Tree-sitter parser for Colony Shell
//!
//! This crate provides a tree-sitter parser for Colony Shell,
//! forked from tree-sitter-bash with Colony-specific extensions:
//! - POSIX shell syntax (inherited from bash grammar)
//! - Colony-specific extensions (typed pipes, var/const, match, etc.)
//! - Incremental parsing for editor features
//!
//! ## Usage
//!
//! ```rust,no_run
//! use colony_tree_sitter::parse;
//!
//! let source = "echo hello world";
//! let tree = parse(source).unwrap();
//! let root = tree.root_node();
//!
//! println!("Parsed: {:?}", root);
//! ```

use tree_sitter::{Language, Parser, Tree};

// Re-export tree-sitter types for users
pub use tree_sitter::{Node, Tree as TreeSitterTree};

extern "C" {
    fn tree_sitter_colony() -> Language;
}

/// Get the Colony Shell tree-sitter language
pub fn language() -> Language {
    unsafe { tree_sitter_colony() }
}

/// Parse Colony Shell source code
///
/// Returns None if parsing fails.
///
/// # Examples
///
/// ```rust,no_run
/// use colony_tree_sitter::parse;
///
/// let tree = parse("echo hello").unwrap();
/// assert!(!tree.root_node().has_error());
/// ```
pub fn parse(source: &str) -> Option<Tree> {
    let mut parser = Parser::new();
    parser.set_language(language()).ok()?;
    parser.parse(source, None)
}

/// Parse with an old tree for incremental parsing
///
/// This is much faster when only small edits have been made,
/// as tree-sitter can reuse most of the old tree.
///
/// # Examples
///
/// ```rust,no_run
/// use colony_tree_sitter::{parse, parse_incremental};
///
/// let tree1 = parse("echo hello").unwrap();
/// let tree2 = parse_incremental("echo world", &tree1).unwrap();
/// ```
pub fn parse_incremental(source: &str, old_tree: &Tree) -> Option<Tree> {
    let mut parser = Parser::new();
    parser.set_language(language()).ok()?;
    parser.parse(source, Some(old_tree))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_can_create_parser() {
        let mut parser = Parser::new();
        parser.set_language(language()).expect("Failed to load language");
    }

    #[test]
    fn test_parse_simple_command() {
        let tree = parse("echo hello world").unwrap();
        let root = tree.root_node();
        assert!(!root.has_error(), "Parse should succeed");
        assert_eq!(root.kind(), "program");
    }

    #[test]
    fn test_parse_pipeline() {
        let tree = parse("cat file.txt | grep pattern").unwrap();
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn test_parse_colony_typed_pipe() {
        // Colony extension: typed pipes
        let tree = parse("cmd1 -> cmd2").unwrap();
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn test_incremental_parsing() {
        let tree1 = parse("echo hello").unwrap();
        let tree2 = parse_incremental("echo world", &tree1).unwrap();

        assert!(!tree1.root_node().has_error());
        assert!(!tree2.root_node().has_error());
        assert_eq!(tree1.root_node().kind(), tree2.root_node().kind());
    }
}
