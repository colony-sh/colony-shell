use std::borrow::Cow;

use crate::shell::execution::handle_llm_command;
use colony_agent::{Agent, NaturalLanguageDetector};
use colony_exec::Executor;
use colony_tree_sitter::parse;
use colony_tuple::{Tuple, Value};
use miette::Result;
use rustyline::completion::Completer;
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::{ValidationContext, ValidationResult, Validator};
use rustyline::{Editor, Helper};

use crate::shell::execute_shell_command;

/// Rustyline helper with tree-sitter validator
struct ColonyHelper;

impl ColonyHelper {
    fn new() -> Self {
        Self
    }
}

impl Completer for ColonyHelper {
    type Candidate = String;

    fn complete(
        &self,
        _line: &str,
        _pos: usize,
        _ctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Self::Candidate>)> {
        Ok((0, vec![])) // No completion for now
    }
}

impl Hinter for ColonyHelper {
    type Hint = String;

    fn hint(&self, _line: &str, _pos: usize, _ctx: &rustyline::Context<'_>) -> Option<Self::Hint> {
        None // No hints for now
    }
}

impl Highlighter for ColonyHelper {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        // TODO: Emit OSC 2337 for syntax highlighting
        // For now, just return the line as-is
        Cow::Borrowed(line)
    }

    fn highlight_char(&self, _line: &str, _pos: usize, _forced: bool) -> bool {
        false // Don't force character-level highlighting
    }
}

impl Validator for ColonyHelper {
    fn validate(&self, ctx: &mut ValidationContext) -> rustyline::Result<ValidationResult> {
        let input = ctx.input();

        // Parse with tree-sitter for syntax validation
        let tree = parse(input);

        match tree {
            Some(tree) => {
                let root = tree.root_node();
                let has_error = root.has_error();

                tracing::trace!(
                    "tree-sitter validation: kind={}, has_error={}, byte_range={}..{}",
                    root.kind(),
                    has_error,
                    root.start_byte(),
                    root.end_byte()
                );

                if has_error {
                    // Incomplete input - allow user to continue
                    Ok(ValidationResult::Incomplete)
                } else {
                    Ok(ValidationResult::Valid(None))
                }
            }
            None => Ok(ValidationResult::Incomplete),
        }
    }
}

impl Helper for ColonyHelper {}

pub(crate) async fn run_repl(mut agent: Option<Agent>, mut executor: Executor) -> Result<()> {
    let helper = ColonyHelper::new();
    let mut rl = Editor::new().map_err(|e| miette::miette!("failed to create readline: {}", e))?;
    rl.set_helper(Some(helper));

    loop {
        let prompt = if agent.is_some() {
            "colony> "
        } else {
            "shell> "
        };

        let readline = rl.readline(prompt);

        match readline {
            Ok(line) => {
                let line = line.trim();

                if line.is_empty() {
                    continue;
                }

                // Add to history
                rl.add_history_entry(line)
                    .map_err(|e| miette::miette!("history error: {}", e))?;

                // Handle exit
                if line == "exit" || line == "quit" {
                    println!("Goodbye!");
                    break;
                }

                // Handle llm command (model management)
                if line.starts_with("llm ") {
                    handle_llm_command(line, &mut agent).await;
                    continue;
                }

                // Route to agent or shell
                if let Some(ref mut agent) = agent {
                    if NaturalLanguageDetector::is_natural_language(line) {
                        // Route to agent
                        match agent.process(line).await {
                            Ok(_response) => {
                                println!(); // Newline after streaming
                            }
                            Err(e) => {
                                eprintln!("Agent error: {}", e);
                            }
                        }
                        continue;
                    }
                }

                // Try parsing and executing shell command
                match execute_shell_command(line, &mut executor).await {
                    Ok(()) => {
                        // Display any reactive tuples that were emitted
                        display_tuple_activity(&executor);
                    }
                    Err(e) => {
                        eprintln!("Error: {}", e);
                    }
                }
            }

            Err(ReadlineError::Interrupted) => {
                println!("^C");
                continue;
            }

            Err(ReadlineError::Eof) => {
                println!("exit");
                break;
            }

            Err(err) => {
                eprintln!("Error: {}", err);
                break;
            }
        }
    }

    Ok(())
}

/// Display reactive tuple activity from the tuple space
///
/// Reads and displays env_change and dir_change tuples to show
/// the reactive environment in action (Rules 42-47)
fn display_tuple_activity(executor: &Executor) {
    let tuple_space = executor.tuple_space();

    // Check for env_change tuples: ("env_change", var_name, old_value, new_value)
    let env_pattern = Tuple::pattern(&[
        Value::Str("env_change".to_string()),
        Value::Wildcard,
        Value::Wildcard,
        Value::Wildcard,
    ]);

    while let Some(tuple) = tuple_space.inp(env_pattern.clone()) {
        if let (Some(Value::Str(var)), Some(Value::Str(old)), Some(new)) = (
            tuple.fields.get(1),
            tuple.fields.get(2),
            tuple.fields.get(3),
        ) {
            match new {
                Value::Null => {
                    println!("🔄 Environment: unset {}", var);
                }
                Value::Str(new_val) => {
                    if old.is_empty() {
                        println!("🔄 Environment: {} = {}", var, new_val);
                    } else {
                        println!("🔄 Environment: {} = {} (was: {})", var, new_val, old);
                    }
                }
                _ => {}
            }
        }
    }

    // Check for dir_change tuples: ("dir_change", old_dir, new_dir)
    let dir_pattern = Tuple::pattern(&[
        Value::Str("dir_change".to_string()),
        Value::Wildcard,
        Value::Wildcard,
    ]);

    while let Some(tuple) = tuple_space.inp(dir_pattern.clone()) {
        if let (Some(Value::Str(_old)), Some(Value::Str(new))) =
            (tuple.fields.get(1), tuple.fields.get(2))
        {
            println!("🔄 Directory: {}", new);
        }
    }
}
