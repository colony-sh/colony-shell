//! TUI-based REPL using colony-tui's InputEditor and ratatui rendering.
//!
//! This module provides an alternative to the rustyline-based REPL that uses
//! vim-mode editing (via `InputEditor`), ratatui-based prompt rendering, and
//! the same command execution pipeline as the standard REPL.

use std::io::{self, Stdout};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use colony_agent::{Agent, NaturalLanguageDetector};
use colony_exec::Executor;
use colony_protocol::{emit_prompt_busy, emit_prompt_ready, emit_prompt_state};
use colony_tui::editor::{CompletionContext, CompletionEngine, EditorAction, EditorHistory};
use colony_tui::{InputEditor, LegacyPromptRenderer as PromptRenderer};
use colony_ui_core::{ComponentContext, PromptManager};
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use crossterm::{cursor, execute};
use miette::Result;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Terminal;
use tokio::sync::RwLock;

use crate::shell::execute_shell_command;
use crate::shell::execution::handle_llm_command;

/// Returns true if running inside colony-terminal (COLONY_TERMINAL env var set)
fn is_colony_terminal() -> bool {
    std::env::var("COLONY_TERMINAL").is_ok()
}

/// Guard that restores terminal state on drop.
struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalGuard {
    fn new() -> io::Result<Self> {
        enable_raw_mode()?;
        let backend = CrosstermBackend::new(io::stdout());
        let terminal = Terminal::new(backend)?;
        Ok(Self { terminal })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), cursor::Show);
    }
}

/// Run the TUI-based REPL loop.
///
/// Uses `InputEditor` for vim-mode line editing and ratatui for prompt rendering.
/// Commands are fed into the same `execute_shell_command` pipeline as the
/// rustyline-based REPL.
pub(crate) async fn run_repl_tui(
    mut agent: Option<Agent>,
    mut executor: Executor,
    cwd_handle: Option<Arc<RwLock<PathBuf>>>,
) -> Result<()> {
    let mut guard = TerminalGuard::new()
        .map_err(|e| miette::miette!("failed to initialize TUI terminal: {}", e))?;

    let mut editor = InputEditor::new();
    let mut history = EditorHistory::new();
    let mut prompt_manager = PromptManager::default();
    let use_osc = is_colony_terminal();

    // Load shell history into editor history
    for entry in executor.history().entries() {
        history.add(entry.to_string());
    }

    configure_history_from_env(&mut executor);

    loop {
        let model_name = agent.as_ref().map(|a| a.current_provider().1.to_string());
        let ctx = ComponentContext::from_executor(&executor, model_name.clone());

        if use_osc {
            let prompt_data = prompt_manager.render_data(&ctx);
            let state = super::repl::build_prompt_state(&ctx, &prompt_data, model_name.as_deref());
            emit_prompt_state(state);
            emit_prompt_ready(None);
        }

        // Draw the prompt and input line
        draw_prompt_and_input(&mut guard.terminal, &prompt_manager, &ctx, &editor)?;

        // Read input line using the InputEditor event loop
        let line = match read_input_line(
            &mut guard.terminal,
            &mut editor,
            &mut history,
            &prompt_manager,
            &ctx,
            &executor,
        )
        .await?
        {
            Some(line) => line,
            None => {
                // Ctrl+D or quit
                println!("\r\nexit");
                save_history(&executor);
                break;
            }
        };

        let line = line.trim().to_string();
        if line.is_empty() {
            editor.reset();
            continue;
        }

        // Expand history (!-expansions)
        let line = match executor.history().expand(&line) {
            Ok((expanded, was_expanded)) => {
                if was_expanded {
                    // Temporarily leave raw mode to print expansion
                    let _ = disable_raw_mode();
                    println!("\r\n{}", expanded);
                    let _ = enable_raw_mode();
                }
                expanded
            }
            Err(e) => {
                let _ = disable_raw_mode();
                eprintln!("\r\ncolony: {}", e);
                let _ = enable_raw_mode();
                editor.reset();
                continue;
            }
        };

        // Add to history
        history.add(line.clone());
        executor.history_mut().add(&line);

        // Reset editor for next input
        editor.reset();

        if line == "exit" || line == "quit" {
            let _ = disable_raw_mode();
            println!("\r\nGoodbye!");
            save_history(&executor);
            break;
        }

        // Leave raw mode for command execution (commands expect normal terminal)
        let _ = disable_raw_mode();
        // Move to a new line after the prompt
        println!();

        if line.starts_with("llm ") {
            handle_llm_command(&line, &mut agent).await;
            let _ = enable_raw_mode();
            continue;
        }

        if let Some(ref mut agent) = agent {
            if NaturalLanguageDetector::is_natural_language(&line) {
                match agent.process(&line).await {
                    Ok(_response) => {
                        println!();
                    }
                    Err(e) => {
                        eprintln!("Agent error: {}", e);
                    }
                }
                let _ = enable_raw_mode();
                continue;
            }
        }

        if use_osc {
            emit_prompt_busy(&line);
        }
        let cmd_start = Instant::now();

        match execute_shell_command(&line, &mut executor).await {
            Ok(()) => {
                let duration_ms = cmd_start.elapsed().as_millis() as u64;
                prompt_manager.on_event("cmd_complete");
                display_tuple_activity(
                    &mut executor,
                    &mut prompt_manager,
                    &cwd_handle,
                    &mut agent,
                )
                .await;

                if use_osc {
                    emit_prompt_ready(Some(duration_ms));
                }
            }
            Err(e) => {
                let duration_ms = cmd_start.elapsed().as_millis() as u64;
                eprintln!("Error: {}", e);
                prompt_manager.on_event("cmd_complete");

                if use_osc {
                    emit_prompt_ready(Some(duration_ms));
                }
            }
        }

        // Re-enable raw mode for the next prompt
        let _ = enable_raw_mode();
    }

    Ok(())
}

