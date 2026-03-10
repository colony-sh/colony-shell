//! TUI-based REPL using colony-tui's InputEditor and ratatui rendering.
//!
//! Full-screen TUI with layout containers: the prompt/output lives in a main
//! pane, and panels are separate panes managed by PanelHost. Command output is
//! captured via pipe/dup2 and rendered in a scrollback buffer within the main pane.

use std::io::{self, Read as IoRead, Stdout, Write as IoWrite};
use std::os::unix::io::{AsRawFd, FromRawFd, RawFd};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use colony_agent::{Agent, NaturalLanguageDetector};
use colony_exec::Executor;
use colony_panel::{
    Focus, FrameLine, InputEvent as PanelInputEvent, PanelHost, PanelMessage, StyleSpec,
};
use colony_protocol::{emit_prompt_busy, emit_prompt_ready, emit_prompt_state};
use colony_tui::editor::{
    CompletionContext, CompletionEngine, EditorAction, EditorHistory, EditorMode,
};
use colony_tui::{InputEditor, LegacyPromptRenderer as PromptRenderer};
use colony_ui_core::{ComponentContext, PromptManager};
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use miette::Result;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Terminal;
use tokio::sync::RwLock;

use crate::shell::execute_shell_command;
use crate::shell::execution::handle_llm_command;

/// Returns true if running inside colony-terminal (COLONY_TERMINAL env var set)
fn is_colony_terminal() -> bool {
    std::env::var("COLONY_TERMINAL").is_ok()
}

// --- Scrollback ---

/// A single line in the scrollback buffer.
#[derive(Clone)]
struct ScrollbackEntry {
    text: String,
    style: ScrollbackStyle,
}

#[derive(Clone, Copy)]
enum ScrollbackStyle {
    /// The prompt line that was shown before a command
    Prompt,
    /// Normal command output
    Output,
    /// Error output
    Error,
}

/// Scrollback buffer for past prompts and command output.
struct Scrollback {
    lines: Vec<ScrollbackEntry>,
    scroll_offset: usize,
    max_lines: usize,
}

impl Scrollback {
    fn new() -> Self {
        Self {
            lines: Vec::new(),
            scroll_offset: 0,
            max_lines: 10_000,
        }
    }

    fn add_prompt(&mut self, prompt_text: &str, command: &str) {
        self.lines.push(ScrollbackEntry {
            text: format!("{}{}", prompt_text, command),
            style: ScrollbackStyle::Prompt,
        });
    }

    fn add_output(&mut self, text: &str) {
        for line in text.lines() {
            self.lines.push(ScrollbackEntry {
                text: line.to_string(),
                style: ScrollbackStyle::Output,
            });
        }
        // Handle trailing newline edge case
        if text.ends_with('\n') && !text.is_empty() {
            // lines() doesn't yield an empty string for trailing newline
        }
        self.trim();
    }

    fn add_error(&mut self, text: &str) {
        for line in text.lines() {
            self.lines.push(ScrollbackEntry {
                text: line.to_string(),
                style: ScrollbackStyle::Error,
            });
        }
        self.trim();
    }

    fn trim(&mut self) {
        if self.lines.len() > self.max_lines {
            let excess = self.lines.len() - self.max_lines;
            self.lines.drain(..excess);
            self.scroll_offset = self.scroll_offset.saturating_sub(excess);
        }
    }

    fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
    }

    fn scroll_up(&mut self, amount: usize) {
        self.scroll_offset = self.scroll_offset.saturating_add(amount);
    }

    fn scroll_down(&mut self, amount: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
    }

    /// Get the visible lines for rendering, given available height.
    /// Returns lines from bottom up (most recent first), reversed for display.
    fn visible_lines(&self, height: usize) -> Vec<&ScrollbackEntry> {
        if self.lines.is_empty() || height == 0 {
            return Vec::new();
        }

        let total = self.lines.len();
        let end = total.saturating_sub(self.scroll_offset);
        let start = end.saturating_sub(height);

        self.lines[start..end].iter().collect()
    }
}

// --- Output Capture ---

/// Captures stdout/stderr during command execution by redirecting them to pipes.
/// A background thread continuously drains the pipe to prevent deadlocks when
/// commands produce more output than the pipe buffer can hold.
struct OutputCapture {
    saved_stdout: RawFd,
    saved_stderr: RawFd,
    reader_handle: std::thread::JoinHandle<String>,
}

