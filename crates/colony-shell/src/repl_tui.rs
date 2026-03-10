//! TUI-based REPL using colony-tui's InputEditor and ratatui rendering.
//!
//! This module provides an alternative to the rustyline-based REPL that uses
//! vim-mode editing (via `InputEditor`), ratatui-based prompt rendering, and
//! the same command execution pipeline as the standard REPL.

use std::io::{self, Stdout};
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
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use crossterm::{cursor, execute};
use miette::Result;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::{Terminal, TerminalOptions, Viewport};
use tokio::sync::RwLock;

use crate::shell::execute_shell_command;
use crate::shell::execution::handle_llm_command;

/// Returns true if running inside colony-terminal (COLONY_TERMINAL env var set)
fn is_colony_terminal() -> bool {
    std::env::var("COLONY_TERMINAL").is_ok()
}

// --- Sidebar types (Task 2) ---

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
    node_count: usize, // cached from last render
}

impl SidebarState {
    fn new() -> Self {
        Self {
            visible: false,
            cursor_index: 0,
            context_menu: None,
            show_hidden: false,
            node_count: 0,
        }
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
    }

    fn cursor_down(&mut self) {
        if self.node_count > 0 && self.cursor_index < self.node_count - 1 {
            self.cursor_index += 1;
        }
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
                        // Expanded dir: move cursor to first child
                        sidebar_state.cursor_down();
                    } else {
                        // Collapsed dir: expand it
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
                    // Expanded dir: collapse it
                    sidebars.handle_action(
                        "file_tree",
                        &format!("toggle:{}", path.display()),
                        ctx,
                    );
                    sidebars.invalidate_all();
                } else if sidebar_state.cursor_index > 0 {
                    // Collapsed dir or file: move cursor up (toward parent)
                    sidebar_state.cursor_up();
                }
            }
            SidebarAction::None
        }
        KeyCode::Char('i') => {
            if let Some(path) = get_node_path_at_cursor(sidebars, sidebar_state, ctx) {
                if !path.is_dir() {
                    return SidebarAction::InsertPath(path);
                }
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
            // TODO: pass show_hidden to FileTreePanel config
            sidebars.invalidate_all();
            SidebarAction::None
        }
        _ => SidebarAction::None,
    }
}

/// Create an inline terminal that occupies `height` lines at the current cursor position.
fn create_inline_terminal(height: u16) -> io::Result<Terminal<CrosstermBackend<Stdout>>> {
    let backend = CrosstermBackend::new(io::stdout());
    Terminal::with_options(
        backend,
        TerminalOptions {
            viewport: Viewport::Inline(height),
        },
    )
}

/// Compute how many lines the TUI needs: prompt(1) + status(1) + sidebar height if visible.
fn tui_height(sidebar_visible: bool) -> u16 {
    if sidebar_visible {
        // Use most of the terminal for the sidebar view
        let (_, term_height) = crossterm::terminal::size().unwrap_or((80, 24));
        // Leave a few lines of scroll-back visible above
        term_height.saturating_sub(1)
    } else {
        // Just prompt + status bar
        2
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

    // Sidebar state (Task 3)
    let mut sidebars = SidebarManager::new();
    sidebars.register(Arc::new(FileTreePanel::new()));
    let mut sidebar_state = SidebarState::new();
    let mut focus = Focus::Prompt;

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
            let state =
                super::repl::build_prompt_state(&ctx, &prompt_data, model_name.as_deref());
            emit_prompt_state(state);
            emit_prompt_ready(None);
        }

        // Draw the prompt and input line
        let sb = render_sidebar_data(&mut sidebars, &mut sidebar_state, &ctx);
        draw_prompt_and_input(
            &mut guard.terminal,
            &prompt_manager,
            &ctx,
            &editor,
            sb.as_ref(),
            &sidebar_state,
            focus,
        )?;

        // Read input line using the InputEditor event loop
        let line = match read_input_line(
            &mut guard.terminal,
            &mut editor,
            &mut history,
            &prompt_manager,
            &ctx,
            &executor,
            &mut sidebars,
            &mut sidebar_state,
            &mut focus,
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
                display_tuple_activity(&mut executor, &mut prompt_manager, &cwd_handle, &mut agent)
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

        // Task 7b: Reset sidebar cursor if cwd changed (e.g., after cd)
        let new_cwd = std::env::current_dir().unwrap_or_default();
        if new_cwd != ctx.cwd {
            sidebar_state.cursor_index = 0;
            sidebars.invalidate_all();
        }

        // Re-enable raw mode for the next prompt
        let _ = enable_raw_mode();
    }

    Ok(())
}

