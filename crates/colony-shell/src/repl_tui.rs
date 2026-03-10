//! TUI-based REPL using colony-tui's InputEditor and ratatui rendering.
//!
//! Full-screen TUI with layout containers: the prompt/output lives in a main
//! pane, and the sidebar is a separate pane. Command output is captured via
//! pipe/dup2 and rendered in a scrollback buffer within the main pane.

use std::io::{self, Read as IoRead, Stdout, Write as IoWrite};
use std::os::unix::io::{AsRawFd, FromRawFd, RawFd};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use colony_agent::{Agent, NaturalLanguageDetector};
use colony_exec::Executor;
use colony_protocol::{emit_prompt_busy, emit_prompt_ready, emit_prompt_state};
use colony_tui::editor::{CompletionContext, CompletionEngine, EditorAction, EditorHistory};
use colony_tui::{InputEditor, LegacyPromptRenderer as PromptRenderer, SidebarRenderer};
use colony_ui_core::{
    ComponentContext, FileTreePanel, PromptManager, SidebarData, SidebarManager, SidebarPosition,
};
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::execute;
use miette::Result;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
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

// --- Sidebar types ---

/// Focus area in the TUI REPL
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Prompt,
    Sidebar,
}

/// Context menu for file actions
#[derive(Debug, Clone)]
struct ContextMenu {
    node_path: PathBuf,
    selected: usize, // 0=insert, 1=preview, 2=open, 3=copy
}

impl ContextMenu {
    fn new(path: PathBuf) -> Self {
        Self {
            node_path: path,
            selected: 0,
        }
    }

    fn labels() -> &'static [&'static str] {
        &[
            "i  Insert path",
            "p  Preview",
            "o  Open $EDITOR",
            "y  Copy path",
        ]
    }

    fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    fn move_down(&mut self) {
        if self.selected < 3 {
            self.selected += 1;
        }
    }
}

/// Sidebar UI state (cursor, context menu, visibility)
struct SidebarState {
    visible: bool,
    cursor_index: usize,
    context_menu: Option<ContextMenu>,
    #[allow(dead_code)]
    show_hidden: bool,
    node_count: usize,
    /// Tick counter for horizontal scroll animation of truncated highlighted items.
    /// Resets to 0 when cursor moves; increments each render cycle.
    scroll_tick: usize,
}

impl SidebarState {
    fn new() -> Self {
        Self {
            visible: false,
            cursor_index: 0,
            context_menu: None,
            show_hidden: false,
            node_count: 0,
            scroll_tick: 0,
        }
    }

    /// Compute the horizontal scroll offset from the current tick.
    /// Starts scrolling after a short delay, then advances 1 char per tick.
    fn scroll_offset(&self) -> usize {
        // Wait ~500ms before scrolling (10 ticks at 50ms poll interval)
        let delay_ticks = 10;
        self.scroll_tick.saturating_sub(delay_ticks)
    }

    fn clamp_cursor(&mut self) {
        if self.node_count == 0 {
            self.cursor_index = 0;
        } else if self.cursor_index >= self.node_count {
            self.cursor_index = self.node_count - 1;
        }
    }

    fn cursor_up(&mut self) {
        self.cursor_index = self.cursor_index.saturating_sub(1);
        self.scroll_tick = 0;
    }

    fn cursor_down(&mut self) {
        if self.node_count > 0 && self.cursor_index < self.node_count - 1 {
            self.cursor_index += 1;
        }
        self.scroll_tick = 0;
    }
}

