use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;

use crate::shell::execution::handle_llm_command;
use colony_agent::{Agent, NaturalLanguageDetector};
use colony_exec::Executor;
use colony_tree_sitter::parse;
use colony_tuple::{Tuple, Value};
use colony_ui_core::{ComponentContext, PromptManager};
use miette::Result;
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::{ValidationContext, ValidationResult, Validator};
use rustyline::history::DefaultHistory;
use rustyline::{Editor, Helper};
use tokio::sync::RwLock;

use crate::shell::execute_shell_command;

struct ColonyHelper {
    commands: Vec<String>,
}

impl ColonyHelper {
    fn new() -> Self {
        Self {
            commands: Self::collect_commands(),
        }
    }

    fn collect_commands() -> Vec<String> {
        let mut commands = vec![
            "cd", "pwd", "echo", "export", "unset", "source", "exit", "history",
            "jobs", "fg", "bg", "alias", "unalias", "set", "shift", "test",
            "true", "false", "read", "printf", "type", "hash", "ulimit",
            "umask", "wait", "trap", "return", "break", "continue", "eval",
            "exec", "command", "builtin", "local", "declare", "readonly",
            "getopts", "times", "llm", "path",
        ].into_iter().map(String::from).collect::<Vec<_>>();

        if let Ok(path) = std::env::var("PATH") {
            for dir in path.split(':') {
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        if let Ok(metadata) = entry.metadata() {
                            if metadata.is_file() && is_executable(&metadata) {
                                if let Some(name) = entry.file_name().to_str() {
                                    if !commands.contains(&name.to_string()) {
                                        commands.push(name.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        commands.sort();
        commands.dedup();
        commands
    }

    fn complete_path(&self, partial: &str, dirs_only: bool) -> Vec<Pair> {
        let mut results = Vec::new();

        let (dir, prefix) = if partial.contains('/') {
            let path = PathBuf::from(partial);
            if partial.ends_with('/') {
                (partial.to_string(), String::new())
            } else {
                let parent = path.parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
                let file = path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
                (if parent.is_empty() { ".".to_string() } else { parent }, file)
            }
        } else {
            (".".to_string(), partial.to_string())
        };

        let expanded_dir = if dir.starts_with('~') {
            if let Ok(home) = std::env::var("HOME") {
                dir.replacen('~', &home, 1)
            } else {
                dir.clone()
            }
        } else {
            dir.clone()
        };

        if let Ok(entries) = std::fs::read_dir(&expanded_dir) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.starts_with(&prefix) {
                        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);

                        if dirs_only && !is_dir {
                            continue;
                        }

                        let display = if dir == "." {
                            name.to_string()
                        } else if dir.ends_with('/') {
                            format!("{}{}", dir, name)
                        } else {
                            format!("{}/{}", dir, name)
                        };

                        let replacement = if is_dir {
                            format!("{}/", display)
                        } else {
                            display.clone()
                        };

                        results.push(Pair {
                            display,
                            replacement,
                        });
                    }
                }
            }
        }

        results.sort_by(|a, b| a.display.cmp(&b.display));
        results
    }

    fn complete_command(&self, partial: &str) -> Vec<Pair> {
        self.commands
            .iter()
            .filter(|cmd| cmd.starts_with(partial))
            .map(|cmd| Pair {
                display: cmd.clone(),
                replacement: cmd.clone(),
            })
            .collect()
    }
}

#[cfg(unix)]
fn is_executable(metadata: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_metadata: &std::fs::Metadata) -> bool {
    true
}

impl Completer for ColonyHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Self::Candidate>)> {
        let line_to_cursor = &line[..pos];

        let words: Vec<&str> = line_to_cursor.split_whitespace().collect();
        let trailing_space = line_to_cursor.ends_with(' ') || line_to_cursor.ends_with('\t');

        if words.is_empty() || (words.len() == 1 && !trailing_space) {
            let partial = words.first().copied().unwrap_or("");
            let start = line_to_cursor.rfind(char::is_whitespace).map(|i| i + 1).unwrap_or(0);
            let completions = self.complete_command(partial);
            return Ok((start, completions));
        }

        let cmd = words[0];
        let partial = if trailing_space { "" } else { words.last().copied().unwrap_or("") };
        let start = if trailing_space {
            pos
        } else {
            line_to_cursor.rfind(char::is_whitespace).map(|i| i + 1).unwrap_or(0)
        };

        let dirs_only = cmd == "cd" || cmd == "pushd" || cmd == "popd";
        let completions = self.complete_path(partial, dirs_only);

        Ok((start, completions))
    }
}

impl Hinter for ColonyHelper {
    type Hint = String;

    fn hint(&self, _line: &str, _pos: usize, _ctx: &rustyline::Context<'_>) -> Option<Self::Hint> {
        None
    }
}

impl Highlighter for ColonyHelper {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        Cow::Borrowed(line)
    }

