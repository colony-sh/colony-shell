use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use std::collections::HashSet;
use std::path::Path;
use std::sync::mpsc::Receiver;

use crate::tutorial::events::{ComplexityLevel, TraceEvent};
use crate::tutorial::tracer::{TraceDepth, TracerConfig};

use super::controls::{handle_event, TutorialAction};
use super::magic_pane::MagicPane;
use super::script_pane::ScriptPane;

pub enum PlaybackState {
    Playing { speed: f32 },
    Paused,
    Stepping,
}

pub struct TutorialTui {
    script_pane: ScriptPane,
    magic_pane: MagicPane,
    state: PlaybackState,
    depth: TraceDepth,
    teach_mode: bool,
    should_quit: bool,
}

impl TutorialTui {
    pub fn new(script_path: &Path) -> std::io::Result<Self> {
        let source = std::fs::read_to_string(script_path)?;
        Ok(Self {
            script_pane: ScriptPane::new(source),
            magic_pane: MagicPane::new(),
            state: PlaybackState::Paused,
            depth: TraceDepth::Mechanics,
            teach_mode: false,
            should_quit: false,
        })
    }

    pub fn with_config(mut self, config: &TracerConfig) -> Self {
        self.depth = config.depth;
        self.teach_mode = config.teach_mode;
        for &line in &config.marked_lines {
            self.script_pane.mark_line(line);
        }
        self
    }

    pub fn toggle_pause(&mut self) {
        self.state = match self.state {
            PlaybackState::Playing { .. } => PlaybackState::Paused,
            PlaybackState::Paused | PlaybackState::Stepping => PlaybackState::Playing { speed: 1.0 },
        };
    }

    pub fn step(&mut self) {
        self.state = PlaybackState::Stepping;
    }

    pub fn toggle_depth(&mut self) {
        self.depth = match self.depth {
            TraceDepth::Mechanics => TraceDepth::Internals,
            TraceDepth::Internals => TraceDepth::Mechanics,
        };
    }

    pub fn toggle_teach_mode(&mut self) {
        self.teach_mode = !self.teach_mode;
    }

    pub fn mark_current_line(&mut self) {
        let line = self.script_pane.current_line();
        if self.script_pane.is_line_marked(line) {
            self.script_pane.unmark_line(line);
        } else {
            self.script_pane.mark_line(line);
        }
    }

    pub fn handle_event(&mut self, event: TraceEvent) {
        match &event {
            TraceEvent::LineEnter { line, .. } => {
                self.script_pane.set_current_line(*line);
                self.magic_pane.clear();
            }
            _ => {}
        }
        self.magic_pane.handle_event(event, self.depth);
    }

    pub fn render(&self, frame: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(frame.area());

        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(chunks[0]);

        self.script_pane.render(frame, main_chunks[0]);
        self.magic_pane.render(frame, main_chunks[1]);
        self.render_controls(frame, chunks[1]);
    }

    fn render_controls(&self, frame: &mut Frame, area: Rect) {
        let state_indicator = match self.state {
            PlaybackState::Playing { .. } => "▶ PLAYING",
            PlaybackState::Paused => "⏸ PAUSED",
            PlaybackState::Stepping => "⏭ STEP",
        };

        let depth_indicator = match self.depth {
            TraceDepth::Mechanics => "L2",
            TraceDepth::Internals => "L3",
        };

        let teach_indicator = if self.teach_mode { "📚" } else { "" };

        let controls = Line::from(vec![
            Span::styled(
                format!(" {} ", state_indicator),
                Style::default().fg(Color::Black).bg(Color::Cyan),
            ),
            Span::raw(" "),
            Span::styled("[space]", Style::default().fg(Color::Yellow)),
            Span::raw(" pause "),
            Span::styled("[enter]", Style::default().fg(Color::Yellow)),
            Span::raw(" step "),
            Span::styled("[d]", Style::default().fg(Color::Yellow)),
            Span::raw(format!(" depth:{} ", depth_indicator)),
            Span::styled("[m]", Style::default().fg(Color::Yellow)),
            Span::raw(" mark "),
            Span::styled("[t]", Style::default().fg(Color::Yellow)),
            Span::raw(format!(" teach{} ", teach_indicator)),
            Span::styled("[q]", Style::default().fg(Color::Yellow)),
            Span::raw(" quit"),
        ]);

        frame.render_widget(Paragraph::new(controls), area);
    }

    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    pub fn process_input(&mut self, event: crossterm::event::Event) {
        match handle_event(event) {
            TutorialAction::TogglePause => self.toggle_pause(),
            TutorialAction::Step => self.step(),
            TutorialAction::ToggleDepth => self.toggle_depth(),
            TutorialAction::MarkLine => self.mark_current_line(),
            TutorialAction::ToggleTeachMode => self.toggle_teach_mode(),
            TutorialAction::Quit => self.should_quit = true,
            TutorialAction::Search | TutorialAction::None => {}
        }
    }

    pub fn is_playing(&self) -> bool {
        matches!(self.state, PlaybackState::Playing { .. })
    }

    pub fn is_stepping(&self) -> bool {
        matches!(self.state, PlaybackState::Stepping)
    }

    pub fn finish_step(&mut self) {
        if matches!(self.state, PlaybackState::Stepping) {
            self.state = PlaybackState::Paused;
        }
    }
}