impl OutputCapture {
    /// Begin capturing stdout and stderr. Spawns a reader thread immediately.
    fn begin() -> io::Result<Self> {
        // SAFETY: dup/dup2/pipe/close are safe to call with valid fds.
        // fd 1 (stdout) and fd 2 (stderr) are always valid in a running process.
        let saved_stdout = nix::unistd::dup(1)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        let saved_stderr = nix::unistd::dup(2)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        let (read_owned, write_owned) = nix::unistd::pipe()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        let read_fd = read_owned.as_raw_fd();
        let write_fd = write_owned.as_raw_fd();

        // Prevent OwnedFd from closing these on drop — we manage lifetimes manually
        std::mem::forget(read_owned);
        std::mem::forget(write_owned);

        // Redirect stdout and stderr to the pipe write end
        nix::unistd::dup2(write_fd, 1)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        nix::unistd::dup2(write_fd, 2)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        // Close the extra write fd (stdout/stderr ARE the write end now)
        let _ = nix::unistd::close(write_fd);

        // Spawn a thread to continuously drain the pipe
        let reader_handle = std::thread::spawn(move || {
            // SAFETY: read_fd is a valid fd from pipe() above
            let mut file = unsafe { std::fs::File::from_raw_fd(read_fd) };
            let mut output = String::new();
            let _ = file.read_to_string(&mut output);
            // file dropped here, closing read_fd
            output
        });

        Ok(Self {
            saved_stdout,
            saved_stderr,
            reader_handle,
        })
    }

    /// Finish capturing: restore stdout/stderr, collect output from reader thread.
    fn finish(self) -> io::Result<String> {
        // Flush to ensure all data reaches the pipe
        let _ = io::stdout().flush();
        let _ = io::stderr().flush();

        // Restore original stdout and stderr (this closes the pipe write end,
        // which signals EOF to the reader thread)
        nix::unistd::dup2(self.saved_stdout, 1)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        nix::unistd::dup2(self.saved_stderr, 2)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        let _ = nix::unistd::close(self.saved_stdout);
        let _ = nix::unistd::close(self.saved_stderr);

        // Wait for the reader thread to finish (it gets EOF when write end closes)
        let output = self
            .reader_handle
            .join()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "reader thread panicked"))?;

        Ok(output)
    }
}

// --- Helpers ---

/// Strip ANSI escape sequences from text for clean scrollback rendering.
fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip ESC [ ... final_byte sequences (CSI)
            if chars.peek() == Some(&'[') {
                chars.next(); // consume '['
                // consume until we hit a letter (0x40-0x7E)
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next.is_ascii_alphabetic() || next == '~' {
                        break;
                    }
                }
            }
            // Skip ESC ] ... BEL/ST sequences (OSC)
            else if chars.peek() == Some(&']') {
                chars.next();
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next == '\x07' {
                        break;
                    }
                    if next == '\x1b' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Build the prompt text string from the prompt manager (for scrollback entries).
fn prompt_text(prompt_manager: &PromptManager, ctx: &ComponentContext) -> String {
    let prompt_data = prompt_manager.render_data(ctx);
    let (left_line, _) = PromptRenderer::render(&prompt_data);
    left_line
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect()
}

/// Render a panel's frame lines inside a bordered area.
fn render_panel_frame(
    frame: &mut ratatui::Frame,
    area: Rect,
    lines: &[FrameLine],
    focused: bool,
) {
    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rendered_lines: Vec<Line> = lines
        .iter()
        .take(inner.height as usize)
        .map(|fl| {
            let indent = fl.indent.unwrap_or(0) as usize;
            let style = fl
                .style
                .as_deref()
                .map(|s| StyleSpec::parse(s).to_ratatui_style())
                .unwrap_or_default();

            let mut spans = Vec::new();
            if indent > 0 {
                spans.push(Span::raw(" ".repeat(indent)));
            }
            if let Some(ref icon) = fl.icon {
                spans.push(Span::styled(format!("{} ", icon), style));
            }
            spans.push(Span::styled(fl.text.clone(), style));
            Line::from(spans)
        })
        .collect();

    let paragraph = Paragraph::new(rendered_lines);
    frame.render_widget(paragraph, inner);
}

/// Convert a crossterm key event to a panel InputEvent.
fn crossterm_key_to_panel_event(key: crossterm::event::KeyEvent) -> PanelInputEvent {
    let key_str = match key.code {
        KeyCode::Enter => "Enter".to_string(),
        KeyCode::Backspace => "Backspace".to_string(),
        KeyCode::Left => "Left".to_string(),
        KeyCode::Right => "Right".to_string(),
        KeyCode::Up => "Up".to_string(),
        KeyCode::Down => "Down".to_string(),
        KeyCode::Tab => "Tab".to_string(),
        KeyCode::Esc => "Esc".to_string(),
        KeyCode::Char(c) => c.to_string(),
        other => format!("{other:?}"),
    };

    let mut modifiers = Vec::new();
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        modifiers.push("ctrl".to_string());
    }
    if key.modifiers.contains(KeyModifiers::SHIFT) {
        modifiers.push("shift".to_string());
    }
    if key.modifiers.contains(KeyModifiers::ALT) {
        modifiers.push("alt".to_string());
    }

    PanelInputEvent::Key {
        key: key_str,
        modifiers,
    }
}

