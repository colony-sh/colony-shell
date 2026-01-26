//! Content block types

/// A block of content in the output area
#[derive(Debug, Clone)]
pub enum ContentBlock {
    /// Raw shell output
    Shell {
        command: String,
        output: Vec<u8>,
        exit_code: i32,
        collapsed: bool,
    },
    /// Agent task with steps
    Agent {
        task: String,
        steps: Vec<AgentStep>,
        status: TaskStatus,
        collapsed: bool,
        /// Duration in milliseconds
        duration_ms: Option<u64>,
    },
}

/// A step in an agent task
#[derive(Debug, Clone)]
pub struct AgentStep {
    pub kind: StepKind,
    pub description: String,
    pub status: TaskStatus,
    pub output: Option<String>,
    /// Nested sub-steps (for tool calls with multiple operations)
    pub sub_steps: Vec<AgentStep>,
    /// Whether sub-steps are collapsed
    pub collapsed: bool,
}

/// Kind of agent step
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepKind {
    /// Reading a file
    Read,
    /// Editing a file
    Edit,
    /// Writing a new file
    Write,
    /// Running a bash command
    Bash,
    /// Searching/grep
    Search,
    /// Agent thinking/reasoning
    Think,
    /// Tool call (MCP or builtin)
    Tool(String),
    /// Agent response/message
    Response,
    /// Error occurred
    Error,
    /// Other kind
    Other(String),
}

/// Status of a task or step
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    InProgress,
    Complete,
    Failed,
    Cancelled,
}

impl ContentBlock {
    /// Create a new shell output block
    pub fn shell(command: String, output: Vec<u8>, exit_code: i32) -> Self {
        Self::Shell {
            command,
            output,
            exit_code,
            collapsed: false,
        }
    }

    /// Create a new agent task block
    pub fn agent(task: String) -> Self {
        Self::Agent {
            task,
            steps: Vec::new(),
            status: TaskStatus::InProgress,
            collapsed: false,
            duration_ms: None,
        }
    }

    /// Add a step to an agent task
    pub fn add_step(&mut self, step: AgentStep) {
        if let Self::Agent { steps, .. } = self {
            steps.push(step);
        }
    }

    /// Update the status of an agent task
    pub fn set_status(&mut self, new_status: TaskStatus) {
        if let Self::Agent { status, .. } = self {
            *status = new_status;
        }
    }

    /// Set the duration for an agent task
    pub fn set_duration(&mut self, ms: u64) {
        if let Self::Agent { duration_ms, .. } = self {
            *duration_ms = Some(ms);
        }
    }

    /// Get steps (for agent blocks)
    pub fn steps(&self) -> Option<&[AgentStep]> {
        match self {
            Self::Agent { steps, .. } => Some(steps),
            _ => None,
        }
    }

    /// Get mutable steps (for agent blocks)
    pub fn steps_mut(&mut self) -> Option<&mut Vec<AgentStep>> {
        match self {
            Self::Agent { steps, .. } => Some(steps),
            _ => None,
        }
    }

    /// Toggle collapsed state
    pub fn toggle_collapsed(&mut self) {
        match self {
            Self::Shell { collapsed, .. } => *collapsed = !*collapsed,
            Self::Agent { collapsed, .. } => *collapsed = !*collapsed,
        }
    }

    /// Check if collapsed
    pub fn is_collapsed(&self) -> bool {
        match self {
            Self::Shell { collapsed, .. } => *collapsed,
            Self::Agent { collapsed, .. } => *collapsed,
        }
    }
}

impl AgentStep {
    /// Create a new agent step
    pub fn new(kind: StepKind, description: impl Into<String>) -> Self {
        Self {
            kind,
            description: description.into(),
            status: TaskStatus::Pending,
            output: None,
            sub_steps: Vec::new(),
            collapsed: true,
        }
    }

    /// Create a step with status
    pub fn with_status(mut self, status: TaskStatus) -> Self {
        self.status = status;
        self
    }

    /// Create a step with output
    pub fn with_output(mut self, output: impl Into<String>) -> Self {
        self.output = Some(output.into());
        self
    }

    /// Add a sub-step
    pub fn add_sub_step(&mut self, step: AgentStep) {
        self.sub_steps.push(step);
    }

    /// Toggle collapsed state of sub-steps
    pub fn toggle_collapsed(&mut self) {
        self.collapsed = !self.collapsed;
    }
}

impl StepKind {
    /// Get an icon for the step kind
    pub fn icon(&self) -> &'static str {
        match self {
            StepKind::Read => "",
            StepKind::Edit => "",
            StepKind::Write => "",
            StepKind::Bash => "",
            StepKind::Search => "",
            StepKind::Think => "",
            StepKind::Tool(_) => "",
            StepKind::Response => "",
            StepKind::Error => "",
            StepKind::Other(_) => "",
        }
    }

    /// Get a label for the step kind
    pub fn label(&self) -> &str {
        match self {
            StepKind::Read => "Read",
            StepKind::Edit => "Edit",
            StepKind::Write => "Write",
            StepKind::Bash => "Bash",
            StepKind::Search => "Search",
            StepKind::Think => "Think",
            StepKind::Tool(name) => name,
            StepKind::Response => "Response",
            StepKind::Error => "Error",
            StepKind::Other(name) => name,
        }
    }
}

impl TaskStatus {
    pub fn icon(&self) -> &'static str {
        match self {
            TaskStatus::Pending => "",
            TaskStatus::InProgress => "",
            TaskStatus::Complete => "",
            TaskStatus::Failed => "",
            TaskStatus::Cancelled => "",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_block_steps() {
        let mut block = ContentBlock::agent("Test task".to_string());

        block.add_step(AgentStep::new(StepKind::Read, "file.rs"));
        block.add_step(AgentStep::new(StepKind::Edit, "Updated function"));

        assert_eq!(block.steps().unwrap().len(), 2);
    }

    #[test]
    fn test_step_with_output() {
        let step = AgentStep::new(StepKind::Bash, "cargo test")
            .with_status(TaskStatus::Complete)
            .with_output("All tests passed");

        assert_eq!(step.status, TaskStatus::Complete);
        assert_eq!(step.output, Some("All tests passed".to_string()));
    }

    #[test]
    fn test_nested_steps() {
        let mut parent =
            AgentStep::new(StepKind::Tool("mcp_task".to_string()), "Research codebase");
        parent.add_sub_step(AgentStep::new(StepKind::Search, "Find main.rs"));
        parent.add_sub_step(AgentStep::new(StepKind::Read, "src/lib.rs"));

        assert_eq!(parent.sub_steps.len(), 2);
    }
}
