//! Main TUI application
//!
//! Provides both the full reactive `App` (vim-style, tree-sitter, event-driven)
//! and the simpler `TuiApp` (basic ratatui renderer using colony-ui-core).

use std::io::{self, Stdout};
use std::time::Duration;

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame, Terminal,
};

use crate::content::{ContentBlock, ContentLine, RawContent};
use crate::event::{Event, EventHandler};
use crate::input::{Editor, Highlighter};
use crate::keybindings::{Action, KeyMapper, Mode};
use crate::prompt::PromptRenderer;
use crate::theme::Theme;
use colony_ui_core::ComponentContext;

/// Application state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppState {
    Running,
    Quitting,
}

/// Main TUI application
pub struct App {
    /// Terminal backend
    terminal: Terminal<CrosstermBackend<Stdout>>,
    /// Event handler
    events: EventHandler,
    /// Key mapper
    keys: KeyMapper,
    /// Theme
    theme: Theme,
    /// Input editor
    editor: Editor,
    /// Syntax highlighter
    highlighter: Highlighter,
    /// Content area
    content: RawContent,
    /// Prompt renderer
    prompt: PromptRenderer,
    /// Command buffer (for : commands)
    command_buffer: String,
    /// Search buffer
    search_buffer: String,
    /// Application state
    state: AppState,
    /// Last exit code
    last_exit_code: i32,
    /// Current model name
    model: Option<String>,
}

impl App {
    /// Create a new TUI application
    pub fn new() -> miette::Result<Self> {
        // Setup terminal
        enable_raw_mode().map_err(|e| miette::miette!("Failed to enable raw mode: {}", e))?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
            .map_err(|e| miette::miette!("Failed to setup terminal: {}", e))?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)
            .map_err(|e| miette::miette!("Failed to create terminal: {}", e))?;