/// Process any sidebar_command tuples from the tuple space and apply them to the panel host.
async fn process_sidebar_commands(executor: &Executor, panel_host: &mut PanelHost) {
    use colony_exec::sidebar::SidebarCommand;

    let tuple_space = executor.tuple_space();
    let pattern = colony_tuple::Tuple::pattern(&[
        colony_tuple::Value::Str("sidebar_command".to_string()),
        colony_tuple::Value::Wildcard,
    ]);

    let mut commands = Vec::new();
    while let Some(tuple) = tuple_space.inp(pattern.clone()) {
        let Some(colony_tuple::Value::Str(json)) = tuple.fields.get(1) else {
            continue;
        };
        let Ok(cmd) = serde_json::from_str::<SidebarCommand>(json) else {
            continue;
        };
        commands.push(cmd);
    }

    for cmd in commands {
        match cmd {
            SidebarCommand::Add {
                position,
                command,
                args,
                weight,
                min_size,
                max_size,
                outer,
            } => {
                let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
                let _ = panel_host
                    .add_panel(
                        &command,
                        &arg_refs,
                        position,
                        weight,
                        min_size.unwrap_or(15),
                        max_size.unwrap_or(40),
                        outer,
                    )
                    .await;
            }
            SidebarCommand::Reset => {
                panel_host.reset().await;
            }
            SidebarCommand::ResetPosition(pos) => {
                panel_host.reset_position(pos).await;
            }
        }
    }
}

// --- Main REPL ---

/// Run the TUI-based REPL loop.
///
/// Full-screen TUI: alternate screen with layout containers for panels + main pane.
/// Command output is captured and rendered in a scrollback buffer.
pub(crate) async fn run_repl_tui(
    mut agent: Option<Agent>,
    mut executor: Executor,
    cwd_handle: Option<Arc<RwLock<PathBuf>>>,
) -> Result<()> {
    // Enter alternate screen and raw mode
    enable_raw_mode().map_err(|e| miette::miette!("failed to enable raw mode: {}", e))?;
    execute!(io::stdout(), EnterAlternateScreen)
        .map_err(|e| miette::miette!("failed to enter alternate screen: {}", e))?;

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)
        .map_err(|e| miette::miette!("failed to create terminal: {}", e))?;

    let mut editor = InputEditor::new();
    let mut history = EditorHistory::new();
    let mut prompt_manager = PromptManager::default();
    let use_osc = is_colony_terminal();

    let mut panel_host = PanelHost::new();

    // Scrollback buffer
    let mut scrollback = Scrollback::new();

    // Load shell history into editor history
    for entry in executor.history().entries() {
        history.add(entry.to_string());
    }

    configure_history_from_env(&mut executor);

    let result = run_repl_loop(
        &mut terminal,
        &mut editor,
        &mut history,
        &mut prompt_manager,
        use_osc,
        &mut panel_host,
        &mut scrollback,
        &mut agent,
        &mut executor,
        &cwd_handle,
    )
    .await;

    panel_host.shutdown().await;

    // Always clean up terminal state
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen);
    let _ = terminal.show_cursor();

    result
}

