use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};
use std::path::Path;

use crate::tutorial::events::TraceEvent;
use crate::tutorial::tracer::{TraceDepth, TracerConfig};

use super::controls::{handle_event, TutorialAction};
use super::env_pane::EnvPane;
use super::magic_pane::MagicPane;
use super::script_pane::ScriptPane;

pub enum PlaybackState {
    Playing { speed_ms: u64 },
    Paused,
    Stepping,
}

pub struct TutorialTui {
    script_pane: ScriptPane,
    magic_pane: MagicPane,
    env_pane: EnvPane,
    output_lines: Vec<String>,
    state: PlaybackState,
    depth: TraceDepth,
    teach_mode: bool,
    should_quit: bool,
    event_history: Vec<(u32, Vec<TraceEvent>)>,
    history_index: Option<usize>,
    current_line_events: Vec<TraceEvent>,
    playback_speed_ms: u64,
}

impl TutorialTui {
    pub fn new(script_path: &Path) -> std::io::Result<Self> {
        let source = std::fs::read_to_string(script_path)?;
        Ok(Self {
            script_pane: ScriptPane::new(source),
            magic_pane: MagicPane::new(),
            env_pane: EnvPane::new(),
            output_lines: Vec::new(),
            state: PlaybackState::Paused,
            depth: TraceDepth::Mechanics,
            teach_mode: false,
            should_quit: false,
            event_history: Vec::new(),
            history_index: None,
            current_line_events: Vec::new(),
            playback_speed_ms: 1500,
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
            PlaybackState::Paused | PlaybackState::Stepping => PlaybackState::Playing {
                speed_ms: self.playback_speed_ms,
            },
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
        self.refresh_magic_pane();
    }

    pub fn toggle_teach_mode(&mut self) {
        self.teach_mode = !self.teach_mode;
    }

    fn refresh_magic_pane(&mut self) {
        self.magic_pane.clear();
        for event in &self.current_line_events {
            self.magic_pane.handle_event(event.clone(), self.depth);
        }
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
                if !self.current_line_events.is_empty() {
                    let current_line = self.script_pane.current_line();
                    self.event_history
                        .push((current_line, std::mem::take(&mut self.current_line_events)));
                }
                self.script_pane.set_current_line(*line);
                self.magic_pane.clear();
                self.env_pane.clear_changed();
                self.history_index = None;
            }
            TraceEvent::Output { text, .. } => {
                for line in text.lines() {
                    if !line.contains("DEBUG ") && !line.starts_with("DEBUG") {
                        self.output_lines.push(line.to_string());
                    }
                }
            }
            TraceEvent::Environment {
                variables, changed, ..
            } => {
                self.env_pane.update(variables.clone(), changed.clone());
            }
            _ => {}
        }
        self.current_line_events.push(event.clone());
        self.magic_pane.handle_event(event, self.depth);
    }