    fn highlight_char(&self, _line: &str, _pos: usize, _forced: bool) -> bool {
        false
    }
}

impl Validator for ColonyHelper {
    fn validate(&self, ctx: &mut ValidationContext) -> rustyline::Result<ValidationResult> {
        let input = ctx.input();
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

pub(crate) async fn run_repl(
    mut agent: Option<Agent>,
    mut executor: Executor,
    cwd_handle: Option<Arc<RwLock<PathBuf>>>,
) -> Result<()> {
    let helper = ColonyHelper::new();
    let mut rl = Editor::new().map_err(|e| miette::miette!("failed to create readline: {}", e))?;
    rl.set_helper(Some(helper));

    configure_history(&mut executor, &mut rl);

    let mut prompt_manager = PromptManager::default();

    loop {
        let model_name = agent.as_ref().map(|a| a.current_provider().1.to_string());
        let ctx = ComponentContext::from_executor(&executor, model_name);
        let terminal_width = terminal_size::terminal_size()
            .map(|(w, _)| w.0 as usize)
            .unwrap_or(80);
        let prompt = prompt_manager.render(&ctx, terminal_width);

        let readline = rl.readline(&prompt);

        match readline {
            Ok(line) => {
                let line = line.trim();

                if line.is_empty() {
                    continue;
                }

                let line = match executor.history().expand(line) {
                    Ok((expanded, was_expanded)) => {
                        if was_expanded {
                            println!("{}", expanded);
                        }
                        expanded
                    }
                    Err(e) => {
                        eprintln!("colony: {}", e);
                        continue;
                    }
                };
                let line = line.as_str();

                rl.add_history_entry(line)
                    .map_err(|e| miette::miette!("history error: {}", e))?;

                executor.history_mut().add(line);

                if line == "exit" || line == "quit" {
                    println!("Goodbye!");
                    save_history(&executor);
                    break;
                }

                if line.starts_with("llm ") {
                    handle_llm_command(line, &mut agent).await;
                    continue;
                }

                if let Some(ref mut agent) = agent {
                    if NaturalLanguageDetector::is_natural_language(line) {
                        match agent.process(line).await {
                            Ok(_response) => {
                                println!();
                            }
                            Err(e) => {
                                eprintln!("Agent error: {}", e);
                            }
                        }
                        continue;
                    }
                }

                match execute_shell_command(line, &mut executor).await {
                    Ok(()) => {
                        prompt_manager.on_event("cmd_complete");
                        display_tuple_activity(&executor, &mut prompt_manager, &cwd_handle).await;
                    }
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        prompt_manager.on_event("cmd_complete");
                    }
                }
            }

            Err(ReadlineError::Interrupted) => {
                println!("^C");
                continue;
            }

            Err(ReadlineError::Eof) => {
                println!("exit");
                save_history(&executor);
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

async fn display_tuple_activity(
    executor: &Executor,
    prompt_manager: &mut PromptManager,
    cwd_handle: &Option<Arc<RwLock<PathBuf>>>,
) {
    let tuple_space = executor.tuple_space();

    let env_pattern = Tuple::pattern(&[
        Value::Str("env_change".to_string()),
        Value::Wildcard,
        Value::Wildcard,
        Value::Wildcard,
    ]);

    while let Some(tuple) = tuple_space.inp(env_pattern.clone()) {
        prompt_manager.on_event("env_change");
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

    let dir_pattern = Tuple::pattern(&[
        Value::Str("dir_change".to_string()),
        Value::Wildcard,
        Value::Wildcard,
    ]);

    while let Some(tuple) = tuple_space.inp(dir_pattern.clone()) {
        prompt_manager.on_event("dir_change");
        if let (Some(Value::Str(_old)), Some(Value::Str(new))) =
            (tuple.fields.get(1), tuple.fields.get(2))
        {
            println!("🔄 Directory: {}", new);
            if let Some(handle) = cwd_handle {
                handle.write().await.clone_from(&PathBuf::from(new));
            }
        }
    }
}

fn configure_history(executor: &mut Executor, rl: &mut Editor<ColonyHelper, DefaultHistory>) {
    let get_env = |name: &str| std::env::var(name).ok();
    executor.history_mut().configure_from_env(get_env);

    let history_file = get_env("HISTFILE")
        .map(PathBuf::from)
        .or_else(|| get_env("HOME").map(|h| PathBuf::from(h).join(".colony_history")));

    if let Some(path) = history_file {
        executor.history_mut().set_file_path(path);
        executor.history_mut().load_from_file();
    }

    for entry in executor.history().entries() {
        let _ = rl.add_history_entry(entry);
    }
}

fn save_history(executor: &Executor) {
    if let Err(e) = executor.history().save_to_file() {
        eprintln!("Warning: failed to save history: {}", e);
    }
}
