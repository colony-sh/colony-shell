//! Alias expansion module
//!
//! Handles alias expansion with:
//! - Cycle detection to prevent infinite recursion
//! - Re-parsing of alias values containing shell metacharacters
//! - Trailing space handling for chained alias expansion

use colony_expand::ExpansionContext;
use colony_parser::{SimpleCommand, Statement, Word};
use std::collections::HashSet;

/// Result of alias expansion
#[derive(Debug)]
pub enum AliasExpansionResult {
    /// The alias expanded to a simple word substitution (no metacharacters)
    /// Contains the new command and any additional arguments from the alias
    SimpleExpansion {
        new_command: String,
        alias_args: Vec<String>,
        /// If true, the next word should also be checked for alias expansion
        expand_next: bool,
    },
    /// The alias value contains shell metacharacters and was re-parsed
    /// Contains the parsed statements that should replace this command
    Reparsed(Vec<Statement>),
    /// No alias found for this command
    NoAlias,
    /// Alias expansion would cause infinite recursion
    CycleDetected,
}

/// Check if a string contains shell metacharacters that require re-parsing
fn contains_metacharacters(s: &str) -> bool {
    // Characters that change shell interpretation beyond simple word splitting
    s.chars().any(|c| {
        matches!(
            c,
            ';' | '|' | '&' | '(' | ')' | '<' | '>' | '`' | '$' | '\n'
        )
    })
}

/// Expand an alias for the given command name
///
/// # Arguments
/// * `command_name` - The command to check for alias expansion
/// * `context` - The expansion context containing alias definitions
/// * `expanding` - Set of aliases currently being expanded (for cycle detection)
///
/// # Returns
/// An `AliasExpansionResult` indicating what action to take
pub fn expand_alias(
    command_name: &str,
    context: &ExpansionContext,
    expanding: &HashSet<String>,
) -> AliasExpansionResult {
    // Check for cycle - if we're already expanding this alias, stop
    if expanding.contains(command_name) {
        return AliasExpansionResult::CycleDetected;
    }

    // Look up the alias
    let alias_value = match context.get_alias(command_name) {
        Some(value) => value.to_string(),
        None => return AliasExpansionResult::NoAlias,
    };

    // Check if trailing space (for chained expansion like "sudo ")
    let expand_next = alias_value.ends_with(' ') || alias_value.ends_with('\t');

    // Check if the alias value contains metacharacters requiring re-parsing
    if contains_metacharacters(&alias_value) {
        // Re-parse the alias value as shell code
        match colony_parser::pest_api::parse(&alias_value) {
            Ok(program) => AliasExpansionResult::Reparsed(program.statements),
            Err(_) => {
                // If parsing fails, fall back to simple word splitting
                simple_expansion(&alias_value, expand_next)
            }
        }
    } else {
        simple_expansion(&alias_value, expand_next)
    }
}

/// Perform simple word-splitting expansion (no metacharacters)
fn simple_expansion(alias_value: &str, expand_next: bool) -> AliasExpansionResult {
    let words: Vec<String> = alias_value
        .split_whitespace()
        .map(|s| s.to_string())
        .collect();

    if words.is_empty() {
        return AliasExpansionResult::NoAlias;
    }

    AliasExpansionResult::SimpleExpansion {
        new_command: words[0].clone(),
        alias_args: words[1..].to_vec(),
        expand_next,
    }
}

/// Expand aliases in a simple command, handling cycles and re-parsing
///
/// This is the main entry point for alias expansion. It handles:
/// 1. Looking up the alias
/// 2. Cycle detection
/// 3. Re-parsing alias values with metacharacters
/// 4. Trailing space expansion
///
/// # Returns
/// - `Ok(Some(statements))` if the alias expanded to multiple statements (needs re-execution)
/// - `Ok(None)` if simple expansion was done (command was modified in place)
/// - The modified command with expanded alias
#[allow(dead_code)]
pub fn expand_command_alias(
    cmd: &SimpleCommand,
    command_name: &str,
    context: &ExpansionContext,
    expanding: &mut HashSet<String>,
) -> Result<Option<Vec<Statement>>, ()> {
    match expand_alias(command_name, context, expanding) {
        AliasExpansionResult::NoAlias => Ok(None),

        AliasExpansionResult::CycleDetected => {
            // Cycle detected - don't expand, just use the command as-is
            // This matches POSIX behavior where self-referential aliases
            // resolve to the external command
            Ok(None)
        }

        AliasExpansionResult::Reparsed(mut statements) => {
            // The alias value was re-parsed into statements
            // We need to append the original command's arguments to the last command
            if !statements.is_empty() {
                if let Some(Statement::SimpleCommand(last_cmd)) = statements.last_mut() {
                    // Append original arguments
                    last_cmd.args.extend(cmd.args.clone());
                    // Append original redirects
                    last_cmd.redirects.extend(cmd.redirects.clone());
                }
            }
            Ok(Some(statements))
        }

        AliasExpansionResult::SimpleExpansion {
            new_command,
            alias_args: _alias_args,
            expand_next,
        } => {
            // Mark this alias as being expanded (for cycle detection)
            expanding.insert(command_name.to_string());

            // Check if we need to also expand the new command (recursive alias)
            // But only if it's a different name (self-reference is handled by CycleDetected)
            if new_command != command_name {
                if let AliasExpansionResult::Reparsed(statements) =
                    expand_alias(&new_command, context, expanding)
                {
                    expanding.remove(command_name);
                    return Ok(Some(statements));
                }
            }

            // Handle trailing space - check if next word should be expanded
            if expand_next && !cmd.args.is_empty() {
                if let Some(first_arg) = cmd.args.first() {
                    if let Some(arg_name) = first_arg.as_literal() {
                        // Check if the first argument is also an alias
                        if !expanding.contains(arg_name) {
                            if let Some(arg_alias) = context.get_alias(arg_name) {
                                // Recursively expand - this handles cases like:
                                // alias sudo="sudo "
                                // alias ll="ls -la"
                                // sudo ll  -> sudo ls -la
                                let arg_expanded: Vec<String> = arg_alias
                                    .split_whitespace()
                                    .map(|s| s.to_string())
                                    .collect();

                                if !arg_expanded.is_empty() {
                                    // The first argument's alias also expanded
                                    // We don't modify cmd here - the caller will handle it
                                    // by reconstructing the command
                                }
                            }
                        }
                    }
                }
            }

            expanding.remove(command_name);
            Ok(None)
        }
    }
}

