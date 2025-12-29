//! Tool registry and classification

use claude_sdk::Tool;
use serde_json::Value;
use std::collections::HashMap;

/// Tool execution classification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolClassification {
    /// Auto-execute without user approval (disable_user_input: true)
    Programmatic,
    /// Require user approval via terminal (disable_user_input: false)
    UserFacing,
}

/// Registered tool with execution handler
pub struct RegisteredTool {
    /// Claude API tool definition
    pub definition: Tool,
    /// Classification for execution
    pub classification: ToolClassification,
    /// Execution handler
    pub handler: Box<dyn ToolHandler>,
}

/// Tool execution handler trait
pub trait ToolHandler: Send + Sync {
    fn execute(&self, input: Value) -> Result<Value, ToolExecutionError>;
}

/// Tool execution errors
#[derive(Debug, thiserror::Error)]
pub enum ToolExecutionError {
    #[error("tool not found: {0}")]
    NotFound(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("execution failed: {0}")]
    ExecutionFailed(String),

    #[error("user denied: {0}")]
    UserDenied(String),
}

/// Tool registry managing all available tools
pub struct ToolRegistry {
    tools: HashMap<String, RegisteredTool>,
}

impl ToolRegistry {
    /// Create new empty registry
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool
    pub fn register(&mut self, name: String, tool: RegisteredTool) {
        self.tools.insert(name, tool);
    }

    /// Get tool by name
    pub fn get(&self, name: &str) -> Option<&RegisteredTool> {
        self.tools.get(name)
    }

    /// Get all tool definitions for Claude API
    pub fn definitions(&self) -> Vec<Tool> {
        self.tools
            .values()
            .map(|rt| {
                let mut tool = rt.definition.clone();
                // Set disable_user_input based on classification
                tool.disable_user_input =
                    Some(rt.classification == ToolClassification::Programmatic);
                tool
            })
            .collect()
    }

    /// Execute a tool by name
    pub fn execute(&self, name: &str, input: Value) -> Result<Value, ToolExecutionError> {
        let tool = self
            .get(name)
            .ok_or_else(|| ToolExecutionError::NotFound(name.to_string()))?;

        tool.handler.execute(input)
    }

    /// Get classification for a tool
    pub fn classification(&self, name: &str) -> Option<ToolClassification> {
        self.get(name).map(|t| t.classification)
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
