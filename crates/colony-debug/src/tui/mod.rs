//! Debug TUI -- split-pane interface for debug and X-Ray modes.
//!
//! The TUI renders two panes side by side:
//! - Left: script source with line numbers, current line, breakpoint markers
//! - Right: annotations (X-Ray) or variable inspector (Debug)
//!
//! A status bar at the bottom shows keyboard shortcuts.

pub mod info_pane;
pub mod source_pane;

use crossterm::event::{self, Event, KeyCode};
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;
use std::time::Duration;
use tokio::sync::mpsc;

use crate::controller::{DebugCommand, DebugMode};
use crate::events::TraceEvent;

use info_pane::InfoPane;
use source_pane::SourcePane;

/// Run the debug TUI. Blocks until the user quits or execution finishes.
///
/// This should be spawned on a separate tokio task. It reads `TraceEvent`s
/// from the executor and sends `DebugCommand`s back.
pub async fn run(
    source: &str,
    mode: DebugMode,
    mut event_rx: mpsc::UnboundedReceiver<TraceEvent>,
    command_tx: mpsc::UnboundedSender<DebugCommand>,
) -> std::io::Result<()> {
    let mut terminal = ratatui::init();
    let mut source_pane = SourcePane::new(source);
    let mut info_pane = InfoPane::new(mode);
    let mut current_mode = mode;

    loop {
        // Render
        terminal.draw(|frame| {
            let main_and_status = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(3), Constraint::Length(1)])
                .split(frame.area());

            let panes = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(main_and_status[0]);

            source_pane.render(frame, panes[0]);
            info_pane.render(frame, panes[1]);

            // Status bar
            let status = match current_mode {
                DebugMode::XRay => {
                    "[space] pause  [s/enter] step  [m] mode  [q] quit"
                }
                DebugMode::Debug => {
                    "[s] step  [n] next  [o] out  [c] continue  [m] mode  [q] quit"
                }
            };
            let status_widget =
                Paragraph::new(status).style(Style::default().fg(Color::DarkGray));
            frame.render_widget(status_widget, main_and_status[1]);
        })?;

        // Handle input (non-blocking)
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => {
                        let _ = command_tx.send(DebugCommand::Quit);
                        break;
                    }
                    KeyCode::Char(' ') => {
                        let _ = command_tx.send(DebugCommand::Pause);
                    }
                    KeyCode::Enter | KeyCode::Char('s') => {
                        let _ = command_tx.send(DebugCommand::StepInto);
                    }
                    KeyCode::Char('n') => {
                        let _ = command_tx.send(DebugCommand::StepOver);
                    }
                    KeyCode::Char('c') => {
                        let _ = command_tx.send(DebugCommand::Continue);
                    }
                    KeyCode::Char('o') => {
                        let _ = command_tx.send(DebugCommand::StepOut);
                    }
                    KeyCode::Char('m') => {
                        current_mode = match current_mode {
                            DebugMode::Debug => DebugMode::XRay,
                            DebugMode::XRay => DebugMode::Debug,
                        };
                        info_pane.set_mode(current_mode);
                        let _ = command_tx.send(DebugCommand::ToggleMode);
                    }
                    _ => {}
                }
            }
        }

        // Consume trace events
        while let Ok(event) = event_rx.try_recv() {
            match &event {
                TraceEvent::LineEnter { line, .. } => {
                    source_pane.set_current_line(*line);
                    info_pane.clear();
                }
                _ => {}
            }
            info_pane.push_event(&event);
        }
    }

    ratatui::restore();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_pane_creates_from_source() {
        let pane = SourcePane::new("echo hello\necho world");
        assert_eq!(pane.line_count(), 2);
    }

    #[test]
    fn info_pane_starts_empty() {
        let pane = InfoPane::new(DebugMode::XRay);
        assert_eq!(pane.section_count(), 0);
    }
}