/// Draw the prompt and current editor input, with optional sidebar.
fn draw_prompt_and_input(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    prompt_manager: &PromptManager,
    ctx: &ComponentContext,
    editor: &InputEditor,
    sidebar_data: Option<&SidebarData>,
    sidebar_state: &SidebarState,
    focus: Focus,
) -> Result<()> {
    let prompt_data = prompt_manager.render_data(ctx);
    let (left_line, _right_line) = PromptRenderer::render(&prompt_data);
    let cursor_row = editor.buffer.cursor_row();
    let cursor_col = editor.buffer.cursor_col();
    let status = editor.status_line();

    terminal
        .draw(|frame| {
            let area = frame.area();

            // Horizontal split: sidebar (if visible) | main
            let (sidebar_area, main_area) =
                if let Some(sb_data) = sidebar_data {
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

            // Render sidebar if visible
            if let (Some(sb_area), Some(sb_data)) = (sidebar_area, sidebar_data) {
                let selected = if focus == Focus::Sidebar {
                    Some(sidebar_state.cursor_index)
                } else {
                    None
                };
                SidebarRenderer::render(
                    frame,
                    sb_area,
                    sb_data,
                    focus == Focus::Sidebar,
                    selected,
                );
            }

            // Render context menu overlay if active (Task 8)
            if let Some(ref menu) = sidebar_state.context_menu {
                if let Some(sb_area) = sidebar_area {
                    render_context_menu(frame, sb_area, sidebar_state.cursor_index, menu);
                }
            }

            // Main layout: prompt+input area + status line
            let main_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(1),    // prompt + input area
                    Constraint::Length(1), // status line
                ])
                .split(main_area);

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
                let continuation = format!(
                    "{:>width$}{}",
                    "> ",
                    line,
                    width = prompt_width as usize - 2
                );
                input_lines.push(Line::from(continuation));
            }

            let input_paragraph = Paragraph::new(input_lines);
            frame.render_widget(input_paragraph, main_chunks[0]);

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
            frame.render_widget(status_bar, main_chunks[1]);

            // Position cursor (only when prompt is focused)
            if focus == Focus::Prompt {
                let cursor_x = main_chunks[0].x + prompt_width + cursor_col as u16;
                let cursor_y = main_chunks[0].y + cursor_row as u16;
                frame.set_cursor_position((cursor_x, cursor_y));
            }
        })
        .map_err(|e| miette::miette!("TUI render error: {}", e))?;

    Ok(())
}