        Ok(Self {
            terminal,
            events: EventHandler::new(Duration::from_millis(100)),
            keys: KeyMapper::new(),
            theme: Theme::default(),
            editor: Editor::new(),
            highlighter: Highlighter::new(),
            content: RawContent::new(),
            prompt: PromptRenderer::new(),
            command_buffer: String::new(),
            search_buffer: String::new(),
            state: AppState::Running,
            last_exit_code: 0,
            model: None,
        })
    }

    /// Run the application main loop
    pub async fn run(&mut self) -> miette::Result<()> {
        while self.state == AppState::Running {
            // Draw UI
            self.draw()?;

            // Handle events
            if let Some(event) = self.events.next().await {
                self.handle_event(event)?;
            }
        }

        // Cleanup
        self.cleanup()?;
        Ok(())
    }

    /// Draw the UI
    fn draw(&mut self) -> miette::Result<()> {
        // Pre-compute data needed for rendering (avoid borrow issues)
        let ctx = self.create_context();
        let mode = self.keys.mode();
        let cursor_pos = self.editor.cursor_pos();
        let cursor = self.editor.cursor();
        let buffer = self.editor.buffer().to_string();
        let command_buffer = self.command_buffer.clone();
        let search_buffer = self.search_buffer.clone();
        let visible_lines = self.content.visible_lines();

        // Render prompt line
        let prompt_line = self.prompt.render(&ctx, 80); // Will be re-rendered with actual width
        let prompt_width: usize = prompt_line.spans.iter().map(|s| s.content.len()).sum();

        // Highlight input
        let highlighted = self.highlighter.highlight(&buffer);

        let theme = self.theme.clone();

        self.terminal
            .draw(|frame| {
                let size = frame.area();

                // Main layout: content area + input area
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Min(3),    // Content area
                        Constraint::Length(3), // Input area (prompt + input)
                        Constraint::Length(1), // Status bar
                    ])
                    .split(size);

                // Render content
                render_content(frame, chunks[0], &visible_lines);

                // Render input
                render_input(frame, chunks[1], &prompt_line, &highlighted, cursor, prompt_width, mode);

                // Render status
                render_status(frame, chunks[2], mode, &command_buffer, &search_buffer, cursor_pos, &theme);
            })
            .map_err(|e| miette::miette!("Failed to draw: {}", e))?;

        Ok(())
    }

    /// Handle an event
    fn handle_event(&mut self, event: Event) -> miette::Result<()> {
        match event {
            Event::Key(key) => {
                if let Some(action) = self.keys.map(key) {
                    self.handle_action(action)?;
                }
            }
            Event::Resize(_, _) => {
                // Terminal will redraw automatically
            }
            Event::TupleEvent(event_type) => {
                self.prompt.on_event(&event_type);
            }
            Event::OutputReady(output) => {
                // Append output to current block
                if let Some(ContentBlock::Shell { output: out, .. }) = self.content.last_mut() {
                    out.extend(output.as_bytes());
                }
            }
            Event::CommandComplete(exit_code) => {
                self.last_exit_code = exit_code;
            }
            Event::Tick | Event::Mouse(_) => {}
        }
        Ok(())
    }

    /// Handle an action
    fn handle_action(&mut self, action: Action) -> miette::Result<()> {
        match action {
            // Mode changes
            Action::EnterNormal => {
                self.keys.set_mode(Mode::Normal);
                self.command_buffer.clear();
                self.search_buffer.clear();
            }
            Action::EnterInsert => self.keys.set_mode(Mode::Insert),
            Action::EnterInsertAfter => {
                self.editor.cursor_right();
                self.keys.set_mode(Mode::Insert);
            }
            Action::EnterInsertStart => {
                self.editor.cursor_line_start();
                self.keys.set_mode(Mode::Insert);
            }
            Action::EnterInsertEnd => {
                self.editor.cursor_line_end();
                self.keys.set_mode(Mode::Insert);
            }
            Action::EnterCommand => {
                self.keys.set_mode(Mode::Command);
                self.command_buffer.clear();
            }
            Action::EnterSearch => {
                self.keys.set_mode(Mode::Search);
                self.search_buffer.clear();
            }
            Action::InsertLineBelow | Action::InsertLineAbove => {
                self.keys.set_mode(Mode::Insert);
            }

            // Text input
            Action::InsertChar(c) => match self.keys.mode() {
                Mode::Insert => self.editor.insert_char(c),
                Mode::Command => self.command_buffer.push(c),
                Mode::Search => self.search_buffer.push(c),
                _ => {}
            },
            Action::DeleteChar => self.editor.delete_char(),
            Action::DeleteCharBackward => match self.keys.mode() {
                Mode::Insert => self.editor.delete_char_backward(),
                Mode::Command => { self.command_buffer.pop(); }
                Mode::Search => { self.search_buffer.pop(); }
                _ => {}
            },
            Action::DeleteWordBackward => self.editor.delete_word_backward(),
            Action::DeleteToLineStart => self.editor.delete_to_line_start(),
            Action::DeleteToLineEnd => self.editor.delete_to_line_end(),
            Action::DeletePending => {} // TODO: vim d operator

            // Cursor movement
            Action::CursorLeft => self.editor.cursor_left(),
            Action::CursorRight => self.editor.cursor_right(),
            Action::CursorLineStart => self.editor.cursor_line_start(),
            Action::CursorLineEnd => self.editor.cursor_line_end(),
            Action::CursorWordForward => self.editor.cursor_word_forward(),
            Action::CursorWordBackward => self.editor.cursor_word_backward(),
            Action::CursorWordEnd => self.editor.cursor_word_end(),
            Action::CursorUp | Action::CursorDown => {}

            // Scrolling
            Action::ScrollUp => self.content.scroll_up(1),
            Action::ScrollDown => self.content.scroll_down(1),
            Action::ScrollHalfPageUp => self.content.scroll_up(10),
            Action::ScrollHalfPageDown => self.content.scroll_down(10),
            Action::ScrollToTop => self.content.scroll_to_top(),
            Action::ScrollToBottom => self.content.scroll_to_bottom(),

            // History
            Action::HistoryPrev => self.editor.history_prev(),
            Action::HistoryNext => self.editor.history_next(),
            Action::HistorySearch => {} // TODO: fzf-style search

            // Completion
            Action::AcceptCompletion => {} // TODO
            Action::NextCompletion => {}   // TODO
            Action::PrevCompletion => {}   // TODO

            // Clipboard (vim-style)
            Action::YankPending => {}  // TODO
            Action::PasteAfter => {}   // TODO
            Action::PasteBefore => {}  // TODO

            // Undo/Redo
            Action::Undo => {} // TODO
            Action::Redo => {} // TODO

            // Search
            Action::ExecuteSearch => {
                // TODO: implement search
                self.keys.set_mode(Mode::Normal);
            }
            Action::SearchNext => {}
            Action::SearchPrev => {}

            // Execution
            Action::Execute => {
                let cmd = self.editor.take();
                if !cmd.trim().is_empty() {
                    self.editor.add_to_history(&cmd);
                    self.execute_command(&cmd)?;
                }
            }
            Action::ExecuteCommand => {
                let cmd = std::mem::take(&mut self.command_buffer);
                self.handle_ex_command(&cmd)?;
                self.keys.set_mode(Mode::Normal);
            }

            // Global
            Action::Interrupt => {
                // Clear current input
                self.editor.clear();
                self.keys.set_mode(Mode::Normal);
            }
            Action::Eof => {
                if self.editor.is_empty() {
                    self.state = AppState::Quitting;
                }
            }
            Action::Quit => {
                self.state = AppState::Quitting;
            }
            Action::CommandPalette => {} // TODO
            Action::ToggleLeftSidebar => {}  // TODO
            Action::ToggleRightSidebar => {} // TODO
        }
        Ok(())
    }

    /// Execute a shell command
    fn execute_command(&mut self, cmd: &str) -> miette::Result<()> {
        // Add command to content area
        self.content.push(ContentBlock::shell(
            cmd.to_string(),
            Vec::new(),
            0,
        ));

        // TODO: Actually execute the command via colony-exec
        // For now, just show a placeholder
        if let Some(ContentBlock::Shell { output, exit_code, .. }) = self.content.last_mut() {
            *output = format!("TODO: Execute '{}'\n", cmd).into_bytes();
            *exit_code = 0;
        }

        Ok(())
    }

    /// Handle ex commands (:quit, :q, etc)
    fn handle_ex_command(&mut self, cmd: &str) -> miette::Result<()> {
        match cmd.trim() {
            "q" | "quit" => self.state = AppState::Quitting,
            "clear" => self.content.clear(),
            _ => {} // Unknown command
        }
        Ok(())
    }

    /// Create component context for prompt rendering
    fn create_context(&self) -> ComponentContext {
        ComponentContext {
            cwd: std::env::current_dir().unwrap_or_default(),
            env: std::env::vars().collect(),
            last_exit_code: self.last_exit_code,
            running_jobs: 0,
            stopped_jobs: 0,
            model: self.model.clone(),
            last_command_duration: None,
            hostname: hostname::get().ok().and_then(|h| h.into_string().ok()),
        }
    }

    /// Cleanup terminal on exit
    fn cleanup(&mut self) -> miette::Result<()> {
        disable_raw_mode().map_err(|e| miette::miette!("Failed to disable raw mode: {}", e))?;
        execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )
        .map_err(|e| miette::miette!("Failed to cleanup terminal: {}", e))?;
        self.terminal
            .show_cursor()
            .map_err(|e| miette::miette!("Failed to show cursor: {}", e))?;
        Ok(())
    }
}

