//! Prompt layout configuration

/// Layout configuration for prompt components
#[derive(Debug, Clone)]
pub struct PromptLayout {
    /// Components on the left side of the prompt
    pub left: Vec<String>,
    /// Components on the right side of the prompt
    pub right: Vec<String>,
    /// Gap between components
    pub gap: String,
}

impl Default for PromptLayout {
    fn default() -> Self {
        Self {
            left: vec![
                "pwd".to_string(),
                "git_branch".to_string(),
                "git_status".to_string(),
            ],
            right: vec![
                "model".to_string(),
                "jobs".to_string(),
                "exit_status".to_string(),
            ],
            gap: "  ".to_string(),
        }
    }
}