/// Render the context menu as a floating overlay on the sidebar.
fn render_context_menu(
    frame: &mut ratatui::Frame,
    sb_area: Rect,
    cursor_index: usize,
    menu: &ContextMenu,
) {
    let menu_width = 20u16.min(sb_area.width.saturating_sub(2));
    let menu_height = 6u16; // 4 options + border
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
) -> Result<()> {
    let sb = render_sidebar_data(sidebars, sidebar_state, ctx);
    draw_prompt_and_input(
        terminal,
        prompt_manager,
        ctx,
        editor,
        sb.as_ref(),
        sidebar_state,
        focus,
    )
}

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
) -> Result<Option<String>> {
    loop {
        // Poll for crossterm events
        if event::poll(std::time::Duration::from_millis(50))
            .map_err(|e| miette::miette!("event poll error: {}", e))?
        {
            let ev = event::read().map_err(|e| miette::miette!("event read error: {}", e))?;

            match ev {
                Event::Key(key)
                    if key.kind == crossterm::event::KeyEventKind::Press =>
                {
                    // Global: Ctrl+B toggles sidebar (Task 5)
                    // Match both Ctrl+B representations crossterm may send
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
                            terminal,
                            prompt_manager,
                            ctx,
                            editor,
                            sidebars,
                            sidebar_state,
                            *focus,
                        )?;
                        continue;
                    }

                    // Tab switches focus when sidebar is visible (Task 5)
                    if key.code == KeyCode::Tab && sidebar_state.visible {
                        *focus = match focus {
                            Focus::Prompt => Focus::Sidebar,
                            Focus::Sidebar => {
                                sidebar_state.context_menu = None;
                                Focus::Prompt
                            }
                        };
                        redraw(
                            terminal,
                            prompt_manager,
                            ctx,
                            editor,
                            sidebars,
                            sidebar_state,
                            *focus,
                        )?;
                        continue;
                    }

                    // Sidebar-focused key handling (Tasks 5, 6, 7)
                    if *focus == Focus::Sidebar {
                        let action =
                            handle_sidebar_key(key, sidebar_state, sidebars, ctx);
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
                                            use std::io::Write;
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
                            terminal,
                            prompt_manager,
                            ctx,
                            editor,
                            sidebars,
                            sidebar_state,
                            *focus,
                        )?;
                        continue;
                    }

                    // --- Prompt-focused key handling (existing) ---

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
                            redraw(
                                terminal,
                                prompt_manager,
                                ctx,
                                editor,
                                sidebars,
                                sidebar_state,
                                *focus,
                            )?;
                        }
                        EditorAction::HistoryPrev => {
                            let current = editor.text();
                            if let Some(entry) = history.prev(&current) {
                                editor.set_text(entry);
                            }
                            redraw(
                                terminal,
                                prompt_manager,
                                ctx,
                                editor,
                                sidebars,
                                sidebar_state,
                                *focus,
                            )?;
                        }
                        EditorAction::HistoryNext => {
                            if let Some(entry) = history.next() {
                                editor.set_text(entry);
                            } else {
                                editor.set_text(history.saved_input());
                            }
                            redraw(
                                terminal,
                                prompt_manager,
                                ctx,
                                editor,
                                sidebars,
                                sidebar_state,
                                *focus,
                            )?;
                        }
                        EditorAction::Complete(prefix) => {
                            let builtins = vec![
                                "cd", "pwd", "echo", "export", "unset", "source", "exit",
                                "history", "jobs", "fg", "bg", "alias", "unalias", "set", "shift",
                                "test", "true", "false", "read", "printf", "type", "hash",
                                "ulimit", "umask", "wait", "trap", "return", "break", "continue",
                                "eval", "exec", "command", "builtin", "local", "declare",
                                "readonly", "getopts", "times", "llm", "path",
                            ]
                            .into_iter()
                            .map(String::from)
                            .collect();

                            let history_entries: Vec<String> =
                                history.entries().iter().cloned().collect();

                            let comp_ctx = CompletionContext::from_env(builtins, history_entries);
                            let items = CompletionEngine::complete(&prefix, &comp_ctx);

                            if let Some(first) = items.first() {
                                let line = editor.text();
                                let suffix = &line[..line.len() - prefix.len()];
                                let completed = format!("{}{}", suffix, first.text);
                                editor.set_text(&completed);
                            }

                            redraw(
                                terminal,
                                prompt_manager,
                                ctx,
                                editor,
                                sidebars,
                                sidebar_state,
                                *focus,
                            )?;
                        }
                        EditorAction::Execute(cmd) => {
                            match cmd.as_str() {
                                "q" | "quit" => {
                                    return Ok(Some("exit".to_string()));
                                }
                                _ => {}
                            }
                            redraw(
                                terminal,
                                prompt_manager,
                                ctx,
                                editor,
                                sidebars,
                                sidebar_state,
                                *focus,
                            )?;
                        }
                        _ => {
                            redraw(
                                terminal,
                                prompt_manager,
                                ctx,
                                editor,
                                sidebars,
                                sidebar_state,
                                *focus,
                            )?;
                        }
                    }
                }
                Event::Resize(_, _) => {
                    redraw(
                        terminal,
                        prompt_manager,
                        ctx,
                        editor,
                        sidebars,
                        sidebar_state,
                        *focus,
                    )?;
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
    super::repl::display_tuple_activity(executor, prompt_manager, cwd_handle, agent).await;
}