/// The inner REPL loop, separated for clean terminal cleanup.
#[allow(clippy::too_many_arguments)]
async fn run_repl_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    editor: &mut InputEditor,
    history: &mut EditorHistory,
    prompt_manager: &mut PromptManager,
    use_osc: bool,
    panel_host: &mut PanelHost,
    scrollback: &mut Scrollback,
    agent: &mut Option<Agent>,
    executor: &mut Executor,
    cwd_handle: &Option<Arc<RwLock<PathBuf>>>,
) -> Result<()> {
    let mut status_message = String::new();

    loop {
        let model_name = agent.as_ref().map(|a| a.current_provider().1.to_string());
        let ctx = ComponentContext::from_executor(executor, model_name.clone());

        if use_osc {
            let prompt_data = prompt_manager.render_data(&ctx);
            let state =
                super::repl::build_prompt_state(&ctx, &prompt_data, model_name.as_deref());
            emit_prompt_state(state);
            emit_prompt_ready(None);
        }

        // Draw the full TUI
        draw_tui(
            terminal,
            prompt_manager,
            &ctx,
            editor,
            panel_host,
            scrollback,
            &status_message,
        )?;

        // Read input line
        let line = match read_input_line(
            terminal,
            editor,
            history,
            prompt_manager,
            &ctx,
            executor,
            panel_host,
            scrollback,
            &status_message,
        )
        .await?
        {
            Some(line) => line,
            None => {
                // Ctrl+D
                save_history(executor);
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
                    scrollback.add_output(&format!("  → {}", expanded));
                }
                expanded
            }
            Err(e) => {
                scrollback.add_error(&format!("colony: {}", e));
                editor.reset();
                continue;
            }
        };

        // Record the command in scrollback
        let pt = prompt_text(prompt_manager, &ctx);
        scrollback.add_prompt(&pt, &line);

        // Add to history
        history.add(line.clone());
        executor.history_mut().add(&line);

        // Reset editor for next input
        editor.reset();

        if line == "exit" || line == "quit" {
            scrollback.add_output("Goodbye!");
            save_history(executor);
            break;
        }

        // Execute the command with output capture
        execute_with_capture(
            &line,
            executor,
            agent,
            prompt_manager,
            scrollback,
            &mut status_message,
            use_osc,
            cwd_handle,
        )
        .await;

        // Process sidebar_command tuples emitted by builtins
        process_sidebar_commands(executor, panel_host).await;

        // Broadcast cwd change to panels if directory changed
        let new_cwd = std::env::current_dir().unwrap_or_default();
        if new_cwd != ctx.cwd {
            panel_host
                .broadcast_tuple(
                    "cwd_change",
                    &serde_json::json!({ "cwd": new_cwd.display().to_string() }),
                )
                .await;
        }

        scrollback.scroll_to_bottom();
    }

    Ok(())
}

