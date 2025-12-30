//! OSC 2337 protocol for colony-terminal communication
//!
//! Colony Shell communicates with colony-terminal using OSC (Operating System Command)
//! escape sequences. The format is:
//!
//! ```text
//! ESC ] 2337 ; <JSON_payload> BEL
//! \x1b]2337;<json>\x07
//! ```
//!
//! The JSON payload contains a ProtocolMessage which the terminal parses and routes
//! to the appropriate handler.
//!
//! ## Message Types
//!
//! - **Agent**: Agent status updates ("thinking", "streaming", "idle")
//! - **Widget**: Interactive widgets (tool approval, file picker, etc.)
//! - **Output**: Output metadata (syntax highlighting, hyperlinks)
//! - **Memory**: Memory operations (store, retrieve, search)
//! - **Session**: Session management (checkpoint, resume)
//! - **SecureAccess**: Secure environment variable access request
//! - **SecureAccessResponse**: User response to secure access request
//!
//! ## Example
//!
//! ```rust
//! use colony_protocol::{emit_agent_status, emit_widget_request};
//!
//! // Notify terminal that agent is thinking
//! emit_agent_status("thinking");
//!
//! // Request tool approval widget
//! emit_widget_request("tool_approval", serde_json::json!({
//!     "tool": "file_write",
//!     "args": {"path": "/etc/hosts"}
//! }));
//! ```

use serde::{Deserialize, Serialize};
use std::io::{self, Write};