/// Result of a sidebar key press that needs action from the caller
enum SidebarAction {
    None,
    InsertPath(PathBuf),
    Preview(PathBuf),
    OpenEditor(PathBuf),
    CopyPath(PathBuf),
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

/// Render sidebar data from the manager, updating node_count and clamping cursor.
fn render_sidebar_data(
    sidebars: &mut SidebarManager,
    sidebar_state: &mut SidebarState,
    ctx: &ComponentContext,
) -> Option<SidebarData> {
    if sidebar_state.visible {
        sidebars.invalidate_all();
        let data = sidebars.render(SidebarPosition::Left, ctx);
        sidebar_state.node_count = data
            .panels
            .first()
            .map_or(0, |p| p.output.lines.len());
        sidebar_state.clamp_cursor();
        Some(data)
    } else {
        None
    }
}

/// Get the file path for the node at the current cursor position.
fn get_node_path_at_cursor(
    sidebars: &mut SidebarManager,
    sidebar_state: &SidebarState,
    ctx: &ComponentContext,
) -> Option<PathBuf> {
    let data = sidebars.render(SidebarPosition::Left, ctx);
    let panel = data.panels.first()?;
    let line = panel.output.lines.get(sidebar_state.cursor_index)?;
    let interactive = line.interactive.as_ref()?;
    let path_str = interactive.hover.as_ref().unwrap_or(&interactive.click);
    let path_str = path_str.strip_prefix("open:").unwrap_or(path_str);
    Some(PathBuf::from(path_str))
}

/// Check if the node at cursor is an expanded directory.
fn is_node_expanded_at_cursor(
    sidebars: &mut SidebarManager,
    sidebar_state: &SidebarState,
    ctx: &ComponentContext,
) -> bool {
    let data = sidebars.render(SidebarPosition::Left, ctx);
    let Some(panel) = data.panels.first() else {
        return false;
    };
    let Some(line) = panel.output.lines.get(sidebar_state.cursor_index) else {
        return false;
    };
    line.expandable && line.expanded
}

/// Shell-escape a path for safe embedding in commands.
fn shell_escape(path: &Path) -> String {
    let s = path.display().to_string();
    if s.contains(' ') || s.contains('\'') || s.contains('"') {
        format!("'{}'", s.replace('\'', "'\\''"))
    } else {
        s
    }
}

/// Handle keyboard input when sidebar is focused.
fn handle_sidebar_key(
    key: crossterm::event::KeyEvent,
    sidebar_state: &mut SidebarState,
    sidebars: &mut SidebarManager,
    ctx: &ComponentContext,
) -> SidebarAction {
    // If context menu is open, handle menu keys
    if let Some(ref mut menu) = sidebar_state.context_menu {
        return match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                menu.move_up();
                SidebarAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                menu.move_down();
                SidebarAction::None
            }
            KeyCode::Enter => {
                let path = menu.node_path.clone();
                let action = match menu.selected {
                    0 => SidebarAction::InsertPath(path),
                    1 => SidebarAction::Preview(path),
                    2 => SidebarAction::OpenEditor(path),
                    3 => SidebarAction::CopyPath(path),
                    _ => SidebarAction::None,
                };
                sidebar_state.context_menu = None;
                action
            }
            KeyCode::Char('i') => {
                let path = menu.node_path.clone();
                sidebar_state.context_menu = None;
                SidebarAction::InsertPath(path)
            }
            KeyCode::Char('p') => {
                let path = menu.node_path.clone();
                sidebar_state.context_menu = None;
                SidebarAction::Preview(path)
            }
            KeyCode::Char('o') => {
                let path = menu.node_path.clone();
                sidebar_state.context_menu = None;
                SidebarAction::OpenEditor(path)
            }
            KeyCode::Char('y') => {
                let path = menu.node_path.clone();
                sidebar_state.context_menu = None;
                SidebarAction::CopyPath(path)
            }
            KeyCode::Esc => {
                sidebar_state.context_menu = None;
                SidebarAction::None
            }
            _ => SidebarAction::None,
        };
    }

    // Normal sidebar navigation
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            sidebar_state.cursor_up();
            SidebarAction::None
        }
        KeyCode::Down | KeyCode::Char('j') => {
            sidebar_state.cursor_down();
            SidebarAction::None
        }
        KeyCode::Enter => {
            if let Some(path) = get_node_path_at_cursor(sidebars, sidebar_state, ctx) {
                if path.is_dir() {
                    sidebars.handle_action(
                        "file_tree",
                        &format!("toggle:{}", path.display()),
                        ctx,
                    );
                    sidebars.invalidate_all();
                } else {
                    sidebar_state.context_menu = Some(ContextMenu::new(path));
                }
            }
            SidebarAction::None
        }
        KeyCode::Right | KeyCode::Char('l') => {
            if let Some(path) = get_node_path_at_cursor(sidebars, sidebar_state, ctx) {
                if path.is_dir() {
                    if is_node_expanded_at_cursor(sidebars, sidebar_state, ctx) {
                        sidebar_state.cursor_down();
                    } else {
                        sidebars.handle_action(
                            "file_tree",
                            &format!("toggle:{}", path.display()),
                            ctx,
                        );
                        sidebars.invalidate_all();
                    }
                }
            }
            SidebarAction::None
        }
        KeyCode::Left | KeyCode::Char('h') => {
            if let Some(path) = get_node_path_at_cursor(sidebars, sidebar_state, ctx) {
                let expanded = is_node_expanded_at_cursor(sidebars, sidebar_state, ctx);
                if path.is_dir() && expanded {
                    sidebars.handle_action(
                        "file_tree",
                        &format!("toggle:{}", path.display()),
                        ctx,
                    );
                    sidebars.invalidate_all();
                } else if sidebar_state.cursor_index > 0 {
                    sidebar_state.cursor_up();
                }
            }
            SidebarAction::None
        }
        KeyCode::Char('i') => {
            if let Some(path) = get_node_path_at_cursor(sidebars, sidebar_state, ctx) {
                return SidebarAction::InsertPath(path);
            }
            SidebarAction::None
        }
        KeyCode::Char('o') => {
            if let Some(path) = get_node_path_at_cursor(sidebars, sidebar_state, ctx) {
                if !path.is_dir() {
                    return SidebarAction::OpenEditor(path);
                }
            }
            SidebarAction::None
        }
        KeyCode::Char('p') => {
            if let Some(path) = get_node_path_at_cursor(sidebars, sidebar_state, ctx) {
                if !path.is_dir() {
                    return SidebarAction::Preview(path);
                }
            }
            SidebarAction::None
        }
        KeyCode::Char('.') => {
            sidebar_state.show_hidden = !sidebar_state.show_hidden;
            sidebars.invalidate_all();
            SidebarAction::None
        }
        _ => SidebarAction::None,
    }
}

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