/// Execute a command, capturing its output into the scrollback buffer.
/// Activity messages (e.g. "Directory: /path") are placed in `status_message`.
async fn execute_with_capture(
    line: &str,
    executor: &mut Executor,
    agent: &mut Option<Agent>,
    prompt_manager: &mut PromptManager,
    scrollback: &mut Scrollback,
    status_message: &mut String,
    use_osc: bool,
    cwd_handle: &Option<Arc<RwLock<PathBuf>>>,
) {
    // For interactive commands (editors, pagers), skip capture and run directly.
    // These need the real terminal.
    let first_word = line.split_whitespace().next().unwrap_or("");
    let interactive_commands = [
        "vi", "vim", "nvim", "nano", "emacs", "less", "more", "top", "htop", "man", "ssh",
        "tmux", "screen",
    ];

    if interactive_commands.contains(&first_word) {
        // Temporarily leave TUI for interactive command
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);

        if let Err(e) = execute_shell_command(line, executor).await {
            eprintln!("Error: {}", e);
        }

        // Return to TUI
        let _ = execute!(io::stdout(), EnterAlternateScreen);
        let _ = enable_raw_mode();
        prompt_manager.on_event("cmd_complete");
        return;
    }

    if line.starts_with("llm ") {
        // LLM commands: capture output
        let capture = match OutputCapture::begin() {
            Ok(c) => c,
            Err(e) => {
                scrollback.add_error(&format!("capture error: {}", e));
                return;
            }
        };

        handle_llm_command(line, agent).await;

        match capture.finish() {
            Ok(output) if !output.is_empty() => {
                scrollback.add_output(&strip_ansi(&output));
            }
            Err(e) => scrollback.add_error(&format!("capture error: {}", e)),
            _ => {}
        }
        return;
    }

    if let Some(ref mut ag) = agent {
        if NaturalLanguageDetector::is_natural_language(line) {
            let capture = match OutputCapture::begin() {
                Ok(c) => c,
                Err(e) => {
                    scrollback.add_error(&format!("capture error: {}", e));
                    return;
                }
            };

            match ag.process(line).await {
                Ok(_) => {}
                Err(e) => {
                    let _ = io::stderr().write_all(format!("Agent error: {}\n", e).as_bytes());
                }
            }

            match capture.finish() {
                Ok(output) if !output.is_empty() => {
                    scrollback.add_output(&strip_ansi(&output));
                }
                Err(e) => scrollback.add_error(&format!("capture error: {}", e)),
                _ => {}
            }
            return;
        }
    }

    if use_osc {
        emit_prompt_busy(line);
    }
    let cmd_start = Instant::now();

    // Capture command output
    let capture = match OutputCapture::begin() {
        Ok(c) => c,
        Err(e) => {
            scrollback.add_error(&format!("capture error: {}", e));
            return;
        }
    };

    let exec_result = execute_shell_command(line, executor).await;

    let output = match capture.finish() {
        Ok(o) => o,
        Err(e) => {
            scrollback.add_error(&format!("capture read error: {}", e));
            String::new()
        }
    };

    if !output.is_empty() {
        scrollback.add_output(&strip_ansi(&output));
    }

    match exec_result {
        Ok(()) => {
            let duration_ms = cmd_start.elapsed().as_millis() as u64;
            prompt_manager.on_event("cmd_complete");

            // Capture tuple activity output into the status bar
            if let Ok(activity_capture) = OutputCapture::begin() {
                display_tuple_activity(executor, prompt_manager, cwd_handle, agent).await;
                if let Ok(activity_output) = activity_capture.finish() {
                    let trimmed = strip_ansi(&activity_output).trim().to_string();
                    if !trimmed.is_empty() {
                        *status_message = trimmed;
                    }
                }
            } else {
                display_tuple_activity(executor, prompt_manager, cwd_handle, agent).await;
            }

            if use_osc {
                emit_prompt_ready(Some(duration_ms));
            }
        }
        Err(e) => {
            let duration_ms = cmd_start.elapsed().as_millis() as u64;
            scrollback.add_error(&format!("Error: {}", e));
            prompt_manager.on_event("cmd_complete");

            if use_osc {
                emit_prompt_ready(Some(duration_ms));
            }
        }
    }
}

// --- Drawing ---

/// Draw the complete TUI: panels + scrollback + prompt + status bar.
fn draw_tui(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    prompt_manager: &PromptManager,
    ctx: &ComponentContext,
    editor: &InputEditor,
    panel_host: &PanelHost,
    scrollback: &Scrollback,
    status_message: &str,
) -> Result<()> {
    let prompt_data = prompt_manager.render_data(ctx);
    let (left_line, _) = PromptRenderer::render(&prompt_data);
    let cursor_row = editor.buffer.cursor_row();
    let cursor_col = editor.buffer.cursor_col();
    let prompt_spans = left_line.spans.clone();
    let prompt_width: u16 = prompt_spans
        .iter()
        .map(|s| s.content.len())
        .sum::<usize>() as u16;

    let focus = panel_host.focus;

    terminal
        .draw(|frame| {
            let area = frame.area();

            let layout = panel_host.resolve_layout(area);

            // Render panels
            for (id, panel_area) in &layout.panels {
                let lines = panel_host.current_frame(*id);
                let panel_focused = focus == Focus::Panel(*id);
                render_panel_frame(frame, *panel_area, lines, panel_focused);
            }

            let main_area = layout.main;

            // Main vertical layout: scrollback | prompt+input | status bar
            let input_height = (editor.buffer.lines().len() as u16).max(1);
            let main_chunks = ratatui::layout::Layout::default()
                .direction(ratatui::layout::Direction::Vertical)
                .constraints([
                    ratatui::layout::Constraint::Min(1),
                    ratatui::layout::Constraint::Length(input_height),
                    ratatui::layout::Constraint::Length(1),
                ])
                .split(main_area);

            // Render scrollback
            render_scrollback(frame, main_chunks[0], scrollback);

            // Render prompt + input
            let mut input_lines: Vec<Line<'_>> = Vec::new();

            let first_input_line = editor.buffer.lines().first().map_or("", |s| s.as_str());
            let mut first_spans = prompt_spans.clone();
            first_spans.push(Span::raw(first_input_line.to_string()));
            input_lines.push(Line::from(first_spans));

            for line in editor.buffer.lines().iter().skip(1) {
                let continuation = format!(
                    "{:>width$}{}",
                    "> ",
                    line,
                    width = prompt_width as usize
                );
                input_lines.push(Line::from(continuation));
            }

            let input_paragraph = Paragraph::new(input_lines);
            frame.render_widget(input_paragraph, main_chunks[1]);

            // Render status bar with activity message
            let bar_text = if status_message.is_empty() {
                String::new()
            } else {
                format!(" {} ", status_message)
            };
            let status_line = Line::from(vec![Span::styled(
                bar_text,
                Style::default()
                    .fg(Color::Yellow)
                    .bg(Color::DarkGray),
            )]);
            let status_bar =
                Paragraph::new(vec![status_line]).style(Style::default().bg(Color::DarkGray));
            frame.render_widget(status_bar, main_chunks[2]);

            // Position cursor (only when main is focused)
            if focus == Focus::Main {
                let cursor_x = main_chunks[1].x + prompt_width + cursor_col as u16;
                let cursor_y = main_chunks[1].y + cursor_row as u16;
                frame.set_cursor_position((cursor_x, cursor_y));
            }
        })
        .map_err(|e| miette::miette!("TUI render error: {}", e))?;

    Ok(())
}

