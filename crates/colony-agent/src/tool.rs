//! Tool execution framework with registry, classification, and built-in tools

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
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

/// Output from a tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    pub success: bool,
    pub data: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ToolOutput {
    pub fn success(data: Value) -> Self {
        Self {
            success: true,
            data,
            error: None,
        }
    }

    pub fn failure(error: impl Into<String>) -> Self {
        Self {
            success: false,
            data: Value::Null,
            error: Some(error.into()),
        }
    }
}

/// Parameter definition for tool input validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolParam {
    pub name: String,
    pub param_type: ParamType,
    pub required: bool,
    pub description: String,
}

/// Supported parameter types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParamType {
    String,
    Integer,
    Number,
    Boolean,
    Array,
    Object,
}

/// Tool trait for implementing executable tools
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> Vec<ToolParam>;
    async fn execute(&self, params: Value) -> Result<ToolOutput, ToolExecutionError>;
}

/// Registered tool with execution handler and classification
pub struct RegisteredTool {
    pub definition: claude_sdk::Tool,
    pub classification: ToolClassification,
    pub handler: Box<dyn Tool>,
}

/// Tool registry managing all available tools
pub struct ToolRegistry {
    tools: HashMap<String, RegisteredTool>,
    auto_approve_patterns: Vec<String>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            auto_approve_patterns: Vec::new(),
        }
    }

    /// Create a registry pre-loaded with built-in tools
    pub fn with_builtins() -> Self {
        let mut registry = Self::new();
        registry.register_tool(
            Box::new(builtins::ShellExec),
            ToolClassification::UserFacing,
        );
        registry.register_tool(
            Box::new(builtins::FileRead),
            ToolClassification::Programmatic,
        );
        registry.register_tool(
            Box::new(builtins::FileWrite),
            ToolClassification::UserFacing,
        );
        registry.register_tool(
            Box::new(builtins::Search),
            ToolClassification::Programmatic,
        );
        registry
    }

    /// Register a tool from a Tool trait object
    pub fn register_tool(
        &mut self,
        tool: Box<dyn Tool>,
        classification: ToolClassification,
    ) {
        let schema = build_input_schema(&tool.parameters());
        let definition = claude_sdk::Tool {
            name: tool.name().to_string(),
            description: tool.description().to_string(),
            input_schema: schema,
            cache_control: None,
            disable_user_input: Some(classification == ToolClassification::Programmatic),
            input_examples: Some(vec![]),
        };
        let name = tool.name().to_string();
        self.tools.insert(
            name,
            RegisteredTool {
                definition,
                classification,
                handler: tool,
            },
        );
    }

    /// Register a tool with a raw RegisteredTool
    pub fn register(&mut self, name: String, tool: RegisteredTool) {
        self.tools.insert(name, tool);
    }

    /// Set auto-approve patterns (tool names that skip approval)
    pub fn set_auto_approve(&mut self, patterns: Vec<String>) {
        self.auto_approve_patterns = patterns;
    }

    /// Check if a tool is auto-approved
    pub fn is_auto_approved(&self, tool_name: &str) -> bool {
        self.auto_approve_patterns
            .iter()
            .any(|p| p == tool_name || p == "*")
    }

    /// Get tool by name
    pub fn get(&self, name: &str) -> Option<&RegisteredTool> {
        self.tools.get(name)
    }

    /// Get all tool definitions for Claude API
    pub fn definitions(&self) -> Vec<claude_sdk::Tool> {
        self.tools
            .values()
            .map(|rt| {
                let mut tool = rt.definition.clone();
                tool.disable_user_input =
                    Some(rt.classification == ToolClassification::Programmatic);
                tool
            })
            .collect()
    }

    /// Validate parameters before execution
    pub fn validate_params(
        &self,
        tool_name: &str,
        params: &Value,
    ) -> Result<(), ToolExecutionError> {
        let tool = self
            .get(tool_name)
            .ok_or_else(|| ToolExecutionError::NotFound(tool_name.to_string()))?;

        let param_defs = tool.handler.parameters();
        let obj = params.as_object();

        for param in &param_defs {
            if param.required {
                let has_value = obj
                    .and_then(|o| o.get(&param.name))
                    .map(|v| !v.is_null())
                    .unwrap_or(false);

                if !has_value {
                    return Err(ToolExecutionError::InvalidInput(format!(
                        "missing required parameter: {}",
                        param.name
                    )));
                }
            }
        }

        Ok(())
    }

    /// Execute a tool by name with parameter validation
    pub async fn execute(
        &self,
        name: &str,
        input: Value,
    ) -> Result<ToolOutput, ToolExecutionError> {
        self.validate_params(name, &input)?;

        let tool = self
            .get(name)
            .ok_or_else(|| ToolExecutionError::NotFound(name.to_string()))?;

        tool.handler.execute(input).await
    }

    /// Get classification for a tool
    pub fn classification(&self, name: &str) -> Option<ToolClassification> {
        self.get(name).map(|t| t.classification)
    }

    /// List all registered tool names
    pub fn tool_names(&self) -> Vec<&str> {
        self.tools.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Build JSON Schema from parameter definitions
fn build_input_schema(params: &[ToolParam]) -> Value {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();

    for param in params {
        let type_str = match param.param_type {
            ParamType::String => "string",
            ParamType::Integer => "integer",
            ParamType::Number => "number",
            ParamType::Boolean => "boolean",
            ParamType::Array => "array",
            ParamType::Object => "object",
        };

        properties.insert(
            param.name.clone(),
            serde_json::json!({
                "type": type_str,
                "description": param.description,
            }),
        );

        if param.required {
            required.push(Value::String(param.name.clone()));
        }
    }

    serde_json::json!({
        "type": "object",
        "properties": properties,
        "required": required,
    })
}

/// Built-in tool implementations
pub mod builtins {
    use super::*;

    /// Execute a shell command
    pub struct ShellExec;

    #[async_trait]
    impl Tool for ShellExec {
        fn name(&self) -> &str {
            "shell_exec"
        }

        fn description(&self) -> &str {
            "Execute a shell command and return its output"
        }

        fn parameters(&self) -> Vec<ToolParam> {
            vec![
                ToolParam {
                    name: "command".to_string(),
                    param_type: ParamType::String,
                    required: true,
                    description: "The shell command to execute".to_string(),
                },
                ToolParam {
                    name: "working_dir".to_string(),
                    param_type: ParamType::String,
                    required: false,
                    description: "Working directory for the command".to_string(),
                },
            ]
        }

        async fn execute(&self, params: Value) -> Result<ToolOutput, ToolExecutionError> {
            let command = params
                .get("command")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    ToolExecutionError::InvalidInput("missing 'command' parameter".to_string())
                })?;

            let working_dir = params
                .get("working_dir")
                .and_then(|v| v.as_str());

            let mut cmd = tokio::process::Command::new("sh");
            cmd.arg("-c").arg(command);

            if let Some(dir) = working_dir {
                cmd.current_dir(dir);
            }

            let output = cmd.output().await.map_err(|e| {
                ToolExecutionError::ExecutionFailed(format!("command execution failed: {}", e))
            })?;

            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let exit_code = output.status.code().unwrap_or(-1);

            Ok(ToolOutput::success(serde_json::json!({
                "stdout": stdout,
                "stderr": stderr,
                "exit_code": exit_code,
            })))
        }
    }

    /// Read a file
    pub struct FileRead;

    #[async_trait]
    impl Tool for FileRead {
        fn name(&self) -> &str {
            "file_read"
        }

        fn description(&self) -> &str {
            "Read the contents of a file"
        }

        fn parameters(&self) -> Vec<ToolParam> {
            vec![ToolParam {
                name: "path".to_string(),
                param_type: ParamType::String,
                required: true,
                description: "Path to the file to read".to_string(),
            }]
        }

        async fn execute(&self, params: Value) -> Result<ToolOutput, ToolExecutionError> {
            let path = params
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    ToolExecutionError::InvalidInput("missing 'path' parameter".to_string())
                })?;

            let content = tokio::fs::read_to_string(path).await.map_err(|e| {
                ToolExecutionError::ExecutionFailed(format!("failed to read file '{}': {}", path, e))
            })?;

            Ok(ToolOutput::success(serde_json::json!({
                "content": content,
                "path": path,
                "size": content.len(),
            })))
        }
    }

    /// Write a file
    pub struct FileWrite;

    #[async_trait]
    impl Tool for FileWrite {
        fn name(&self) -> &str {
            "file_write"
        }

        fn description(&self) -> &str {
            "Write content to a file, creating it if it doesn't exist"
        }

        fn parameters(&self) -> Vec<ToolParam> {
            vec![
                ToolParam {
                    name: "path".to_string(),
                    param_type: ParamType::String,
                    required: true,
                    description: "Path to the file to write".to_string(),
                },
                ToolParam {
                    name: "content".to_string(),
                    param_type: ParamType::String,
                    required: true,
                    description: "Content to write to the file".to_string(),
                },
            ]
        }

        async fn execute(&self, params: Value) -> Result<ToolOutput, ToolExecutionError> {
            let path = params
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    ToolExecutionError::InvalidInput("missing 'path' parameter".to_string())
                })?;

            let content = params
                .get("content")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    ToolExecutionError::InvalidInput("missing 'content' parameter".to_string())
                })?;

            tokio::fs::write(path, content).await.map_err(|e| {
                ToolExecutionError::ExecutionFailed(format!(
                    "failed to write file '{}': {}",
                    path, e
                ))
            })?;

            Ok(ToolOutput::success(serde_json::json!({
                "path": path,
                "bytes_written": content.len(),
            })))
        }
    }

    /// Search for files or content
    pub struct Search;

    #[async_trait]
    impl Tool for Search {
        fn name(&self) -> &str {
            "search"
        }

        fn description(&self) -> &str {
            "Search for files by name pattern or content using grep"
        }

        fn parameters(&self) -> Vec<ToolParam> {
            vec![
                ToolParam {
                    name: "pattern".to_string(),
                    param_type: ParamType::String,
                    required: true,
                    description: "Search pattern (regex for content, glob for files)".to_string(),
                },
                ToolParam {
                    name: "path".to_string(),
                    param_type: ParamType::String,
                    required: false,
                    description: "Directory to search in (defaults to current directory)"
                        .to_string(),
                },
                ToolParam {
                    name: "mode".to_string(),
                    param_type: ParamType::String,
                    required: false,
                    description: "Search mode: 'content' (grep) or 'files' (find). Defaults to 'content'"
                        .to_string(),
                },
            ]
        }

        async fn execute(&self, params: Value) -> Result<ToolOutput, ToolExecutionError> {
            let pattern = params
                .get("pattern")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    ToolExecutionError::InvalidInput("missing 'pattern' parameter".to_string())
                })?;

            let path = params
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or(".");

            let mode = params
                .get("mode")
                .and_then(|v| v.as_str())
                .unwrap_or("content");

            let command = match mode {
                "files" => format!("find {} -name '{}' 2>/dev/null | head -50", path, pattern),
                _ => format!(
                    "grep -rn '{}' {} 2>/dev/null | head -50",
                    pattern, path
                ),
            };

            let output = tokio::process::Command::new("sh")
                .arg("-c")
                .arg(&command)
                .output()
                .await
                .map_err(|e| {
                    ToolExecutionError::ExecutionFailed(format!("search failed: {}", e))
                })?;

            let results = String::from_utf8_lossy(&output.stdout).to_string();
            let matches: Vec<&str> = results.lines().collect();

            Ok(ToolOutput::success(serde_json::json!({
                "matches": matches,
                "count": matches.len(),
                "pattern": pattern,
                "mode": mode,
            })))
        }
    }
}