impl Drop for App {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}

// Standalone render functions to avoid borrow issues

fn render_content(frame: &mut Frame, area: Rect, visible_lines: &[ContentLine]) {
    let lines: Vec<Line> = visible_lines
        .iter()
        .map(|line| match line {
            ContentLine::Command { text, exit_code } => {
                let style = if *exit_code == 0 {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::Red)
                };
                Line::from(Span::styled(text.clone(), style))
            }
            ContentLine::Output(text) => Line::from(text.clone()),
            ContentLine::Task { text, status: _ } => {
                Line::from(Span::styled(text.clone(), Style::default().fg(Color::Blue)))
            }
            ContentLine::Step { text, status: _ } => {
                Line::from(Span::styled(text.clone(), Style::default().fg(Color::DarkGray)))
            }
        })
        .collect();

    let content = Paragraph::new(lines)
        .block(Block::default().borders(Borders::NONE))
        .wrap(Wrap { trim: false });

    frame.render_widget(content, area);
}

fn render_input(
    frame: &mut Frame,
    area: Rect,
    prompt_line: &Line,
    highlighted: &Line,
    cursor: usize,
    prompt_width: usize,
    mode: Mode,
) {
    // Combine prompt with input
    let mut spans = prompt_line.spans.clone();
    spans.extend(highlighted.spans.clone());

    let input_line = Line::from(spans);

    let input_widget = Paragraph::new(vec![input_line])
        .block(Block::default().borders(Borders::TOP));

    frame.render_widget(input_widget, area);

    // Position cursor
    let cursor_x = area.x + prompt_width as u16 + cursor as u16;
    let cursor_y = area.y + 1;

    if mode == Mode::Insert {
        frame.set_cursor_position((cursor_x.min(area.right() - 1), cursor_y));
    }
}