/// Build a new SimpleCommand with alias expansion applied
///
/// This creates a new command with the alias value substituted for the command name.
/// It handles simple expansions where the alias doesn't contain metacharacters.
#[allow(dead_code)]
pub fn build_expanded_command(
    original_cmd: &SimpleCommand,
    new_command: &str,
    alias_args: &[String],
    context: &ExpansionContext,
    expanding: &HashSet<String>,
) -> SimpleCommand {
    let mut new_cmd = original_cmd.clone();
    new_cmd.name = Word::literal(new_command, original_cmd.span);

    // Build new args: alias_args first, then original args
    let mut new_args = Vec::new();
    for alias_arg in alias_args {
        new_args.push(Word::literal(alias_arg, original_cmd.span));
    }

    // Handle trailing space expansion for the first original argument
    let alias_value = context.get_alias(original_cmd.name.as_literal().unwrap_or(""));
    let expand_next = alias_value
        .map(|v| v.ends_with(' ') || v.ends_with('\t'))
        .unwrap_or(false);

    let mut skip_first_arg = false;
    if expand_next && !original_cmd.args.is_empty() {
        if let Some(first_arg) = original_cmd.args.first() {
            if let Some(arg_name) = first_arg.as_literal() {
                if !expanding.contains(arg_name) {
                    if let Some(arg_alias) = context.get_alias(arg_name) {
                        // Expand the first argument's alias
                        let arg_words: Vec<String> = arg_alias
                            .split_whitespace()
                            .map(|s| s.to_string())
                            .collect();
                        for word in arg_words {
                            new_args.push(Word::literal(&word, first_arg.span));
                        }
                        skip_first_arg = true;
                    }
                }
            }
        }
    }

    // Add remaining original args
    let args_to_add = if skip_first_arg {
        &original_cmd.args[1..]
    } else {
        &original_cmd.args[..]
    };
    new_args.extend(args_to_add.iter().cloned());

    new_cmd.args = new_args;
    new_cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contains_metacharacters() {
        assert!(contains_metacharacters("echo one; echo two"));
        assert!(contains_metacharacters("cat | wc -l"));
        assert!(contains_metacharacters("echo $VAR"));
        assert!(contains_metacharacters("cat > file"));
        assert!(!contains_metacharacters("ls -la"));
        assert!(!contains_metacharacters("echo hello world"));
    }

    #[test]
    fn test_cycle_detection() {
        let mut context = ExpansionContext::new();
        context.set_alias("ls".to_string(), "ls".to_string());

        let mut expanding = HashSet::new();
        expanding.insert("ls".to_string());

        let result = expand_alias("ls", &context, &expanding);
        assert!(matches!(result, AliasExpansionResult::CycleDetected));
    }

    #[test]
    fn test_simple_expansion() {
        let mut context = ExpansionContext::new();
        context.set_alias("ll".to_string(), "ls -la".to_string());

        let expanding = HashSet::new();
        let result = expand_alias("ll", &context, &expanding);

        match result {
            AliasExpansionResult::SimpleExpansion {
                new_command,
                alias_args,
                expand_next,
            } => {
                assert_eq!(new_command, "ls");
                assert_eq!(alias_args, vec!["-la"]);
                assert!(!expand_next);
            }
            _ => panic!("Expected SimpleExpansion"),
        }
    }

    #[test]
    fn test_trailing_space() {
        let mut context = ExpansionContext::new();
        context.set_alias("sudo".to_string(), "sudo ".to_string());

        let expanding = HashSet::new();
        let result = expand_alias("sudo", &context, &expanding);

        match result {
            AliasExpansionResult::SimpleExpansion { expand_next, .. } => {
                assert!(expand_next);
            }
            _ => panic!("Expected SimpleExpansion"),
        }
    }

    #[test]
    fn test_metacharacter_reparsing() {
        let mut context = ExpansionContext::new();
        context.set_alias("both".to_string(), "echo one; echo two".to_string());

        let expanding = HashSet::new();
        let result = expand_alias("both", &context, &expanding);

        assert!(matches!(result, AliasExpansionResult::Reparsed(_)));
    }
}