/// Sync wrapper for backward compatibility with existing ToolHandler trait users
pub trait ToolHandler: Send + Sync {
    fn execute(&self, input: Value) -> Result<Value, ToolExecutionError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_output_success() {
        let output = ToolOutput::success(serde_json::json!({"result": "ok"}));
        assert!(output.success);
        assert!(output.error.is_none());
    }

    #[test]
    fn test_tool_output_failure() {
        let output = ToolOutput::failure("something broke");
        assert!(!output.success);
        assert_eq!(output.error.as_deref(), Some("something broke"));
    }

    #[test]
    fn test_registry_with_builtins() {
        let registry = ToolRegistry::with_builtins();
        assert!(registry.get("shell_exec").is_some());
        assert!(registry.get("file_read").is_some());
        assert!(registry.get("file_write").is_some());
        assert!(registry.get("search").is_some());
    }

    #[test]
    fn test_registry_classification() {
        let registry = ToolRegistry::with_builtins();
        assert_eq!(
            registry.classification("file_read"),
            Some(ToolClassification::Programmatic)
        );
        assert_eq!(
            registry.classification("shell_exec"),
            Some(ToolClassification::UserFacing)
        );
        assert_eq!(
            registry.classification("file_write"),
            Some(ToolClassification::UserFacing)
        );
        assert_eq!(
            registry.classification("search"),
            Some(ToolClassification::Programmatic)
        );
    }