// --- Main REPL ---

/// Run the TUI-based REPL loop.
///
/// Full-screen TUI: alternate screen with layout containers for sidebar + main pane.
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

    // Sidebar state
    let mut sidebars = SidebarManager::new();
    sidebars.register(Arc::new(FileTreePanel::new()));
    let mut sidebar_state = SidebarState::new();
    let mut focus = Focus::Prompt;

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
        &mut sidebars,
        &mut sidebar_state,
        &mut focus,
        &mut scrollback,
        &mut agent,
        &mut executor,
        &cwd_handle,
    )
    .await;

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
    sidebars: &mut SidebarManager,
    sidebar_state: &mut SidebarState,
    focus: &mut Focus,
    scrollback: &mut Scrollback,
    agent: &mut Option<Agent>,
    executor: &mut Executor,
    cwd_handle: &Option<Arc<RwLock<PathBuf>>>,
) -> Result<()> {
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
            sidebars,
            sidebar_state,
            *focus,
            scrollback,
        )?;

        // Read input line
        let line = match read_input_line(
            terminal,
            editor,
            history,
            prompt_manager,
            &ctx,
            executor,
            sidebars,
            sidebar_state,
            focus,
            scrollback,
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
            use_osc,
            cwd_handle,
        )
        .await;

        // Reset sidebar cursor if cwd changed
        let new_cwd = std::env::current_dir().unwrap_or_default();
        if new_cwd != ctx.cwd {
            sidebar_state.cursor_index = 0;
            sidebars.invalidate_all();
        }

        scrollback.scroll_to_bottom();
    }

    Ok(())
}