    pub fn render(&mut self, frame: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(frame.area());

        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(30),
                Constraint::Percentage(30),
                Constraint::Percentage(20),
                Constraint::Percentage(20),
            ])
            .split(chunks[0]);

        self.script_pane.render(frame, main_chunks[0]);
        self.magic_pane.render(frame, main_chunks[1]);
        self.env_pane.render(frame, main_chunks[2]);
        self.render_output_pane(frame, main_chunks[3]);
        self.render_controls(frame, chunks[1]);
    }

    fn render_output_pane(&self, frame: &mut Frame, area: Rect) {
        let lines: Vec<Line> = self
            .output_lines
            .iter()
            .map(|s| Line::from(Span::styled(s.clone(), Style::default().fg(Color::White))))
            .collect();

        let block = Block::default()
            .title(" Output ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Green));

        let inner_height = area.height.saturating_sub(2) as usize;
        let skip = lines.len().saturating_sub(inner_height);
        let visible_lines: Vec<Line> = lines.into_iter().skip(skip).collect();

        let paragraph = Paragraph::new(visible_lines)
            .block(block)
            .wrap(Wrap { trim: false });

        frame.render_widget(paragraph, area);
    }

    fn render_controls(&self, frame: &mut Frame, area: Rect) {
        let state_indicator = match self.state {
            PlaybackState::Playing { speed_ms } => format!("▶ {}ms", speed_ms),
            PlaybackState::Paused => "⏸ PAUSED".to_string(),
            PlaybackState::Stepping => "⏭ STEP".to_string(),
        };

        let depth_indicator = match self.depth {
            TraceDepth::Mechanics => "L2",
            TraceDepth::Internals => "L3",
        };

        let teach_indicator = if self.teach_mode { "📚" } else { "" };
        let ast_indicator = if self.magic_pane.is_ast_expanded() {
            "📂"
        } else {
            "📁"
        };

        let history_indicator = if let Some(idx) = self.history_index {
            format!(" [{}/{}]", idx + 1, self.event_history.len())
        } else {
            String::new()
        };

        let controls = Line::from(vec![
            Span::styled(
                format!(" {} ", state_indicator),
                Style::default().fg(Color::Black).bg(Color::Cyan),
            ),
            Span::styled(history_indicator, Style::default().fg(Color::Magenta)),
            Span::raw(" "),
            Span::styled("[space]", Style::default().fg(Color::Yellow)),
            Span::raw(" play "),
            Span::styled("[↑↓]", Style::default().fg(Color::Yellow)),
            Span::raw(" scroll "),
            Span::styled("[←→]", Style::default().fg(Color::Yellow)),
            Span::raw(" hist "),
            Span::styled("[d]", Style::default().fg(Color::Yellow)),
            Span::raw(format!(":{} ", depth_indicator)),
            Span::styled("[e]", Style::default().fg(Color::Yellow)),
            Span::raw(format!("{} ", ast_indicator)),
            Span::styled("[t]", Style::default().fg(Color::Yellow)),
            Span::raw(format!("{} ", teach_indicator)),
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
            TutorialAction::HistoryBack => self.history_back(),
            TutorialAction::HistoryForward => self.history_forward(),
            TutorialAction::SpeedUp => self.speed_up(),
            TutorialAction::SpeedDown => self.speed_down(),
            TutorialAction::ScrollUp => self.magic_pane.scroll_up(1),
            TutorialAction::ScrollDown => self.magic_pane.scroll_down(1),
            TutorialAction::ScrollPageUp => self.magic_pane.scroll_up(10),
            TutorialAction::ScrollPageDown => self.magic_pane.scroll_down(10),
            TutorialAction::ToggleAstExpand => self.toggle_ast_expand(),
            TutorialAction::Search | TutorialAction::None => {}
        }
    }

    fn toggle_ast_expand(&mut self) {
        self.magic_pane.toggle_ast_expand();
        self.refresh_magic_pane();
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

    pub fn history_back(&mut self) {
        if self.event_history.is_empty() {
            return;
        }

        self.state = PlaybackState::Paused;

        let new_index = match self.history_index {
            Some(0) => return,
            Some(idx) => idx - 1,
            None => self.event_history.len().saturating_sub(1),
        };

        self.history_index = Some(new_index);
        self.show_history_at_index(new_index);
    }

    pub fn history_forward(&mut self) {
        if self.event_history.is_empty() {
            return;
        }

        let max_index = self.event_history.len().saturating_sub(1);

        match self.history_index {
            Some(idx) if idx < max_index => {
                let new_index = idx + 1;
                self.history_index = Some(new_index);
                self.show_history_at_index(new_index);
            }
            Some(idx) if idx == max_index => {
                self.history_index = None;
                self.magic_pane.clear();
                for event in &self.current_line_events {
                    self.magic_pane.handle_event(event.clone(), self.depth);
                }
            }
            _ => {}
        }
    }

    fn show_history_at_index(&mut self, index: usize) {
        if let Some((line, events)) = self.event_history.get(index) {
            self.script_pane.set_current_line(*line);
            self.magic_pane.clear();
            for event in events {
                self.magic_pane.handle_event(event.clone(), self.depth);
            }
        }
    }

    pub fn speed_up(&mut self) {
        self.playback_speed_ms = (self.playback_speed_ms / 2).max(100);
        if let PlaybackState::Playing { ref mut speed_ms } = self.state {
            *speed_ms = self.playback_speed_ms;
        }
    }

    pub fn speed_down(&mut self) {
        self.playback_speed_ms = (self.playback_speed_ms * 2).min(5000);
        if let PlaybackState::Playing { ref mut speed_ms } = self.state {
            *speed_ms = self.playback_speed_ms;
        }
    }

    pub fn get_playback_speed_ms(&self) -> u64 {
        self.playback_speed_ms
    }

    pub fn is_browsing_history(&self) -> bool {
        self.history_index.is_some()
    }

    #[allow(dead_code)]
    pub fn add_output(&mut self, line: String) {
        self.output_lines.push(line);
    }

    #[allow(dead_code)]
    pub fn clear_output(&mut self) {
        self.output_lines.clear();
    }
}
