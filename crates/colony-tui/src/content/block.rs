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
    },
}

/// A step in an agent task
#[derive(Debug, Clone)]
pub struct AgentStep {
    pub kind: StepKind,
    pub description: String,
    pub status: TaskStatus,
    pub output: Option<String>,
}

/// Kind of agent step
#[derive(Debug, Clone)]
pub enum StepKind {
    Read,
    Edit,
    Write,
    Bash,
    Search,
    Think,
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

impl TaskStatus {
    pub fn icon(&self) -> &'static str {
        match self {
            TaskStatus::Pending => " ",
            TaskStatus::InProgress => " ",
            TaskStatus::Complete => " ",
            TaskStatus::Failed => " ",
            TaskStatus::Cancelled => " ",
        }
    }
}