/// Draw the prompt and current editor input.
fn draw_prompt_and_input(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    prompt_manager: &PromptManager,
    ctx: &ComponentContext,
    editor: &InputEditor,
) -> Result<()> {
    let prompt_data = prompt_manager.render_data(ctx);
    let (left_line, _right_line) = PromptRenderer::render(&prompt_data);
    let cursor_row = editor.buffer.cursor_row();
    let cursor_col = editor.buffer.cursor_col();
    let status = editor.status_line();

    terminal
        .draw(|frame| {
            let area = frame.area();

            // Layout: status bar (1 line) at top, prompt+input fills the rest
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(1),    // prompt + input area
                    Constraint::Length(1), // status line
                ])
                .split(area);

            // Render prompt + input in the main area
            let prompt_spans = left_line.spans.clone();
            let prompt_text = prompt_spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>();
            let prompt_width = prompt_text.len() as u16;

            let mut input_lines: Vec<Line<'_>> = Vec::new();

            // First line: prompt + first line of input
            let first_input_line = editor.buffer.lines().first().map_or("", |s| s.as_str());
            let mut first_spans = prompt_spans;
            first_spans.push(Span::raw(first_input_line.to_string()));
            input_lines.push(Line::from(first_spans));

            // Subsequent lines: continuation of multi-line input
            for line in editor.buffer.lines().iter().skip(1) {
                let continuation = format!("{:>width$}{}", "> ", line, width = prompt_width as usize - 2);
                input_lines.push(Line::from(continuation));
            }

            let input_paragraph = Paragraph::new(input_lines);
            frame.render_widget(input_paragraph, chunks[0]);

            // Render status bar
            let status_line = Line::from(vec![Span::styled(
                format!(" {} ", status),
                Style::default()
                    .fg(Color::White)
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )]);
            let status_bar =
                Paragraph::new(vec![status_line]).style(Style::default().bg(Color::DarkGray));
            frame.render_widget(status_bar, chunks[1]);

            // Position cursor
            let cursor_x = if cursor_row == 0 {
                chunks[0].x + prompt_width + cursor_col as u16
            } else {
                chunks[0].x + prompt_width + cursor_col as u16
            };
            let cursor_y = chunks[0].y + cursor_row as u16;
            frame.set_cursor_position((cursor_x, cursor_y));
        })
        .map_err(|e| miette::miette!("TUI render error: {}", e))?;

    Ok(())
}

