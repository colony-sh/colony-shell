//! Main content display area
//!
//! Shows terminal output (plain text lines with ANSI color support)
//! and agent task structured views when an agent is active.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

/// Status of an agent task
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskStatus {
    Running,
    Complete,
    Failed,
}

/// A step in an agent task execution
#[derive(Debug, Clone)]
pub struct AgentStep {
    pub tool: String,
    pub description: String,
    pub status: TaskStatus,
}

/// Content blocks that can appear in the main area
#[derive(Debug, Clone)]
pub enum ContentBlock {
    Shell {
        command: String,
        output: Vec<String>,
        exit_code: i32,
        collapsed: bool,
    },
    Agent {
        task: String,
        steps: Vec<AgentStep>,
        status: TaskStatus,
        collapsed: bool,
    },
}

/// Manages the scrollback buffer and rendering of the main content area
pub struct ContentArea {
    blocks: Vec<ContentBlock>,
    scroll_offset: usize,
    max_scrollback: usize,
}

impl ContentArea {
    pub fn new() -> Self {
        Self {
            blocks: Vec::new(),
            scroll_offset: 0,
            max_scrollback: 10_000,
        }
    }

    pub fn with_max_scrollback(mut self, max: usize) -> Self {
        self.max_scrollback = max;
        self
    }

    /// Add a shell command + output block
    pub fn push_shell_block(&mut self, command: String, output: Vec<String>, exit_code: i32) {
        self.blocks.push(ContentBlock::Shell {
            command,
            output,
            exit_code,
            collapsed: false,
        });
        self.trim_scrollback();
        self.scroll_to_bottom();
    }

    /// Add an agent task block
    pub fn push_agent_block(&mut self, task: String) {
        self.blocks.push(ContentBlock::Agent {
            task,
            steps: Vec::new(),
            status: TaskStatus::Running,
            collapsed: false,
        });
        self.scroll_to_bottom();
    }

    /// Add a step to the most recent agent block
    pub fn push_agent_step(&mut self, step: AgentStep) {
        let Some(ContentBlock::Agent { ref mut steps, .. }) = self.blocks.last_mut() else {
            return;
        };
        steps.push(step);
    }

    /// Complete the most recent agent block
    pub fn complete_agent_block(&mut self, status: TaskStatus) {
        let Some(ContentBlock::Agent {
            status: ref mut block_status,
            ..
        }) = self.blocks.last_mut()
        else {
            return;
        };
        *block_status = status;
    }

    /// Toggle collapse on a block at the given index
    pub fn toggle_collapse(&mut self, index: usize) {
        let Some(block) = self.blocks.get_mut(index) else {
            return;
        };

        match block {
            ContentBlock::Shell {
                ref mut collapsed, ..
            } => *collapsed = !*collapsed,
            ContentBlock::Agent {
                ref mut collapsed, ..
            } => *collapsed = !*collapsed,
        }
    }

    /// Get total number of content blocks
    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    /// Scroll up by one line
    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    /// Scroll down by one line
    pub fn scroll_down(&mut self) {
        self.scroll_offset += 1;
    }

    /// Scroll to the bottom (most recent output)
    pub fn scroll_to_bottom(&mut self) {
        let total_lines = self.total_lines();
        self.scroll_offset = total_lines;
    }

    /// Get the current scroll offset
    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    /// Render the content area into the given frame area
    pub fn render(&self, frame: &mut Frame<'_>, area: Rect) {
        let lines = self.to_lines();

        let block = Block::default()
            .borders(Borders::NONE);

        let paragraph = Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((self.scroll_offset as u16, 0));

        frame.render_widget(paragraph, area);
    }

    /// Convert all content blocks to ratatui Lines
    fn to_lines(&self) -> Vec<Line<'static>> {
        let mut lines = Vec::new();

        for block in &self.blocks {
            match block {
                ContentBlock::Shell {
                    command,
                    output,
                    exit_code,
                    collapsed,
                } => {
                    render_shell_block(&mut lines, command, output, *exit_code, *collapsed);
                }
                ContentBlock::Agent {
                    task,
                    steps,
                    status,
                    collapsed,
                } => {
                    render_agent_block(&mut lines, task, steps, status, *collapsed);
                }
            }
        }

        lines
    }

    fn total_lines(&self) -> usize {
        self.to_lines().len()
    }

    fn trim_scrollback(&mut self) {
        while self.blocks.len() > self.max_scrollback {
            self.blocks.remove(0);
        }
    }
}

impl Default for ContentArea {
    fn default() -> Self {
        Self::new()
    }
}

fn render_shell_block(
    lines: &mut Vec<Line<'static>>,
    command: &str,
    output: &[String],
    exit_code: i32,
    collapsed: bool,
) {
    // Command line
    let exit_style = if exit_code == 0 {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::Red)
    };

    let prefix = if collapsed { "\u{25b6}" } else { "\u{25bc}" };

    lines.push(Line::from(vec![
        Span::styled(format!("{} ", prefix), Style::default().fg(Color::DarkGray)),
        Span::styled("> ", exit_style),
        Span::styled(
            command.to_string(),
            Style::default().add_modifier(Modifier::BOLD),
        ),
    ]));

    if collapsed {
        return;
    }

    // Output lines
    for line in output {
        lines.push(Line::from(Span::raw(line.clone())));
    }

    // Blank line separator
    lines.push(Line::from(""));
}

