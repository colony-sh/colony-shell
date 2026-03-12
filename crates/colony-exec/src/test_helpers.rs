//! Shared test helpers for colony-exec test modules

use colony_parser::{SimpleCommand, Span, Word};

/// Create a `SimpleCommand` from a name and argument strings
pub fn make_simple_command(name: &str, args: Vec<&str>) -> SimpleCommand {
    SimpleCommand {
        name: Word::literal(name, Span::DUMMY),
        args: args
            .into_iter()
            .map(|s| Word::literal(s, Span::DUMMY))
            .collect(),
        redirects: vec![],
        env_assignments: vec![],
        span: Span::DUMMY,
    }
}