/// Read a complete input line using the InputEditor.
///
/// Returns `Some(String)` on submission, `None` on EOF (Ctrl+D).
async fn read_input_line(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    editor: &mut InputEditor,
    history: &mut EditorHistory,
    prompt_manager: &PromptManager,
    ctx: &ComponentContext,
    _executor: &Executor,
) -> Result<Option<String>> {
    loop {
        // Poll for crossterm events
        if event::poll(std::time::Duration::from_millis(50))
            .map_err(|e| miette::miette!("event poll error: {}", e))?
        {
            let ev = event::read().map_err(|e| miette::miette!("event read error: {}", e))?;

            match ev {
                Event::Key(key) => {
                    // Handle Ctrl+D for EOF
                    if matches!(
                        (key.modifiers, key.code),
                        (KeyModifiers::CONTROL, KeyCode::Char('d'))
                    ) && editor.text().is_empty()
                    {
                        return Ok(None);
                    }

                    let action = editor.handle_event(key);

                    match action {
                        EditorAction::Submit(text) => {
                            return Ok(Some(text));
                        }
                        EditorAction::Cancel => {
                            editor.reset();
                            // Redraw with empty input
                            draw_prompt_and_input(terminal, prompt_manager, ctx, editor)?;
                        }
                        EditorAction::HistoryPrev => {
                            let current = editor.text();
                            if let Some(entry) = history.prev(&current) {
                                editor.set_text(entry);
                            }
                            draw_prompt_and_input(terminal, prompt_manager, ctx, editor)?;
                        }
                        EditorAction::HistoryNext => {
                            if let Some(entry) = history.next() {
                                editor.set_text(entry);
                            } else {
                                editor.set_text(history.saved_input());
                            }
                            draw_prompt_and_input(terminal, prompt_manager, ctx, editor)?;
                        }
                        EditorAction::Complete(prefix) => {
                            let builtins = vec![
                                "cd", "pwd", "echo", "export", "unset", "source", "exit",
                                "history", "jobs", "fg", "bg", "alias", "unalias", "set",
                                "shift", "test", "true", "false", "read", "printf", "type",
                                "hash", "ulimit", "umask", "wait", "trap", "return", "break",
                                "continue", "eval", "exec", "command", "builtin", "local",
                                "declare", "readonly", "getopts", "times", "llm", "path",
                            ]
                            .into_iter()
                            .map(String::from)
                            .collect();

                            let history_entries: Vec<String> =
                                history.entries().iter().cloned().collect();

                            let comp_ctx =
                                CompletionContext::from_env(builtins, history_entries);
                            let items = CompletionEngine::complete(&prefix, &comp_ctx);

                            if let Some(first) = items.first() {
                                // Replace the prefix with the completion
                                let line = editor.text();
                                let suffix = &line[..line.len() - prefix.len()];
                                let completed = format!("{}{}", suffix, first.text);
                                editor.set_text(&completed);
                            }

                            draw_prompt_and_input(terminal, prompt_manager, ctx, editor)?;
                        }
                        EditorAction::Execute(cmd) => {
                            // Handle vim :commands
                            match cmd.as_str() {
                                "q" | "quit" => {
                                    return Ok(Some("exit".to_string()));
                                }
                                _ => {}
                            }
                            draw_prompt_and_input(terminal, prompt_manager, ctx, editor)?;
                        }
                        _ => {
                            // Continue, ModeChanged, Search, etc. - just redraw
                            draw_prompt_and_input(terminal, prompt_manager, ctx, editor)?;
                        }
                    }
                }
                Event::Resize(_, _) => {
                    draw_prompt_and_input(terminal, prompt_manager, ctx, editor)?;
                }
                _ => {}
            }
        }
    }
}

/// Configure history from environment variables.
fn configure_history_from_env(executor: &mut Executor) {
    let get_env = |name: &str| std::env::var(name).ok();
    executor.history_mut().configure_from_env(get_env);

    let history_file = get_env("HISTFILE")
        .map(PathBuf::from)
        .or_else(|| get_env("HOME").map(|h| PathBuf::from(h).join(".colony_history")));

    if let Some(path) = history_file {
        executor.history_mut().set_file_path(path);
        executor.history_mut().load_from_file();
    }
}

fn save_history(executor: &Executor) {
    if let Err(e) = executor.history().save_to_file() {
        eprintln!("Warning: failed to save history: {}", e);
    }
}

/// Re-use the existing display_tuple_activity from the main repl module.
async fn display_tuple_activity(
    executor: &mut Executor,
    prompt_manager: &mut PromptManager,
    cwd_handle: &Option<Arc<RwLock<PathBuf>>>,
    agent: &mut Option<Agent>,
) {
    // Delegate to the shared implementation in super::repl
    super::repl::display_tuple_activity(executor, prompt_manager, cwd_handle, agent).await;
}