fn render_status(
    frame: &mut Frame,
    area: Rect,
    mode: Mode,
    command_buffer: &str,
    search_buffer: &str,
    cursor_pos: (usize, usize),
    theme: &Theme,
) {
    let mode_style = theme.mode_style(&mode);

    let mode_span = Span::styled(format!(" {} ", mode.name()), mode_style);

    let status_text = match mode {
        Mode::Command => format!(":{}", command_buffer),
        Mode::Search => format!("/{}", search_buffer),
        _ => String::new(),
    };

    let right_text = format!(
        " {}:{} ",
        cursor_pos.0 + 1,
        cursor_pos.1 + 1
    );

    let padding = (area.width as usize)
        .saturating_sub(mode.name().len() + 2)
        .saturating_sub(status_text.len())
        .saturating_sub(right_text.len());

    let line = Line::from(vec![
        mode_span,
        Span::raw(status_text),
        Span::raw(" ".repeat(padding)),
        Span::styled(right_text, Style::default().add_modifier(Modifier::DIM)),
    ]);

    let status = Paragraph::new(vec![line])
        .style(Style::default().bg(Color::DarkGray));

    frame.render_widget(status, area);
}

// === Legacy TuiApp (simpler renderer using colony-ui-core directly) ===

use colony_ui_core::component::SidebarOutput;
use colony_ui_core::context::ComponentContext as LegacyComponentContext;
use colony_ui_core::manager::PromptManager;
use crossterm::event::{self as crossterm_event, Event as CrosstermEvent, KeyCode, KeyEvent, KeyModifiers};
use crossterm::ExecutableCommand;

use crate::content_area::ContentArea;
use crate::layout::{LayoutState, TuiLayout};
use crate::prompt_renderer::PromptRenderer as LegacyPromptRenderer;
use crate::sidebar_renderer::SidebarRenderer;

/// The legacy TUI application (simpler, uses colony-ui-core PromptManager directly)
pub struct TuiApp {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    layout_state: LayoutState,
    content_area: ContentArea,
    prompt_manager: PromptManager,
    left_sidebar_outputs: Vec<(String, SidebarOutput)>,
    right_sidebar_outputs: Vec<(String, SidebarOutput)>,
    should_quit: bool,
    input_buffer: String,
    cursor_position: usize,
}

impl TuiApp {
    /// Create a new TUI application, entering raw mode and alternate screen
    pub fn new() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        stdout.execute(EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        Ok(Self {
            terminal,
            layout_state: LayoutState::new(),
            content_area: ContentArea::new(),
            prompt_manager: PromptManager::default(),
            left_sidebar_outputs: Vec::new(),
            right_sidebar_outputs: Vec::new(),
            should_quit: false,
            input_buffer: String::new(),
            cursor_position: 0,
        })
    }

    /// Create with a custom PromptManager
    pub fn with_prompt_manager(mut self, manager: PromptManager) -> Self {
        self.prompt_manager = manager;
        self
    }

    /// Access the layout state mutably
    pub fn layout_state_mut(&mut self) -> &mut LayoutState {
        &mut self.layout_state
    }

    /// Access the content area mutably
    pub fn content_area_mut(&mut self) -> &mut ContentArea {
        &mut self.content_area
    }

    /// Access the prompt manager mutably
    pub fn prompt_manager_mut(&mut self) -> &mut PromptManager {
        &mut self.prompt_manager
    }

    /// Update sidebar outputs
    pub fn set_left_sidebar_outputs(&mut self, outputs: Vec<(String, SidebarOutput)>) {
        self.left_sidebar_outputs = outputs;
    }

    /// Update sidebar outputs
    pub fn set_right_sidebar_outputs(&mut self, outputs: Vec<(String, SidebarOutput)>) {
        self.right_sidebar_outputs = outputs;
    }

    /// Check if the app should quit
    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    /// Main event loop
    pub fn run(&mut self, ctx: &LegacyComponentContext) -> io::Result<()> {
        while !self.should_quit {
            self.draw(ctx)?;

            // Poll for events with a timeout
            if crossterm_event::poll(Duration::from_millis(50))? {
                match crossterm_event::read()? {
                    CrosstermEvent::Key(key) => self.handle_key_event(key),
                    CrosstermEvent::Resize(_, _) => {} // Will redraw on next loop
                    _ => {}
                }
            }
        }

        Ok(())
    }