fn render_agent_block(
    lines: &mut Vec<Line<'static>>,
    task: &str,
    steps: &[AgentStep],
    status: &TaskStatus,
    collapsed: bool,
) {
    let (status_icon, status_color) = match status {
        TaskStatus::Running => ("\u{25cf}", Color::Yellow),     // filled circle
        TaskStatus::Complete => ("\u{2713}", Color::Green),      // check mark
        TaskStatus::Failed => ("\u{2717}", Color::Red),          // x mark
    };

    let prefix = if collapsed { "\u{25b6}" } else { "\u{25bc}" };

    lines.push(Line::from(vec![
        Span::styled(format!("{} ", prefix), Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{} ", status_icon),
            Style::default().fg(status_color),
        ),
        Span::styled(
            format!("Task: {}", task),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    if collapsed {
        return;
    }

    for (i, step) in steps.iter().enumerate() {
        let is_last = i == steps.len() - 1;
        let connector = if is_last { "\u{2514}\u{2500}" } else { "\u{251c}\u{2500}" };

        let (step_icon, step_color) = match step.status {
            TaskStatus::Running => ("\u{25cb}", Color::Yellow),
            TaskStatus::Complete => ("\u{2713}", Color::Green),
            TaskStatus::Failed => ("\u{2717}", Color::Red),
        };

        lines.push(Line::from(vec![
            Span::styled(
                format!("  {} ", connector),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                format!("{} ", step_icon),
                Style::default().fg(step_color),
            ),
            Span::styled(
                step.tool.clone(),
                Style::default().fg(Color::Cyan),
            ),
            Span::raw(" "),
            Span::styled(
                step.description.clone(),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }

    lines.push(Line::from(""));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_area_new() {
        let area = ContentArea::new();
        assert_eq!(area.block_count(), 0);
        assert_eq!(area.scroll_offset(), 0);
    }

    #[test]
    fn test_push_shell_block() {
        let mut area = ContentArea::new();
        area.push_shell_block(
            "ls -la".to_string(),
            vec!["file1.rs".to_string(), "file2.rs".to_string()],
            0,
        );
        assert_eq!(area.block_count(), 1);
    }

    #[test]
    fn test_push_agent_block_and_steps() {
        let mut area = ContentArea::new();
        area.push_agent_block("Fix the test".to_string());
        area.push_agent_step(AgentStep {
            tool: "Read".to_string(),
            description: "tests/main.rs".to_string(),
            status: TaskStatus::Complete,
        });
        area.push_agent_step(AgentStep {
            tool: "Edit".to_string(),
            description: "src/lib.rs [+3 -1]".to_string(),
            status: TaskStatus::Complete,
        });
        area.complete_agent_block(TaskStatus::Complete);

        assert_eq!(area.block_count(), 1);
    }

    #[test]
    fn test_toggle_collapse() {
        let mut area = ContentArea::new();
        area.push_shell_block("echo hello".to_string(), vec!["hello".to_string()], 0);

        // Initially not collapsed
        let ContentBlock::Shell { collapsed, .. } = &area.blocks[0] else {
            panic!("expected Shell block");
        };
        assert!(!collapsed);

        area.toggle_collapse(0);

        let ContentBlock::Shell { collapsed, .. } = &area.blocks[0] else {
            panic!("expected Shell block");
        };
        assert!(collapsed);
    }

    #[test]
    fn test_scroll_operations() {
        let mut area = ContentArea::new();
        assert_eq!(area.scroll_offset(), 0);

        area.scroll_down();
        assert_eq!(area.scroll_offset(), 1);

        area.scroll_down();
        assert_eq!(area.scroll_offset(), 2);

        area.scroll_up();
        assert_eq!(area.scroll_offset(), 1);

        area.scroll_up();
        area.scroll_up(); // Should not go below 0
        assert_eq!(area.scroll_offset(), 0);
    }

    #[test]
    fn test_to_lines_shell_block() {
        let mut area = ContentArea::new();
        area.push_shell_block(
            "echo test".to_string(),
            vec!["test".to_string()],
            0,
        );

        let lines = area.to_lines();
        // command line + output line + blank separator
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_to_lines_shell_block_collapsed() {
        let mut area = ContentArea::new();
        area.push_shell_block(
            "echo test".to_string(),
            vec!["test".to_string()],
            0,
        );
        area.toggle_collapse(0);

        let lines = area.to_lines();
        // Only command line (collapsed)
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn test_to_lines_agent_block() {
        let mut area = ContentArea::new();
        area.push_agent_block("Fix bug".to_string());
        area.push_agent_step(AgentStep {
            tool: "Read".to_string(),
            description: "file.rs".to_string(),
            status: TaskStatus::Complete,
        });

        let lines = area.to_lines();
        // task header + 1 step + blank separator
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_toggle_collapse_invalid_index() {
        let mut area = ContentArea::new();
        // Should not panic
        area.toggle_collapse(999);
    }

    #[test]
    fn test_push_agent_step_no_agent_block() {
        let mut area = ContentArea::new();
        // Should not panic when no agent block exists
        area.push_agent_step(AgentStep {
            tool: "Read".to_string(),
            description: "file.rs".to_string(),
            status: TaskStatus::Complete,
        });
    }

    #[test]
    fn test_max_scrollback() {
        let mut area = ContentArea::new().with_max_scrollback(3);
        for i in 0..5 {
            area.push_shell_block(format!("cmd {}", i), vec![], 0);
        }
        assert_eq!(area.block_count(), 3);
    }
}
