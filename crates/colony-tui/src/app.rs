//! TUI application state and main event loop
//!
//! Manages the ratatui `Terminal`, layout state, and the main
//! `run()` event loop that polls crossterm events and tuple space events.

use crate::content_area::ContentArea;
use crate::layout::{LayoutState, TuiLayout};
use crate::prompt_renderer::PromptRenderer;
use crate::sidebar_renderer::SidebarRenderer;
use colony_ui_core::component::SidebarOutput;
use colony_ui_core::context::ComponentContext;
use colony_ui_core::manager::PromptManager;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Terminal;
use std::io::{self, Stdout};
use std::time::Duration;

/// The main TUI application
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
    pub fn run(&mut self, ctx: &ComponentContext) -> io::Result<()> {
        while !self.should_quit {
            self.draw(ctx)?;

            // Poll for events with a timeout
            if event::poll(Duration::from_millis(50))? {
                match event::read()? {
                    Event::Key(key) => self.handle_key_event(key),
                    Event::Resize(_, _) => {} // Will redraw on next loop
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
    fn draw(&mut self, ctx: &ComponentContext) -> io::Result<()> {
        let layout_state = self.layout_state.clone();
        let prompt_data = self.prompt_manager.render_data(ctx);
        let (left_line, _right_line) = PromptRenderer::render(&prompt_data);
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