/// Render the scrollback buffer into the given area.
fn render_scrollback(frame: &mut ratatui::Frame, area: Rect, scrollback: &Scrollback) {
    if area.height == 0 {
        return;
    }

    let visible = scrollback.visible_lines(area.height as usize);
    let lines: Vec<Line> = visible
        .iter()
        .map(|entry| {
            let style = match entry.style {
                ScrollbackStyle::Prompt => Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ScrollbackStyle::Output => Style::default().fg(Color::White),
                ScrollbackStyle::Error => Style::default().fg(Color::Red),
            };
            Line::from(Span::styled(entry.text.clone(), style))
        })
        .collect();

    // Pad with empty lines if scrollback doesn't fill the area
    let mut padded = Vec::new();
    let pad_count = (area.height as usize).saturating_sub(lines.len());
    for _ in 0..pad_count {
        padded.push(Line::from(""));
    }
    padded.extend(lines);

    let paragraph = Paragraph::new(padded);
    frame.render_widget(paragraph, area);
}

/// Redraw helper that combines layout resolution and draw call.
fn redraw(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    prompt_manager: &PromptManager,
    ctx: &ComponentContext,
    editor: &InputEditor,
    panel_host: &PanelHost,
    scrollback: &Scrollback,
    status_message: &str,
) -> Result<()> {
    draw_tui(
        terminal,
        prompt_manager,
        ctx,
        editor,
        panel_host,
        scrollback,
        status_message,
    )
}

// --- Input ---