/// Execute a command, capturing its output into the scrollback buffer.
async fn execute_with_capture(
    line: &str,
    executor: &mut Executor,
    agent: &mut Option<Agent>,
    prompt_manager: &mut PromptManager,
    scrollback: &mut Scrollback,
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
            display_tuple_activity(executor, prompt_manager, cwd_handle, agent).await;

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

/// Draw the complete TUI: sidebar + scrollback + prompt + status bar.
fn draw_tui(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    prompt_manager: &PromptManager,
    ctx: &ComponentContext,
    editor: &InputEditor,
    sidebars: &mut SidebarManager,
    sidebar_state: &mut SidebarState,
    focus: Focus,
    scrollback: &Scrollback,
) -> Result<()> {
    let sidebar_data = render_sidebar_data(sidebars, sidebar_state, ctx);

    let prompt_data = prompt_manager.render_data(ctx);
    let (left_line, _) = PromptRenderer::render(&prompt_data);
    let cursor_row = editor.buffer.cursor_row();
    let cursor_col = editor.buffer.cursor_col();
    let status = editor.status_line();
    let prompt_spans = left_line.spans.clone();
    let prompt_width: u16 = prompt_spans
        .iter()
        .map(|s| s.content.len())
        .sum::<usize>() as u16;

    terminal
        .draw(|frame| {
            let area = frame.area();

            // Horizontal split: sidebar (if visible) | main
            let (sidebar_area, main_area) = if let Some(ref sb_data) = sidebar_data {
                if sb_data.visible && area.width > sb_data.width + 10 {
                    let chunks = Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints([
                            Constraint::Length(sb_data.width),
                            Constraint::Min(10),
                        ])
                        .split(area);
                    (Some(chunks[0]), chunks[1])
                } else {
                    (None, area)
                }
            } else {
                (None, area)
            };

            // Render sidebar
            if let (Some(sb_area), Some(ref sb_data)) = (sidebar_area, &sidebar_data) {
                let selected = if focus == Focus::Sidebar {
                    Some(sidebar_state.cursor_index)
                } else {
                    None
                };
                SidebarRenderer::render(frame, sb_area, sb_data, focus == Focus::Sidebar, selected, sidebar_state.scroll_offset());

                // Context menu overlay
                if let Some(ref menu) = sidebar_state.context_menu {
                    render_context_menu(frame, sb_area, sidebar_state.cursor_index, menu);
                }
            }

            // Main vertical layout: scrollback | prompt+input | status bar
            let input_height = (editor.buffer.lines().len() as u16).max(1);
            let main_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(1),            // scrollback area
                    Constraint::Length(input_height), // prompt + input
                    Constraint::Length(1),          // status line
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
            frame.render_widget(status_bar, main_chunks[2]);

            // Position cursor (only when prompt is focused)
            if focus == Focus::Prompt {
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

/// Render the context menu as a floating overlay on the sidebar.
fn render_context_menu(
    frame: &mut ratatui::Frame,
    sb_area: Rect,
    cursor_index: usize,
    menu: &ContextMenu,
) {
    let menu_width = 20u16.min(sb_area.width.saturating_sub(2));
    let menu_height = 6u16;
    let menu_x = sb_area.x + 1;
    let menu_y = (sb_area.y + cursor_index as u16 + 2)
        .min(sb_area.y + sb_area.height.saturating_sub(menu_height));

    let menu_area = Rect {
        x: menu_x,
        y: menu_y,
        width: menu_width,
        height: menu_height,
    };

    let menu_lines: Vec<Line> = ContextMenu::labels()
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let style = if i == menu.selected {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else {
                Style::default().fg(Color::White)
            };
            Line::from(Span::styled(format!(" {} ", label), style))
        })
        .collect();

    let menu_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Rgb(30, 30, 50)));
    let inner = menu_block.inner(menu_area);
    frame.render_widget(menu_block, menu_area);
    frame.render_widget(Paragraph::new(menu_lines), inner);
}

/// Redraw helper that combines sidebar data rendering and draw call.
fn redraw(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    prompt_manager: &PromptManager,
    ctx: &ComponentContext,
    editor: &InputEditor,
    sidebars: &mut SidebarManager,
    sidebar_state: &mut SidebarState,
    focus: Focus,
    scrollback: &Scrollback,
) -> Result<()> {
    draw_tui(
        terminal,
        prompt_manager,
        ctx,
        editor,
        sidebars,
        sidebar_state,
        focus,
        scrollback,
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
    sidebars: &mut SidebarManager,
    sidebar_state: &mut SidebarState,
    focus: &mut Focus,
    scrollback: &mut Scrollback,
) -> Result<Option<String>> {
    loop {
        if event::poll(std::time::Duration::from_millis(50))
            .map_err(|e| miette::miette!("event poll error: {}", e))?
        {
            let ev = event::read().map_err(|e| miette::miette!("event read error: {}", e))?;

            match ev {
                Event::Key(key)
                    if key.kind == crossterm::event::KeyEventKind::Press =>
                {
                    // Global: Ctrl+B toggles sidebar
                    if matches!(
                        (key.modifiers, key.code),
                        (KeyModifiers::CONTROL, KeyCode::Char('b'))
                            | (KeyModifiers::NONE, KeyCode::Char('\x02'))
                    ) {
                        sidebar_state.visible = !sidebar_state.visible;
                        sidebars.set_visible(SidebarPosition::Left, sidebar_state.visible);
                        if !sidebar_state.visible {
                            *focus = Focus::Prompt;
                            sidebar_state.context_menu = None;
                        }
                        redraw(
                            terminal, prompt_manager, ctx, editor,
                            sidebars, sidebar_state, *focus, scrollback,
                        )?;
                        continue;
                    }

                    // Tab switches focus when sidebar is visible
                    if key.code == KeyCode::Tab && sidebar_state.visible {
                        *focus = match focus {
                            Focus::Prompt => Focus::Sidebar,
                            Focus::Sidebar => {
                                sidebar_state.context_menu = None;
                                Focus::Prompt
                            }
                        };
                        redraw(
                            terminal, prompt_manager, ctx, editor,
                            sidebars, sidebar_state, *focus, scrollback,
                        )?;
                        continue;
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
                            sidebars, sidebar_state, *focus, scrollback,
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
                            sidebars, sidebar_state, *focus, scrollback,
                        )?;
                        continue;
                    }

                    // Sidebar-focused key handling
                    if *focus == Focus::Sidebar {
                        let action = handle_sidebar_key(key, sidebar_state, sidebars, ctx);
                        match action {
                            SidebarAction::InsertPath(path) => {
                                let relative =
                                    path.strip_prefix(&ctx.cwd).unwrap_or(&path);
                                let path_str = relative.display().to_string();
                                let current = editor.text();
                                editor.set_text(&format!("{}{}", current, path_str));
                                *focus = Focus::Prompt;
                            }
                            SidebarAction::Preview(path) => {
                                return Ok(Some(format!(
                                    "head -50 {}",
                                    shell_escape(&path)
                                )));
                            }
                            SidebarAction::OpenEditor(path) => {
                                let editor_cmd = std::env::var("EDITOR")
                                    .unwrap_or_else(|_| "vi".to_string());
                                return Ok(Some(format!(
                                    "{} {}",
                                    editor_cmd,
                                    shell_escape(&path)
                                )));
                            }
                            SidebarAction::CopyPath(path) => {
                                let relative =
                                    path.strip_prefix(&ctx.cwd).unwrap_or(&path);
                                let path_str = relative.display().to_string();
                                if cfg!(target_os = "macos") {
                                    let _ = std::process::Command::new("pbcopy")
                                        .stdin(std::process::Stdio::piped())
                                        .spawn()
                                        .and_then(|mut child| {
                                            if let Some(ref mut stdin) = child.stdin {
                                                stdin.write_all(path_str.as_bytes())?;
                                            }
                                            child.wait()
                                        });
                                }
                            }
                            SidebarAction::None => {}
                        }
                        redraw(
                            terminal, prompt_manager, ctx, editor,
                            sidebars, sidebar_state, *focus, scrollback,
                        )?;
                        continue;
                    }

                    // --- Prompt-focused key handling ---

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

                    redraw(
                        terminal, prompt_manager, ctx, editor,
                        sidebars, sidebar_state, *focus, scrollback,
                    )?;
                }
                Event::Resize(_, _) => {
                    redraw(
                        terminal, prompt_manager, ctx, editor,
                        sidebars, sidebar_state, *focus, scrollback,
                    )?;
                }
                _ => {}
            }
        } else if *focus == Focus::Sidebar && sidebar_state.visible {
            // No event: advance scroll animation for truncated highlighted items
            sidebar_state.scroll_tick += 1;
            redraw(
                terminal, prompt_manager, ctx, editor,
                sidebars, sidebar_state, *focus, scrollback,
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
