use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crate::shell::execution::handle_llm_command;
use colony_agent::{Agent, NaturalLanguageDetector};
use colony_exec::Executor;
use colony_history::{HistoryStore, HistoryWriter, PrivacyAction, PrivacyFilter};
use colony_protocol::{
    emit_prompt, emit_prompt_busy, emit_prompt_ready, emit_prompt_state, GitStateData,
    InteractiveConfig, PromptComponentData, PromptSpan, PromptStateData,
};
use colony_tree_sitter::parse;
use colony_tuple::{Tuple, Value};
use colony_ui_core::{color_to_string, ComponentContext, PromptData, PromptManager};
use miette::Result;
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::{ValidationContext, ValidationResult, Validator};
use rustyline::{Editor, Helper};
use tokio::sync::RwLock;

use crate::shell::{execute_shell_command, is_colony_terminal};

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
            "cd", "pwd", "echo", "export", "unset", "source", "exit", "history", "jobs", "fg",
            "bg", "alias", "unalias", "set", "shift", "test", "true", "false", "read", "printf",
            "type", "hash", "ulimit", "umask", "wait", "trap", "return", "break", "continue",
            "eval", "exec", "command", "builtin", "local", "declare", "readonly", "getopts",
            "times", "llm", "path",
        ]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();

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
                let parent = path
                    .parent()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();
                let file = path
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_default();
                (
                    if parent.is_empty() {
                        ".".to_string()
                    } else {
                        parent
                    },
                    file,
                )
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
            let start = line_to_cursor
                .rfind(char::is_whitespace)
                .map(|i| i + 1)
                .unwrap_or(0);
            let completions = self.complete_command(partial);
            return Ok((start, completions));
        }

        let cmd = words[0];
        let partial = if trailing_space {
            ""
        } else {
            words.last().copied().unwrap_or("")
        };
        let start = if trailing_space {
            pos
        } else {
            line_to_cursor
                .rfind(char::is_whitespace)
                .map(|i| i + 1)
                .unwrap_or(0)
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

/// Drain pending LLM messages from the agent's channel and write to the executor's ChannelManager.
fn drain_llm_channel(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<colony_channels::LlmMessage>,
    executor: &mut Executor,
) {
    while let Ok(msg) = rx.try_recv() {
        let Some(llm_ch) = executor.channels_mut().llm() else {
            break;
        };
        llm_ch.add_message(msg);
    }
}

pub(crate) async fn run_repl(
    mut agent: Option<Agent>,
    mut executor: Executor,
    cwd_handle: Option<Arc<RwLock<PathBuf>>>,
    _history_writer: Option<HistoryWriter>,
    _history_store: Option<Arc<HistoryStore>>,
) -> Result<()> {
    // Set up LLM channel bridge: agent -> mpsc -> ChannelManager
    let mut llm_channel_rx = agent.as_mut().map(|a| a.set_llm_channel());

    let helper = ColonyHelper::new();
    let mut rl = Editor::new().map_err(|e| miette::miette!("failed to create readline: {}", e))?;
    rl.set_helper(Some(helper));

    configure_history(&mut executor, &mut rl);

    let privacy_filter = PrivacyFilter::new(vec![]);
    let session_id = executor.session_id().to_string();

    let mut prompt_manager = PromptManager::default();
    let use_osc = is_colony_terminal();

    loop {
        let model_name = agent.as_ref().map(|a| a.current_provider().1.to_string());
        let ctx = ComponentContext::from_executor(&executor, model_name.clone());
        let terminal_width = terminal_size::terminal_size()
            .map(|(w, _)| w.0 as usize)
            .unwrap_or(80);

        // Emit prompt data via OSC 2337 (only when running in colony-terminal)
        if use_osc {
            let prompt_data = prompt_manager.render_data(&ctx);
            emit_prompt_osc(&prompt_data);

            // Emit full PromptState for reactive terminal rendering
            let state = build_prompt_state(&ctx, &prompt_data, model_name.as_deref());
            emit_prompt_state(state);

            // Signal shell is ready for input
            emit_prompt_ready(None);
        }

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

                // Agent builtin — intercept here because agent lives in REPL, not executor
                if line == "agent" || line.starts_with("agent ") {
                    handle_agent_builtin(line, &mut agent).await;
                    continue;
                }

                // Intercept `colony onboard` to run the wizard from the REPL
                if line == "colony onboard" || line.starts_with("colony onboard ") {
                    crate::onboarding::wizard::run();
                    continue;
                }

                if line.starts_with("llm ") {
                    // Channel builtins (show/export/clear) fall through to the executor
                    let sub = line.split_whitespace().nth(1).unwrap_or("");
                    if !matches!(sub, "show" | "export" | "clear") {
                        handle_llm_command(line, &mut agent).await;
                        // Drain LLM channel messages after agent interaction
                        if let Some(ref mut rx) = llm_channel_rx {
                            drain_llm_channel(rx, &mut executor);
                        }
                        continue;
                    }
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
                        // Drain LLM channel messages into the executor's ChannelManager
                        if let Some(ref mut rx) = llm_channel_rx {
                            drain_llm_channel(rx, &mut executor);
                        }
                        continue;
                    }
                }

                // Emit Prompt=busy before command execution
                if use_osc {
                    emit_prompt_busy(line);
                }
                let cmd_start = Instant::now();

                let exec_result = execute_shell_command(line, &mut executor).await;
                let duration_ms = cmd_start.elapsed().as_millis() as u64;
                let exit_code = executor.context().last_exit_status();

                // Record command to SQLite history store
                if let Some(ref writer) = _history_writer {
                    if privacy_filter.should_record(line) == PrivacyAction::Record {
                        let cwd = std::env::current_dir()
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_default();
                        let record = colony_history::HistoryRecord::builder()
                            .command(line)
                            .cwd(&cwd)
                            .exit_code(exit_code)
                            .duration_ms(duration_ms)
                            .session_id(&session_id)
                            .build();
                        if let Ok(record) = record {
                            writer.send(record);
                        }
                    }
                }

                match exec_result {
                    Ok(()) => {
                        prompt_manager.on_event("cmd_complete");
                        display_tuple_activity(
                            &mut executor,
                            &mut prompt_manager,
                            &cwd_handle,
                            &mut agent,
                        )
                        .await;

                        // Wire LLM channel if a new agent was created during tuple processing
                        if agent.is_some() && llm_channel_rx.is_none() {
                            if let Some(ref mut a) = agent {
                                llm_channel_rx = Some(a.set_llm_channel());
                            }
                        }

                        // Emit Prompt=ready after command completes
                        if use_osc {
                            emit_prompt_ready(Some(duration_ms));
                        }
                    }
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        prompt_manager.on_event("cmd_complete");

                        if use_osc {
                            emit_prompt_ready(Some(duration_ms));
                        }
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

/// Build a PromptStateData from the current context and rendered prompt data
pub(crate) fn build_prompt_state(
    ctx: &ComponentContext,
    prompt_data: &PromptData,
    model: Option<&str>,
) -> PromptStateData {
    let cwd = ctx.cwd.to_string_lossy().to_string();
    let display_path = prompt_data
        .left
        .iter()
        .find(|c| c.id == "pwd")
        .and_then(|c| c.output.data.as_ref())
        .and_then(|d| d.get("display"))
        .and_then(|v| v.as_str())
        .unwrap_or(&cwd)
        .to_string();

    // Extract git state from rendered component data
    let git = prompt_data
        .left
        .iter()
        .chain(prompt_data.right.iter())
        .find(|c| c.id == "git_branch")
        .and_then(|branch_data| {
            let branch = branch_data
                .output
                .data
                .as_ref()?
                .get("branch")?
                .as_str()?
                .to_string();
            let detached = branch_data
                .output
                .data
                .as_ref()
                .and_then(|d| d.get("detached"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            // Extract git status from status component
            let status_data = prompt_data
                .left
                .iter()
                .chain(prompt_data.right.iter())
                .find(|c| c.id == "git_status");

            let (staged, modified, untracked, insertions, deletions) = status_data
                .and_then(|sd| sd.output.data.as_ref())
                .map(|d| {
                    (
                        d.get("staged").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
                        d.get("modified").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
                        d.get("untracked").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
                        d.get("insertions").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
                        d.get("deletions").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
                    )
                })
                .unwrap_or_default();

            Some(GitStateData {
                branch,
                detached,
                commit: None,
                staged,
                modified,
                untracked,
                conflicted: 0,
                ahead: None,
                behind: None,
                insertions,
                deletions,
            })
        });

    let shlvl = std::env::var("SHLVL")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);

    let in_job = ctx.running_jobs > 0;

    PromptStateData {
        cwd,
        display_path,
        user: std::env::var("USER").ok(),
        host: ctx.hostname.clone(),
        exit_status: ctx.last_exit_code,
        git,
        agent: None, // Agent state populated when agent is active
        shlvl,
        in_job,
        model: model.map(|s| s.to_string()),
    }
}

pub(crate) async fn display_tuple_activity(
    executor: &mut Executor,
    prompt_manager: &mut PromptManager,
    cwd_handle: &Option<Arc<RwLock<PathBuf>>>,
    agent: &mut Option<Agent>,
) {
    // Collect tuples first, then process (avoids holding immutable borrow across mutable calls)
    let env_tuples: Vec<Tuple> = {
        let tuple_space = executor.tuple_space();
        let session_id = executor.session_id();
        // Pattern: ("env_change", session_id, var, old, new)
        let env_pattern = Tuple::pattern(&[
            Value::Str("env_change".to_string()),
            Value::Str(session_id.to_string()),
            Value::Wildcard,
            Value::Wildcard,
            Value::Wildcard,
        ]);
        let mut tuples = Vec::new();
        while let Some(tuple) = tuple_space.inp(env_pattern.clone()) {
            tuples.push(tuple);
        }
        tuples
    };

    for tuple in &env_tuples {
        prompt_manager.on_event("env_change");
        if let (Some(Value::Str(var)), Some(Value::Str(old)), Some(new)) = (
            tuple.fields.get(2),
            tuple.fields.get(3),
            tuple.fields.get(4),
        ) {
            match new {
                Value::Null => {
                    println!("  Environment: unset {}", var);
                }
                Value::Str(new_val) => {
                    if old.is_empty() {
                        println!("  Environment: {} = {}", var, new_val);
                    } else {
                        println!("  Environment: {} = {} (was: {})", var, new_val, old);
                    }
                }
                _ => {}
            }
        }
    }

    let dir_tuples: Vec<Tuple> = {
        let tuple_space = executor.tuple_space();
        let session_id = executor.session_id();
        // Pattern: ("dir_change", session_id, old, new)
        let dir_pattern = Tuple::pattern(&[
            Value::Str("dir_change".to_string()),
            Value::Str(session_id.to_string()),
            Value::Wildcard,
            Value::Wildcard,
        ]);
        let mut tuples = Vec::new();
        while let Some(tuple) = tuple_space.inp(dir_pattern.clone()) {
            tuples.push(tuple);
        }
        tuples
    };

    for tuple in &dir_tuples {
        prompt_manager.on_event("dir_change");
        if let (Some(Value::Str(_old)), Some(Value::Str(new))) =
            (tuple.fields.get(2), tuple.fields.get(3))
        {
            println!("  Directory: {}", new);
            if let Some(handle) = cwd_handle {
                handle.write().await.clone_from(&PathBuf::from(new));
            }

            // Load special files from new directory with hierarchy walk (F030)
            let new_path = PathBuf::from(new);
            if colony_exec::special_files::SpecialFilesLoader::has_special_files_in_hierarchy(&new_path) {
                executor.load_special_files(&new_path);

                if let Some(state) = executor.special_files() {
                    if !state.agents.is_empty() {
                        let names: Vec<&str> =
                            state.agents.iter().map(|a| a.name.as_str()).collect();
                        println!("Agents loaded: {}", names.join(", "));

                        // Configure the agent from the first AGENT.md definition
                        if let Some(first_def) = state.agents.first() {
                            match agent {
                                Some(ref mut a) => {
                                    if let Err(e) = a.configure_from_definition(first_def).await {
                                        eprintln!(
                                            "Warning: failed to configure agent from {}: {}",
                                            first_def.name, e
                                        );
                                    } else {
                                        println!(
                                            "Agent configured: {} (model: {})",
                                            first_def.name,
                                            a.current_provider().1,
                                        );
                                    }
                                }
                                None => {
                                    // No agent yet, create one from the definition
                                    match Agent::from_definition(first_def).await {
                                        Ok(new_agent) => {
                                            println!(
                                                "Agent created: {} (model: {})",
                                                first_def.name,
                                                new_agent.current_provider().1,
                                            );
                                            *agent = Some(new_agent);
                                        }
                                        Err(e) => {
                                            eprintln!(
                                                "Warning: failed to create agent from {}: {}",
                                                first_def.name, e
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if !state.context_entries.is_empty() {
                        println!("Context seeded: {} entries", state.context_entries.len());
                    }
                }
            }
        }
    }

    // Collect and display heartbeat findings (F080)
    let heartbeat_tuples: Vec<Tuple> = {
        let tuple_space = executor.tuple_space();
        let heartbeat_pattern = Tuple::pattern(&[
            Value::Str("heartbeat_finding".to_string()),
            Value::Wildcard,
            Value::Wildcard,
            Value::Wildcard,
            Value::Wildcard,
        ]);
        let mut tuples = Vec::new();
        while let Some(tuple) = tuple_space.inp(heartbeat_pattern.clone()) {
            tuples.push(tuple);
        }
        tuples
    };

    for tuple in &heartbeat_tuples {
        if let (
            Some(Value::Str(_path)),
            Some(Value::Str(_run_id)),
            Some(Value::Str(severity)),
            Some(Value::Str(message)),
        ) = (
            tuple.fields.get(1),
            tuple.fields.get(2),
            tuple.fields.get(3),
            tuple.fields.get(4),
        ) {
            println!("[heartbeat] {severity}: {message}");
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

async fn handle_agent_builtin(line: &str, agent: &mut Option<Agent>) {
    let args: Vec<&str> = line.split_whitespace().collect();
    let subcmd = args.get(1).copied().unwrap_or("status");

    match subcmd {
        "status" => {
            let Some(agent) = agent.as_ref() else {
                println!("Agent: not initialized");
                println!("  Start a conversation with # or ? prefix to activate.");
                return;
            };
            let (provider, model) = agent.current_provider();
            let stats = agent.usage_stats();
            let util = agent.context_utilization();
            let msg_count = agent.message_count();
            let warning = agent.budget_warning();

            println!("Agent: active");
            println!("  Provider:    {:?}", provider);
            println!("  Model:       {}", model);
            println!("  Messages:    {}", msg_count);
            println!("  Context:     {:.0}% utilized", util * 100.0);
            println!("  Tokens in:   {}", stats.total_input_tokens);
            println!("  Tokens out:  {}", stats.total_output_tokens);
            println!("  Requests:    {}", stats.request_count);
            if let Some(conv_id) = agent.conversation_id() {
                println!("  Conversation: {}", conv_id);
            }
            if !matches!(warning, colony_agent::BudgetWarning::None) {
                println!("  Budget:      {:?}", warning);
            }
        }
        "model" => {
            let Some(agent) = agent.as_ref() else {
                eprintln!("agent: not initialized");
                return;
            };
            let (provider, model) = agent.current_provider();
            println!("{:?}:{}", provider, model);
        }
        "switch" => {
            let Some(spec) = args.get(2) else {
                eprintln!("Usage: agent switch <provider:model>");
                eprintln!("  e.g. agent switch anthropic:claude-sonnet-4-5-20250514");
                eprintln!("       agent switch openai:gpt-4o");
                eprintln!("       agent switch ollama:llama3");
                return;
            };
            let Some(agent) = agent.as_mut() else {
                eprintln!("agent: not initialized");
                return;
            };
            match agent.switch_to(spec).await {
                Ok(()) => {
                    let (provider, model) = agent.current_provider();
                    println!("Switched to {:?}:{}", provider, model);
                }
                Err(e) => eprintln!("agent switch: {}", e),
            }
        }
        "reset" => {
            let Some(agent) = agent.as_mut() else {
                eprintln!("agent: not initialized");
                return;
            };
            agent.clear_messages();
            println!("Conversation cleared. System prompt retained.");
        }
        "tools" => {
            let Some(agent) = agent.as_ref() else {
                eprintln!("agent: not initialized");
                return;
            };
            let names = agent.tools().tool_names();
            if names.is_empty() {
                println!("No tools registered.");
            } else {
                println!("{} tools registered:", names.len());
                for name in &names {
                    let classification = agent.tools().classification(name);
                    let badge = match classification {
                        Some(colony_agent::ToolClassification::Programmatic) => "auto",
                        Some(colony_agent::ToolClassification::UserFacing) => "approval",
                        None => "unknown",
                    };
                    println!("  {:<30} [{}]", name, badge);
                }
            }
        }
        "history" => {
            let Some(agent) = agent.as_ref() else {
                eprintln!("agent: not initialized");
                return;
            };
            let Some(store) = agent.conversation_store() else {
                println!("Conversation persistence not enabled.");
                return;
            };
            let limit = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(10);
            match store.list_conversations(limit) {
                Ok(convs) => {
                    if convs.is_empty() {
                        println!("No saved conversations.");
                    } else {
                        println!("{} most recent conversations:", convs.len());
                        for conv in &convs {
                            let title = conv.title.as_deref().unwrap_or("untitled");
                            let date = conv.updated_at.format("%Y-%m-%d %H:%M");
                            println!(
                                "  {}  {:30} {:>3} msgs  {}",
                                &conv.id[..8],
                                title,
                                conv.message_count,
                                date
                            );
                        }
                    }
                }
                Err(e) => eprintln!("agent history: {}", e),
            }
        }
        "messages" => {
            let Some(agent) = agent.as_ref() else {
                eprintln!("agent: not initialized");
                return;
            };
            println!("{} messages in conversation", agent.message_count());
        }
        "config" => {
            let Some(agent) = agent.as_ref() else {
                eprintln!("agent: not initialized");
                return;
            };
            let (provider, model) = agent.current_provider();
            let config = agent.config();
            println!("provider:      {:?}", provider);
            println!("model:         {}", model);
            println!("max_tokens:    {}", config.max_tokens);
            println!("temperature:   {}", config.temperature.map(|t| format!("{:.1}", t)).unwrap_or_else(|| "default".to_string()));
            println!("caching:       {}", config.use_caching);
            println!("persistence:   {}", config.persistence_enabled);
            println!("memory:        {}", config.memory_enabled);
            println!("context:       {:.0}%", agent.context_utilization() * 100.0);
            if !config.auto_approve_patterns.is_empty() {
                println!("auto_approve:  {}", config.auto_approve_patterns.join(", "));
            }
        }
        "help" | "--help" | "-h" => {
            println!("Usage: agent <subcommand>");
            println!();
            println!("Subcommands:");
            println!("  status                Show agent status, model, token usage (default)");
            println!("  model                 Show current provider:model");
            println!("  switch <provider:model> Switch to a different model");
            println!("  reset                 Clear conversation history");
            println!("  tools                 List registered tools with classifications");
            println!("  history [N]           Show N most recent conversations (default 10)");
            println!("  messages              Show conversation message count");
            println!("  config                Show full agent configuration");
            println!("  help                  Show this help");
        }
        other => {
            eprintln!("agent: unknown subcommand '{}'. Try 'agent help'.", other);
        }
    }
}

fn emit_prompt_osc(data: &PromptData) {
    let convert_components =
        |components: &[colony_ui_core::ComponentData]| -> Vec<PromptComponentData> {
            components
                .iter()
                .map(|cd| {
                    let spans = cd
                        .output
                        .spans
                        .iter()
                        .map(|s| PromptSpan {
                            text: s.text.clone(),
                            fg: s.style.fg.as_ref().map(color_to_string),
                            bg: s.style.bg.as_ref().map(color_to_string),
                            bold: s.style.bold,
                            dim: s.style.dim,
                            italic: s.style.italic,
                        })
                        .collect();

                    let interactive = cd.output.interactive.as_ref().map(|i| InteractiveConfig {
                        click: i.click.clone(),
                        hover: i.hover.clone(),
                    });

                    PromptComponentData {
                        id: cd.id.clone(),
                        spans,
                        data: cd.output.data.clone(),
                        interactive,
                    }
                })
                .collect()
        };

    let left = convert_components(&data.left);
    let right = convert_components(&data.right);
    emit_prompt(left, right);
}
