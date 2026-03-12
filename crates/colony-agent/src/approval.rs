//! Tool approval UI via OSC 2337 Widget=approval
//!
//! Before executing tools that require user approval, this module:
//! 1. Checks if the tool is auto-approved via configured patterns
//! 2. Sends an OSC 2337 Widget=approval escape sequence for colony terminals
//! 3. Falls back to stderr prompt for non-colony terminals
//! 4. Returns the user's approval decision

use crate::tool::ToolClassification;
use crate::AgentError;
use colony_protocol::{emit_widget_request, ProtocolMessage};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex};

/// Tool approval response from user/terminal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApprovalResponse {
    Approved,
    Modified(serde_json::Value),
    Denied(String),
}

/// Configuration for auto-approve patterns
#[derive(Debug, Clone, Default)]
pub struct ApprovalConfig {
    pub auto_approve_patterns: Vec<String>,
    pub colony_terminal: bool,
}


impl ApprovalConfig {
    pub fn with_auto_approve(mut self, patterns: Vec<String>) -> Self {
        self.auto_approve_patterns = patterns;
        self
    }

    pub fn with_colony_terminal(mut self, detected: bool) -> Self {
        self.colony_terminal = detected;
        self
    }
}

/// Pending approval senders keyed by component_id.
pub type PendingApprovals = Arc<Mutex<HashMap<String, oneshot::Sender<bool>>>>;

/// Tool approval manager
pub struct ApprovalManager {
    config: ApprovalConfig,
    pending_approvals: Option<PendingApprovals>,
}

impl ApprovalManager {
    pub fn new(config: ApprovalConfig) -> Self {
        Self {
            config,
            pending_approvals: None,
        }
    }

    /// Create an `ApprovalManager` wired to the widget server's approval channel.
    ///
    /// When a `WidgetResponse` arrives on the Unix socket, the widget server
    /// completes the oneshot sender stored in `pending`. This lets
    /// `request_osc_approval` block until the user responds in colony-nerve.
    pub fn with_approvals(config: ApprovalConfig, pending: PendingApprovals) -> Self {
        Self {
            config,
            pending_approvals: Some(pending),
        }
    }

    /// Check if approval is needed for a tool call
    pub fn needs_approval(
        &self,
        tool_name: &str,
        classification: ToolClassification,
    ) -> bool {
        if classification == ToolClassification::Programmatic {
            return false;
        }

        !self.is_auto_approved(tool_name)
    }

    /// Check if a tool matches an auto-approve pattern
    fn is_auto_approved(&self, tool_name: &str) -> bool {
        self.config
            .auto_approve_patterns
            .iter()
            .any(|p| p == tool_name || p == "*")
    }

    /// Request approval for a tool execution
    pub async fn request_approval(
        &self,
        tool_name: &str,
        input: &serde_json::Value,
    ) -> Result<ApprovalResponse, AgentError> {
        if self.config.colony_terminal {
            self.request_osc_approval(tool_name, input).await
        } else {
            self.request_stderr_approval(tool_name, input).await
        }
    }

    /// Request approval via OSC 2337 Widget=approval escape sequence.
    ///
    /// If a `pending_approvals` channel is wired (via `with_approvals`), this
    /// method blocks until the terminal responds or a 60-second timeout expires.
    /// Without a channel it falls back to auto-approve with a warning.
    async fn request_osc_approval(
        &self,
        tool_name: &str,
        input: &serde_json::Value,
    ) -> Result<ApprovalResponse, AgentError> {
        let component_id = uuid::Uuid::new_v4().to_string();

        let approval_data = serde_json::json!({
            "tool": tool_name,
            "input": input,
            "component_id": &component_id,
        });

        emit_widget_request("approval", approval_data);

        // If we have the approval channel, wait for the terminal response.
        let Some(pending) = &self.pending_approvals else {
            tracing::warn!(
                "No approval channel — auto-approving '{}'",
                tool_name
            );
            return Ok(ApprovalResponse::Approved);
        };

        let (tx, rx) = oneshot::channel();
        {
            let mut map = pending.lock().await;
            map.insert(component_id.clone(), tx);
        }

        match tokio::time::timeout(std::time::Duration::from_secs(60), rx).await {
            Ok(Ok(true)) => {
                tracing::info!("Tool '{}' approved by user", tool_name);
                Ok(ApprovalResponse::Approved)
            }
            Ok(Ok(false)) => {
                tracing::info!("Tool '{}' denied by user", tool_name);
                Ok(ApprovalResponse::Denied("denied by user".to_string()))
            }
            Ok(Err(_)) => {
                tracing::warn!("Approval channel closed for '{}'", tool_name);
                Ok(ApprovalResponse::Denied(
                    "approval channel closed".to_string(),
                ))
            }
            Err(_) => {
                // Timeout — clean up the pending entry
                let mut map = pending.lock().await;
                map.remove(&component_id);
                tracing::warn!("Approval timed out for '{}'", tool_name);
                Ok(ApprovalResponse::Denied(
                    "approval timed out after 60s".to_string(),
                ))
            }
        }
    }