    /// Handle a single key event
    fn handle_key_event(&mut self, key: KeyEvent) {
        match (key.modifiers, key.code) {
            // Quit: Ctrl+D
            (KeyModifiers::CONTROL, KeyCode::Char('d')) => {
                self.should_quit = true;
            }
            // Toggle left sidebar: Ctrl+B
            (KeyModifiers::CONTROL, KeyCode::Char('b')) => {
                self.layout_state.toggle_left_sidebar();
            }
            // Toggle right sidebar: Ctrl+N
            (KeyModifiers::CONTROL, KeyCode::Char('n')) => {
                self.layout_state.toggle_right_sidebar();
            }
            // Scroll up content
            (KeyModifiers::NONE, KeyCode::Up) => {
                self.content_area.scroll_up();
            }
            // Scroll down content
            (KeyModifiers::NONE, KeyCode::Down) => {
                self.content_area.scroll_down();
            }
            // Character input
            (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::Char(c)) => {
                self.input_buffer.insert(self.cursor_position, c);
                self.cursor_position += 1;
            }
            // Backspace
            (KeyModifiers::NONE, KeyCode::Backspace) => {
                if self.cursor_position > 0 {
                    self.cursor_position -= 1;
                    self.input_buffer.remove(self.cursor_position);
                }
            }
            // Move cursor left
            (KeyModifiers::NONE, KeyCode::Left) => {
                self.cursor_position = self.cursor_position.saturating_sub(1);
            }
            // Move cursor right
            (KeyModifiers::NONE, KeyCode::Right) => {
                if self.cursor_position < self.input_buffer.len() {
                    self.cursor_position += 1;
                }
            }
            // Home
            (KeyModifiers::NONE, KeyCode::Home) => {
                self.cursor_position = 0;
            }
            // End
            (KeyModifiers::NONE, KeyCode::End) => {
                self.cursor_position = self.input_buffer.len();
            }
            _ => {}
        }
    }

    /// Draw the full TUI layout
    fn draw(&mut self, ctx: &LegacyComponentContext) -> io::Result<()> {
        let layout_state = self.layout_state.clone();
        let prompt_data = self.prompt_manager.render_data(ctx);
        let (left_line, _right_line) = LegacyPromptRenderer::render(&prompt_data);
        let input_text = self.input_buffer.clone();
        let cursor_pos = self.cursor_position;

        // Collect sidebar data before the closure
        let left_outputs = self.left_sidebar_outputs.clone();
        let right_outputs = self.right_sidebar_outputs.clone();

        self.terminal.draw(|frame| {
            let area = frame.area();
            let layout = TuiLayout::compute(area, &layout_state);

            // Render left sidebar
            if let Some(sidebar_area) = layout.left_sidebar {
                let sidebar = SidebarRenderer::new("Explorer");
                let output = left_outputs
                    .first()
                    .map(|(_, o)| o.clone())
                    .unwrap_or_else(SidebarOutput::hidden);
                sidebar.render(frame, sidebar_area, &output);
            }

            // Render main content
            let content_area_ref = &self.content_area;
            content_area_ref.render(frame, layout.main_content);

            // Render right sidebar
            if let Some(sidebar_area) = layout.right_sidebar {
                let sidebar = SidebarRenderer::new("Info");
                let output = right_outputs
                    .first()
                    .map(|(_, o)| o.clone())
                    .unwrap_or_else(SidebarOutput::hidden);
                sidebar.render(frame, sidebar_area, &output);
            }

            // Render input area
            let input_block = Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(Color::DarkGray));

            let input_line = Line::from(vec![
                Span::styled("> ", Style::default().fg(Color::Green)),
                Span::raw(input_text),
            ]);

            let input_paragraph = Paragraph::new(vec![input_line]).block(input_block);
            frame.render_widget(input_paragraph, layout.input_area);

            // Set cursor position
            frame.set_cursor_position((
                layout.input_area.x + 2 + cursor_pos as u16,
                layout.input_area.y + 1,
            ));

            // Render status bar
            let status_line = Line::from(vec![
                Span::styled(" ", Style::default().bg(Color::DarkGray)),
                left_line.spans.into_iter().next().unwrap_or(Span::raw("")),
                Span::styled(
                    "  ",
                    Style::default().bg(Color::DarkGray),
                ),
            ]);

            let status_bar = Paragraph::new(vec![status_line])
                .style(Style::default().bg(Color::DarkGray).fg(Color::White));
            frame.render_widget(status_bar, layout.status_bar);
        })?;

        Ok(())
    }
}

impl Drop for TuiApp {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = io::stdout().execute(LeaveAlternateScreen);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // TuiApp::new() requires a terminal, so we test the components it uses
    // rather than TuiApp directly in unit tests.

    #[test]
    fn test_layout_state_in_app_context() {
        let mut state = LayoutState::new();
        assert!(!state.left_sidebar_visible);

        state.toggle_left_sidebar();
        assert!(state.left_sidebar_visible);

        state.toggle_right_sidebar();
        assert!(state.right_sidebar_visible);
    }
}
