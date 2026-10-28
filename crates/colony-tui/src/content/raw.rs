//! Raw shell output content area

use super::ContentBlock;

/// Raw content area with scrollback
#[derive(Debug)]
pub struct RawContent {
    /// Content blocks
    blocks: Vec<ContentBlock>,
    /// Scroll offset (lines from bottom)
    scroll_offset: usize,
    /// Total visible height
    visible_height: usize,
}

impl RawContent {
    pub fn new() -> Self {
        Self {
            blocks: Vec::new(),
            scroll_offset: 0,
            visible_height: 24,
        }
    }

    /// Add a new content block
    pub fn push(&mut self, block: ContentBlock) {
        self.blocks.push(block);
        // Auto-scroll to bottom on new content
        self.scroll_offset = 0;
    }

    /// Get all blocks
    pub fn blocks(&self) -> &[ContentBlock] {
        &self.blocks
    }

    /// Get mutable reference to last block
    pub fn last_mut(&mut self) -> Option<&mut ContentBlock> {
        self.blocks.last_mut()
    }

    /// Set visible height
    pub fn set_visible_height(&mut self, height: usize) {
        self.visible_height = height;
    }

    /// Scroll up by lines
    pub fn scroll_up(&mut self, lines: usize) {
        let max_scroll = self.total_lines().saturating_sub(self.visible_height);
        self.scroll_offset = (self.scroll_offset + lines).min(max_scroll);
    }

    /// Scroll down by lines
    pub fn scroll_down(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
    }

    /// Scroll to top
    pub fn scroll_to_top(&mut self) {
        let max_scroll = self.total_lines().saturating_sub(self.visible_height);
        self.scroll_offset = max_scroll;
    }

    /// Scroll to bottom
    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
    }

    /// Get scroll offset
    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    /// Calculate total lines across all blocks
    fn total_lines(&self) -> usize {
        self.blocks
            .iter()
            .map(|block| match block {
                ContentBlock::Shell {
                    output, collapsed, ..
                } => {
                    if *collapsed {
                        1 // Just the command line
                    } else {
                        1 + String::from_utf8_lossy(output).lines().count()
                    }
                }
                ContentBlock::Agent {
                    steps, collapsed, ..
                } => {
                    if *collapsed {
                        1 // Just the task name
                    } else {
                        1 + steps.len()
                    }
                }
            })
            .sum()
    }

    /// Clear all content
    pub fn clear(&mut self) {
        self.blocks.clear();
        self.scroll_offset = 0;
    }

    /// Get visible lines for rendering
    pub fn visible_lines(&self) -> Vec<ContentLine> {
        let mut lines = Vec::new();

        for block in &self.blocks {
            match block {
                ContentBlock::Shell {
                    command,
                    output,
                    exit_code,
                    collapsed,
                } => {
                    // Command line with exit code indicator
                    let indicator = if *exit_code == 0 {
                        "\u{2714}"
                    } else {
                        "\u{2718}"
                    };
                    lines.push(ContentLine::Command {
                        text: format!("{} {}", indicator, command),
                        exit_code: *exit_code,
                    });

                    // Output lines (if not collapsed)
                    if !collapsed {
                        for line in String::from_utf8_lossy(output).lines() {
                            lines.push(ContentLine::Output(line.to_string()));
                        }
                    }
                }
                ContentBlock::Agent {
                    task,
                    steps,
                    status,
                    collapsed,
                    duration_ms,
                } => {
                    // Task header with duration if available
                    let duration_str = duration_ms
                        .map(|ms| format!(" ({}ms)", ms))
                        .unwrap_or_default();
                    lines.push(ContentLine::Task {
                        text: format!("{} {}{}", status.icon(), task, duration_str),
                        status: *status,
                    });

                    if !collapsed {
                        for step in steps {
                            // Main step line
                            let step_icon = step.kind.icon();
                            let step_label = step.kind.label();
                            lines.push(ContentLine::Step {
                                text: format!(
                                    "  {} {} {} {}",
                                    step.status.icon(),
                                    step_icon,
                                    step_label,
                                    step.description
                                ),
                                status: step.status,
                            });

                            // Output (if present and step not collapsed)
                            if !step.collapsed {
                                if let Some(ref output) = step.output {
                                    for line in output.lines().take(5) {
                                        lines.push(ContentLine::Output(format!("     {}", line)));
                                    }
                                    // Indicate if truncated
                                    if output.lines().count() > 5 {
                                        lines.push(ContentLine::Output(
                                            "     ... (output truncated)".to_string(),
                                        ));
                                    }
                                }

                                // Sub-steps
                                for sub_step in &step.sub_steps {
                                    let sub_icon = sub_step.kind.icon();
                                    lines.push(ContentLine::Step {
                                        text: format!(
                                            "    {} {} {}",
                                            sub_step.status.icon(),
                                            sub_icon,
                                            sub_step.description
                                        ),
                                        status: sub_step.status,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        // Apply scrolling (from bottom)
        let total = lines.len();
        let start = total.saturating_sub(self.visible_height + self.scroll_offset);
        let end = total.saturating_sub(self.scroll_offset);

        lines[start..end].to_vec()
    }
}

/// A line of content for rendering
#[derive(Debug, Clone)]
pub enum ContentLine {
    Command {
        text: String,
        exit_code: i32,
    },
    Output(String),
    Task {
        text: String,
        status: super::TaskStatus,
    },
    Step {
        text: String,
        status: super::TaskStatus,
    },
}

impl Default for RawContent {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_and_scroll() {
        let mut content = RawContent::new();
        content.push(ContentBlock::shell(
            "ls".to_string(),
            b"file1\nfile2\n".to_vec(),
            0,
        ));

        assert_eq!(content.blocks().len(), 1);
        assert_eq!(content.scroll_offset(), 0);
    }

    #[test]
    fn test_visible_lines() {
        let mut content = RawContent::new();
        content.set_visible_height(10);
        content.push(ContentBlock::shell(
            "echo hi".to_string(),
            b"hi\n".to_vec(),
            0,
        ));

        let lines = content.visible_lines();
        assert!(!lines.is_empty());
    }
}