    #[test]
    fn test_auto_approve() {
        let mut registry = ToolRegistry::with_builtins();
        assert!(!registry.is_auto_approved("file_read"));

        registry.set_auto_approve(vec!["file_read".to_string()]);
        assert!(registry.is_auto_approved("file_read"));
        assert!(!registry.is_auto_approved("shell_exec"));
    }

    #[test]
    fn test_auto_approve_wildcard() {
        let mut registry = ToolRegistry::with_builtins();
        registry.set_auto_approve(vec!["*".to_string()]);
        assert!(registry.is_auto_approved("file_read"));
        assert!(registry.is_auto_approved("shell_exec"));
    }

    #[test]
    fn test_validate_params_missing_required() {
        let registry = ToolRegistry::with_builtins();
        let result = registry.validate_params("file_read", &serde_json::json!({}));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("missing required parameter"));
    }

    #[test]
    fn test_validate_params_ok() {
        let registry = ToolRegistry::with_builtins();
        let result =
            registry.validate_params("file_read", &serde_json::json!({"path": "/tmp/test"}));
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_params_unknown_tool() {
        let registry = ToolRegistry::with_builtins();
        let result = registry.validate_params("nonexistent", &serde_json::json!({}));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("tool not found"));
    }

    #[test]
    fn test_build_input_schema() {
        let params = vec![
            ToolParam {
                name: "path".to_string(),
                param_type: ParamType::String,
                required: true,
                description: "File path".to_string(),
            },
            ToolParam {
                name: "recursive".to_string(),
                param_type: ParamType::Boolean,
                required: false,
                description: "Recurse".to_string(),
            },
        ];

        let schema = build_input_schema(&params);
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["properties"]["path"]["type"], "string");
        assert_eq!(schema["properties"]["recursive"]["type"], "boolean");
        assert_eq!(schema["required"][0], "path");
    }

    #[test]
    fn test_definitions_list() {
        let registry = ToolRegistry::with_builtins();
        let defs = registry.definitions();
        assert_eq!(defs.len(), 4);
        let names: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"shell_exec"));
        assert!(names.contains(&"file_read"));
        assert!(names.contains(&"file_write"));
        assert!(names.contains(&"search"));
    }

    #[test]
    fn test_tool_names() {
        let registry = ToolRegistry::with_builtins();
        let names = registry.tool_names();
        assert_eq!(names.len(), 4);
    }

    #[tokio::test]
    async fn test_file_read_execute() {
        let tool = builtins::FileRead;
        let result = tool
            .execute(serde_json::json!({"path": "/tmp/nonexistent_test_file_12345"}))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_shell_exec_execute() {
        let tool = builtins::ShellExec;
        let result = tool
            .execute(serde_json::json!({"command": "echo hello"}))
            .await
            .unwrap();
        assert!(result.success);
        assert_eq!(result.data["stdout"], "hello\n");
        assert_eq!(result.data["exit_code"], 0);
    }

    #[tokio::test]
    async fn test_search_execute() {
        let tool = builtins::Search;
        let result = tool
            .execute(serde_json::json!({"pattern": "test", "path": "/dev/null", "mode": "content"}))
            .await
            .unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_registry_execute() {
        let registry = ToolRegistry::with_builtins();
        let result = registry
            .execute("shell_exec", serde_json::json!({"command": "echo test"}))
            .await
            .unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_registry_execute_not_found() {
        let registry = ToolRegistry::with_builtins();
        let result = registry
            .execute("nonexistent", serde_json::json!({}))
            .await;
        assert!(result.is_err());
    }
}