    /// Fallback: request approval via stderr prompt for non-colony terminals
    async fn request_stderr_approval(
        &self,
        tool_name: &str,
        input: &serde_json::Value,
    ) -> Result<ApprovalResponse, AgentError> {
        let input_preview = serde_json::to_string_pretty(input)
            .unwrap_or_else(|_| format!("{:?}", input));

        let preview = if input_preview.len() > 200 {
            format!("{}...", &input_preview[..200])
        } else {
            input_preview
        };

        eprint!(
            "\n[colony] Tool '{}' wants to execute:\n{}\nApprove? [y/N] ",
            tool_name, preview
        );
        std::io::stderr()
            .flush()
            .map_err(|e| AgentError::Protocol(format!("stderr flush failed: {}", e)))?;

        let mut response = String::new();
        std::io::stdin()
            .read_line(&mut response)
            .map_err(|e| AgentError::Protocol(format!("stdin read failed: {}", e)))?;

        let trimmed = response.trim().to_lowercase();
        if trimmed == "y" || trimmed == "yes" {
            Ok(ApprovalResponse::Approved)
        } else {
            Ok(ApprovalResponse::Denied(
                "user denied via terminal prompt".to_string(),
            ))
        }
    }

    /// Create an `ApprovalManager` that auto-detects the terminal environment.
    ///
    /// Reads `COLONY_TERMINAL` at call time. Prefer this over `Default` when
    /// the runtime environment is meaningful (i.e. not tests).
    pub fn from_env() -> Self {
        let config = ApprovalConfig::default().with_colony_terminal(Self::detect_colony_terminal());
        Self::new(config)
    }

    /// Detect if the terminal is a colony terminal (supports OSC 2337)
    pub fn detect_colony_terminal() -> bool {
        std::env::var("COLONY_TERMINAL").is_ok()
    }
}

impl Default for ApprovalManager {
    fn default() -> Self {
        Self::new(ApprovalConfig::default())
    }
}

/// Parse an approval response from OSC 2337 JSON payload
pub fn parse_approval_response(json: &str) -> Option<ApprovalResponse> {
    if let Ok(ProtocolMessage::WidgetAction {
        widget,
        action,
        payload,
    }) = serde_json::from_str::<ProtocolMessage>(json)
    {
        if widget != "approval" {
            return None;
        }

        return match action.as_str() {
            "approve" => Some(ApprovalResponse::Approved),
            "modify" => Some(ApprovalResponse::Modified(payload)),
            "deny" => {
                let reason = payload
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("denied by user")
                    .to_string();
                Some(ApprovalResponse::Denied(reason))
            }
            _ => None,
        };
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_approval_config_defaults() {
        let config = ApprovalConfig::default();
        assert!(config.auto_approve_patterns.is_empty());
        assert!(!config.colony_terminal);
    }

    #[test]
    fn test_needs_approval_programmatic() {
        let manager = ApprovalManager::new(ApprovalConfig::default());
        assert!(!manager.needs_approval("file_read", ToolClassification::Programmatic));
    }

    #[test]
    fn test_needs_approval_user_facing() {
        let manager = ApprovalManager::new(ApprovalConfig::default());
        assert!(manager.needs_approval("shell_exec", ToolClassification::UserFacing));
    }

    #[test]
    fn test_auto_approve_pattern() {
        let config = ApprovalConfig::default()
            .with_auto_approve(vec!["shell_exec".to_string()]);
        let manager = ApprovalManager::new(config);
        assert!(!manager.needs_approval("shell_exec", ToolClassification::UserFacing));
        assert!(manager.needs_approval("file_write", ToolClassification::UserFacing));
    }

    #[test]
    fn test_auto_approve_wildcard() {
        let config = ApprovalConfig::default()
            .with_auto_approve(vec!["*".to_string()]);
        let manager = ApprovalManager::new(config);
        assert!(!manager.needs_approval("shell_exec", ToolClassification::UserFacing));
        assert!(!manager.needs_approval("file_write", ToolClassification::UserFacing));
    }

    #[test]
    fn test_parse_approval_response_approve() {
        let json = r#"{"type":"WidgetAction","widget":"approval","action":"approve","payload":{}}"#;
        let result = parse_approval_response(json);
        assert!(matches!(result, Some(ApprovalResponse::Approved)));
    }

    #[test]
    fn test_parse_approval_response_deny() {
        let json = r#"{"type":"WidgetAction","widget":"approval","action":"deny","payload":{"reason":"too dangerous"}}"#;
        let result = parse_approval_response(json);
        assert!(matches!(result, Some(ApprovalResponse::Denied(r)) if r == "too dangerous"));
    }

    #[test]
    fn test_parse_approval_response_modify() {
        let json = r#"{"type":"WidgetAction","widget":"approval","action":"modify","payload":{"command":"ls"}}"#;
        let result = parse_approval_response(json);
        assert!(matches!(result, Some(ApprovalResponse::Modified(_))));
    }

    #[test]
    fn test_parse_approval_response_wrong_widget() {
        let json = r#"{"type":"WidgetAction","widget":"other","action":"approve","payload":{}}"#;
        let result = parse_approval_response(json);
        assert!(result.is_none());
    }

    #[test]
    fn test_colony_terminal_config() {
        let config = ApprovalConfig::default()
            .with_colony_terminal(true);
        assert!(config.colony_terminal);
    }
}