/// Read a complete input line using the InputEditor.
///
/// Returns `Some(String)` on submission, `None` on EOF (Ctrl+D).
#[allow(clippy::too_many_arguments)]
async fn read_input_line(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    editor: &mut InputEditor,
    history: &mut EditorHistory,
    prompt_manager: &PromptManager,
    ctx: &ComponentContext,
    _executor: &Executor,
    panel_host: &mut PanelHost,
    scrollback: &mut Scrollback,
    status_message: &str,
) -> Result<Option<String>> {
    loop {
        // Poll panels for new frames
        let messages = panel_host.poll_panels().await;
        for (id, msg) in &messages {
            if let PanelMessage::Emit(ref emit) = msg {
                panel_host
                    .broadcast_tuple(&emit.key, &emit.value)
                    .await;
            }
            let _ = (id, msg); // suppress unused warnings for non-emit messages
        }

        if event::poll(std::time::Duration::from_millis(50))
            .map_err(|e| miette::miette!("event poll error: {}", e))?
        {
            let ev = event::read().map_err(|e| miette::miette!("event read error: {}", e))?;

            match ev {
                Event::Key(key)
                    if key.kind == crossterm::event::KeyEventKind::Press =>
                {
                    // Tab: cycle panel focus when panels exist and prompt empty
                    if key.code == KeyCode::Tab && panel_host.has_panels() {
                        let prompt_empty = editor.text().is_empty();
                        if prompt_empty || !matches!(panel_host.focus, Focus::Main) {
                            panel_host.cycle_focus();
                            redraw(
                                terminal, prompt_manager, ctx, editor,
                                panel_host, scrollback, status_message,
                            )?;
                            continue;
                        }
                    }

                    // Scrollback navigation: Shift+Up/Down or Ctrl+Up/Down
                    if matches!(
                        (key.modifiers, key.code),
                        (KeyModifiers::SHIFT, KeyCode::Up)
                            | (KeyModifiers::CONTROL, KeyCode::Up)
                    ) {
                        scrollback.scroll_up(3);
                        redraw(
                            terminal, prompt_manager, ctx, editor,
                            panel_host, scrollback, status_message,
                        )?;
                        continue;
                    }
                    if matches!(
                        (key.modifiers, key.code),
                        (KeyModifiers::SHIFT, KeyCode::Down)
                            | (KeyModifiers::CONTROL, KeyCode::Down)
                    ) {
                        scrollback.scroll_down(3);
                        redraw(
                            terminal, prompt_manager, ctx, editor,
                            panel_host, scrollback, status_message,
                        )?;
                        continue;
                    }

                    // Panel-focused key handling: forward keys to the focused panel
                    if let Focus::Panel(_) = panel_host.focus {
                        // Esc returns focus to main
                        if key.code == KeyCode::Esc {
                            panel_host.focus = Focus::Main;
                            redraw(
                                terminal, prompt_manager, ctx, editor,
                                panel_host, scrollback, status_message,
                            )?;
                            continue;
                        }

                        let panel_event = crossterm_key_to_panel_event(key);
                        let _ = panel_host.send_to_focused(&panel_event).await;
                        redraw(
                            terminal, prompt_manager, ctx, editor,
                            panel_host, scrollback, status_message,
                        )?;
                        continue;
                    }

                    // --- Main-focused key handling ---

                    // Ctrl+D for EOF
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
                        }
                        EditorAction::HistoryPrev => {
                            let current = editor.text();
                            if let Some(entry) = history.prev(&current) {
                                editor.set_text(entry);
                            }
                        }
                        EditorAction::HistoryNext => {
                            if let Some(entry) = history.next() {
                                editor.set_text(entry);
                            } else {
                                editor.set_text(history.saved_input());
                            }
                        }
                        EditorAction::Complete(prefix) => {
                            let builtins = vec![
                                "cd", "pwd", "echo", "export", "unset", "source", "exit",
                                "history", "jobs", "fg", "bg", "alias", "unalias", "set",
                                "shift", "test", "true", "false", "read", "printf", "type",
                                "hash", "ulimit", "umask", "wait", "trap", "return", "break",
                                "continue", "eval", "exec", "command", "builtin", "local",
                                "declare", "readonly", "getopts", "times", "llm", "path",
                                "sidebar",
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
                                let line = editor.text();
                                let suffix = &line[..line.len() - prefix.len()];
                                let completed = format!("{}{}", suffix, first.text);
                                editor.set_text(&completed);
                            }
                        }
                        EditorAction::Execute(cmd) => {
                            if matches!(cmd.as_str(), "q" | "quit") {
                                return Ok(Some("exit".to_string()));
                            }
                        }
                        _ => {}
                    }

                    // Force insert mode — the TUI REPL doesn't use vim normal mode
                    if editor.mode() != EditorMode::Insert {
                        editor.set_mode(EditorMode::Insert);
                    }

                    redraw(
                        terminal, prompt_manager, ctx, editor,
                        panel_host, scrollback, status_message,
                    )?;
                }
                Event::Resize(_, _) => {
                    // Notify panels of new dimensions
                    let size = terminal.size().unwrap_or_default();
                    let area = Rect::new(0, 0, size.width, size.height);
                    let layout = panel_host.resolve_layout(area);
                    panel_host.notify_resize(&layout).await;

                    redraw(
                        terminal, prompt_manager, ctx, editor,
                        panel_host, scrollback, status_message,
                    )?;
                }
                _ => {}
            }
        } else if !messages.is_empty() {
            // Panels produced new frames, redraw
            redraw(
                terminal, prompt_manager, ctx, editor,
                panel_host, scrollback, status_message,
            )?;
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
    super::repl::display_tuple_activity(executor, prompt_manager, cwd_handle, agent).await;
}