/// OSC 2337 protocol messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ProtocolMessage {
    Agent {
        status: String,
    },
    Widget {
        kind: String,
        data: serde_json::Value,
    },
    Memory {
        operation: String,
    },
    Output {
        content_type: String,
    },
    Session {
        action: String,
    },
    SecureAccess {
        request_id: u64,
        variable: String,
        script_path: Option<String>,
        script_hash: Option<String>,
    },
    SecureAccessResponse {
        request_id: u64,
        decision: SecureAccessDecision,
    },
    Prompt {
        left: Vec<PromptComponentData>,
        right: Vec<PromptComponentData>,
    },
    WidgetRequest {
        widget: String,
        component_id: String,
        context: serde_json::Value,
    },
    WidgetData {
        widget: String,
        data: serde_json::Value,
    },
    WidgetAction {
        widget: String,
        action: String,
        payload: serde_json::Value,
    },
    WidgetResult {
        widget: String,
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptComponentData {
    pub id: String,
    pub spans: Vec<PromptSpan>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interactive: Option<InteractiveConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractiveConfig {
    pub click: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hover: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptSpan {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bg: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bold: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub dim: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub italic: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecureAccessDecision {
    AllowOnce,
    Remember,
    Deny,
}

/// Emit a protocol message to stdout as an OSC 2337 escape sequence
///
/// Format: `ESC ] 2337 ; <JSON> BEL`
///
/// # Example
///
/// ```rust
/// use colony_protocol::{ProtocolMessage, emit_message};
///
/// let msg = ProtocolMessage::Agent {
///     status: "idle".to_string()
/// };
///
/// emit_message(&msg);
/// ```
pub fn emit_message(msg: &ProtocolMessage) {
    if let Ok(json) = serde_json::to_string(msg) {
        // OSC sequence: ESC ] 2337 ; <payload> BEL
        print!("\x1b]2337;{}\x07", json);
        io::stdout().flush().ok();
    }
}

/// Emit an agent status update
///
/// # Example
///
/// ```rust
/// use colony_protocol::emit_agent_status;
///
/// emit_agent_status("thinking");
/// emit_agent_status("streaming");
/// emit_agent_status("idle");
/// ```
pub fn emit_agent_status(status: &str) {
    let msg = ProtocolMessage::Agent {
        status: status.to_string(),
    };
    emit_message(&msg);
}

/// Emit a widget request
///
/// Widgets are interactive UI elements displayed by the terminal.
///
/// # Example
///
/// ```rust
/// use colony_protocol::emit_widget_request;
/// use serde_json::json;
///
/// // Request tool approval
/// emit_widget_request("tool_approval", json!({
///     "tool": "file_write",
///     "path": "/important/file.txt"
/// }));
///
/// // Request file picker
/// emit_widget_request("file_picker", json!({
///     "mode": "open",
///     "filter": "*.rs"
/// }));
/// ```
pub fn emit_widget_request(kind: &str, data: serde_json::Value) {
    let msg = ProtocolMessage::Widget {
        kind: kind.to_string(),
        data,
    };
    emit_message(&msg);
}

/// Emit a memory operation notification
///
/// # Example
///
/// ```rust
/// use colony_protocol::emit_memory_operation;
///
/// emit_memory_operation("store");
/// emit_memory_operation("retrieve");
/// emit_memory_operation("search");
/// ```
pub fn emit_memory_operation(operation: &str) {
    let msg = ProtocolMessage::Memory {
        operation: operation.to_string(),
    };
    emit_message(&msg);
}

/// Emit output metadata
///
/// Used for syntax highlighting, hyperlinks, etc.
///
/// # Example
///
/// ```rust
/// use colony_protocol::emit_output_metadata;
///
/// emit_output_metadata("syntax_highlight");
/// emit_output_metadata("hyperlink");
/// ```
pub fn emit_output_metadata(content_type: &str) {
    let msg = ProtocolMessage::Output {
        content_type: content_type.to_string(),
    };
    emit_message(&msg);
}

/// Emit a session action
///
/// # Example
///
/// ```rust
/// use colony_protocol::emit_session_action;
///
/// emit_session_action("checkpoint");
/// emit_session_action("resume");
/// ```
pub fn emit_session_action(action: &str) {
    let msg = ProtocolMessage::Session {
        action: action.to_string(),
    };
    emit_message(&msg);
}

pub fn emit_secure_access_request(
    request_id: u64,
    variable: &str,
    script_path: Option<&str>,
    script_hash: Option<&str>,
) {
    let msg = ProtocolMessage::SecureAccess {
        request_id,
        variable: variable.to_string(),
        script_path: script_path.map(|s| s.to_string()),
        script_hash: script_hash.map(|s| s.to_string()),
    };
    emit_message(&msg);
}

pub fn emit_prompt(left: Vec<PromptComponentData>, right: Vec<PromptComponentData>) {
    let msg = ProtocolMessage::Prompt { left, right };
    emit_message(&msg);
}

pub fn emit_widget_data(widget: &str, data: serde_json::Value) {
    let msg = ProtocolMessage::WidgetData {
        widget: widget.to_string(),
        data,
    };
    emit_message(&msg);
}

pub fn emit_widget_result(widget: &str, success: bool, message: Option<&str>, error: Option<&str>) {
    let msg = ProtocolMessage::WidgetResult {
        widget: widget.to_string(),
        success,
        message: message.map(|s| s.to_string()),
        error: error.map(|s| s.to_string()),
    };
    emit_message(&msg);
}

pub fn parse_widget_request(json: &str) -> Option<(String, String, serde_json::Value)> {
    if let Ok(ProtocolMessage::WidgetRequest {
        widget,
        component_id,
        context,
    }) = serde_json::from_str::<ProtocolMessage>(json)
    {
        return Some((widget, component_id, context));
    }
    None
}

pub fn parse_widget_action(json: &str) -> Option<(String, String, serde_json::Value)> {
    if let Ok(ProtocolMessage::WidgetAction {
        widget,
        action,
        payload,
    }) = serde_json::from_str::<ProtocolMessage>(json)
    {
        return Some((widget, action, payload));
    }
    None
}

pub fn parse_secure_access_response(json: &str) -> Option<(u64, SecureAccessDecision)> {
    if let Ok(ProtocolMessage::SecureAccessResponse {
        request_id,
        decision,
    }) = serde_json::from_str::<ProtocolMessage>(json)
    {
        return Some((request_id, decision));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_message_serialization() {
        let msg = ProtocolMessage::Agent {
            status: "thinking".to_string(),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"Agent\""));
        assert!(json.contains("\"status\":\"thinking\""));
    }

    #[test]
    fn test_widget_message() {
        let msg = ProtocolMessage::Widget {
            kind: "tool_approval".to_string(),
            data: serde_json::json!({"tool": "file_write", "path": "/etc/hosts"}),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"Widget\""));
        assert!(json.contains("\"kind\":\"tool_approval\""));
        assert!(json.contains("file_write"));
    }

    #[test]
    fn test_memory_message() {
        let msg = ProtocolMessage::Memory {
            operation: "store".to_string(),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"Memory\""));
        assert!(json.contains("\"operation\":\"store\""));
    }

    #[test]
    fn test_output_message() {
        let msg = ProtocolMessage::Output {
            content_type: "syntax_highlight".to_string(),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"Output\""));
        assert!(json.contains("\"content_type\":\"syntax_highlight\""));
    }

    #[test]
    fn test_session_message() {
        let msg = ProtocolMessage::Session {
            action: "checkpoint".to_string(),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"Session\""));
        assert!(json.contains("\"action\":\"checkpoint\""));
    }

    #[test]
    fn test_secure_access_request() {
        let msg = ProtocolMessage::SecureAccess {
            request_id: 42,
            variable: "AWS_SECRET_KEY".to_string(),
            script_path: Some("/home/user/deploy.sh".to_string()),
            script_hash: Some("sha256:abc123".to_string()),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"SecureAccess\""));
        assert!(json.contains("\"request_id\":42"));
        assert!(json.contains("\"variable\":\"AWS_SECRET_KEY\""));
        assert!(json.contains("\"script_path\":\"/home/user/deploy.sh\""));
        assert!(json.contains("\"script_hash\":\"sha256:abc123\""));
    }

    #[test]
    fn test_secure_access_response() {
        let msg = ProtocolMessage::SecureAccessResponse {
            request_id: 42,
            decision: SecureAccessDecision::Remember,
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"SecureAccessResponse\""));
        assert!(json.contains("\"request_id\":42"));
        assert!(json.contains("\"decision\":\"Remember\""));
    }

    #[test]
    fn test_parse_secure_access_response() {
        let json = r#"{"type":"SecureAccessResponse","request_id":123,"decision":"AllowOnce"}"#;
        let result = parse_secure_access_response(json);
        assert!(result.is_some());
        let (id, decision) = result.unwrap();
        assert_eq!(id, 123);
        assert_eq!(decision, SecureAccessDecision::AllowOnce);

        let json = r#"{"type":"SecureAccessResponse","request_id":456,"decision":"Deny"}"#;
        let result = parse_secure_access_response(json);
        assert!(result.is_some());
        let (id, decision) = result.unwrap();
        assert_eq!(id, 456);
        assert_eq!(decision, SecureAccessDecision::Deny);
    }

    #[test]
    fn test_prompt_message() {
        let msg = ProtocolMessage::Prompt {
            left: vec![PromptComponentData {
                id: "pwd".to_string(),
                spans: vec![PromptSpan {
                    text: "~/project".to_string(),
                    fg: Some("cyan".to_string()),
                    bg: None,
                    bold: false,
                    dim: false,
                    italic: false,
                }],
                data: None,
                interactive: None,
            }],
            right: vec![],
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"Prompt\""));
        assert!(json.contains("pwd"));
        assert!(json.contains("~/project"));
    }

    #[test]
    fn test_prompt_with_interactive() {
        let msg = ProtocolMessage::Prompt {
            left: vec![PromptComponentData {
                id: "git_branch".to_string(),
                spans: vec![PromptSpan {
                    text: " main".to_string(),
                    fg: Some("green".to_string()),
                    bg: None,
                    bold: false,
                    dim: false,
                    italic: false,
                }],
                data: Some(serde_json::json!({"branch": "main"})),
                interactive: Some(InteractiveConfig {
                    click: "branch_picker".to_string(),
                    hover: Some("Click to switch branches".to_string()),
                }),
            }],
            right: vec![],
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"interactive\""));
        assert!(json.contains("\"click\":\"branch_picker\""));
        assert!(json.contains("\"hover\":\"Click to switch branches\""));
    }

    #[test]
    fn test_widget_request_parsing() {
        let json = r#"{"type":"WidgetRequest","widget":"branch_picker","component_id":"git_branch","context":{"current_branch":"main"}}"#;
        let result = parse_widget_request(json);
        assert!(result.is_some());
        let (widget, component_id, context) = result.unwrap();
        assert_eq!(widget, "branch_picker");
        assert_eq!(component_id, "git_branch");
        assert_eq!(context["current_branch"], "main");
    }

    #[test]
    fn test_widget_action_parsing() {
        let json = r#"{"type":"WidgetAction","widget":"branch_picker","action":"switch","payload":{"branch":"feature-x"}}"#;
        let result = parse_widget_action(json);
        assert!(result.is_some());
        let (widget, action, payload) = result.unwrap();
        assert_eq!(widget, "branch_picker");
        assert_eq!(action, "switch");
        assert_eq!(payload["branch"], "feature-x");
    }

    #[test]
    fn test_widget_data_serialization() {
        let msg = ProtocolMessage::WidgetData {
            widget: "branch_picker".to_string(),
            data: serde_json::json!({
                "current": "main",
                "local": [{"name": "main", "current": true}]
            }),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"WidgetData\""));
        assert!(json.contains("\"widget\":\"branch_picker\""));
        assert!(json.contains("\"current\":\"main\""));
    }

    #[test]
    fn test_widget_result_success() {
        let msg = ProtocolMessage::WidgetResult {
            widget: "branch_picker".to_string(),
            success: true,
            message: Some("Switched to branch 'feature-x'".to_string()),
            error: None,
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"WidgetResult\""));
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("Switched to branch"));
        assert!(!json.contains("\"error\""));
    }

    #[test]
    fn test_widget_result_error() {
        let msg = ProtocolMessage::WidgetResult {
            widget: "branch_picker".to_string(),
            success: false,
            message: None,
            error: Some("Cannot switch with uncommitted changes".to_string()),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"success\":false"));
        assert!(json.contains("\"error\""));
        assert!(!json.contains("\"message\""));
    }

    // Note: Testing actual escape sequence emission would require
    // capturing stdout, which is complex. These tests verify the
    // JSON serialization is correct, which is the core logic.
}
