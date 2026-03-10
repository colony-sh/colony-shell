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
//! - **Hello**: Protocol version negotiation (shell/terminal handshake)
//! - **SecureAccess**: Secure environment variable access request
//! - **SecureAccessResponse**: User response to secure access request
//! - **Tree**: File tree state sync (snapshot, updates, actions)
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

pub mod tree;

use serde::{Deserialize, Serialize};
use std::io::{self, Write};

/// Current OSC 2337 protocol version supported by this crate.
pub const PROTOCOL_VERSION: u32 = 1;
/// Minimum protocol version supported by this crate.
pub const PROTOCOL_MIN_VERSION: u32 = 1;
/// Maximum protocol version supported by this crate.
pub const PROTOCOL_MAX_VERSION: u32 = 1;

pub use tree::{
    GitStatus, NodeId, TreeAction, TreeActionResult, TreeNode, TreeNodeType, TreeSnapshot,
    TreeUpdate,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProtocolRole {
    Shell,
    Terminal,
}

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
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<serde_json::Value>,
    },
    Session {
        action: String,
    },
    Hello {
        role: ProtocolRole,
        min_version: u32,
        max_version: u32,
    },
    HelloAck {
        role: ProtocolRole,
        negotiated_version: u32,
    },
    HelloNack {
        role: ProtocolRole,
        reason: String,
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
    Heartbeat {
        agent: String,
        status: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        task: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        progress: Option<u8>,
        updated: String,
    },
    /// Full prompt state for reactive terminal rendering (Prompt=state)
    PromptState {
        state: PromptStateData,
    },
    /// Signal that shell is ready for input (Prompt=ready)
    PromptReady {
        timestamp: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        last_duration_ms: Option<u64>,
    },
    /// Signal that shell is executing a command (Prompt=busy)
    PromptBusy {
        command: String,
        started_at: u64,
    },
    /// File tree snapshot (full tree sent on cd / initial load)
    FileTreeSnapshot {
        tree: serde_json::Value,
        cwd: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        shell_id: Option<String>,
    },
    /// File tree delta (incremental update on file change)
    FileTreeDelta {
        kind: FileTreeChangeKind,
        path: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        shell_id: Option<String>,
    },
    /// File tree git status overlay
    FileTreeGit {
        statuses: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        shell_id: Option<String>,
    },
    /// Pane split operation
    PaneSplit {
        direction: PaneSplitDirection,
        #[serde(default = "default_split_ratio")]
        ratio: f64,
    },
    /// Pane close
    PaneClose {
        #[serde(skip_serializing_if = "Option::is_none")]
        pane_id: Option<u32>,
    },
    /// Pane focus direction
    PaneFocus {
        direction: PaneFocusDirection,
    },
    /// Pane layout preset
    PaneLayout {
        preset: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        panes: Option<serde_json::Value>,
    },
    /// Full tree snapshot (shell → terminal)
    TreeSnapshot(TreeSnapshot),
    /// Incremental tree update (shell → terminal)
    TreeUpdate(TreeUpdate),
    /// Tree action result (shell → terminal)
    TreeActionResult(TreeActionResult),
    /// User tree action (terminal → shell)
    TreeAction(TreeAction),
    /// Event worker rules from colony-nerve
    EventRules {
        rules: Vec<serde_json::Value>,
    },
    /// Widget response from terminal (e.g. tool approval decision)
    WidgetResponse {
        widget: String,
        component_id: String,
        approved: bool,
    },
    /// Mirror suggestion from shell/agent to terminal sidebar
    MirrorSuggestion {
        text: String,
        priority: String, // "info" or "attention"
    },
}

/// Full prompt state data sent via OSC 2337 (Prompt=state per spec B.3.11)
///
/// Contains all shell state needed for the terminal to render a prompt.
/// The terminal theme controls visual rendering; the shell only provides data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptStateData {
    pub cwd: String,
    pub display_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    pub exit_status: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git: Option<GitStateData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentStateData>,
    pub shlvl: u32,
    pub in_job: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// Git repository state for prompt rendering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitStateData {
    pub branch: String,
    pub detached: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    pub staged: usize,
    pub modified: usize,
    pub untracked: usize,
    pub conflicted: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ahead: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub behind: Option<usize>,
    pub insertions: usize,
    pub deletions: usize,
}

/// Agent state for prompt rendering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStateData {
    pub name: String,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
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

/// Kind of file tree change for delta messages
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileTreeChangeKind {
    Created,
    Modified,
    Removed,
}

/// Pane split direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaneSplitDirection {
    Horizontal,
    Vertical,
}

/// Pane focus direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaneFocusDirection {
    Left,
    Right,
    Up,
    Down,
}

fn default_split_ratio() -> f64 {
    0.5
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
        payload: None,
    };
    emit_message(&msg);
}

pub fn emit_output_payload(content_type: &str, payload: serde_json::Value) {
    let msg = ProtocolMessage::Output {
        content_type: content_type.to_string(),
        payload: Some(payload),
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

pub fn emit_hello(role: ProtocolRole) {
    let msg = ProtocolMessage::Hello {
        role,
        min_version: PROTOCOL_MIN_VERSION,
        max_version: PROTOCOL_MAX_VERSION,
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

/// Emit a heartbeat status update for an agent
pub fn emit_heartbeat(
    agent: &str,
    status: &str,
    task: Option<&str>,
    progress: Option<u8>,
    updated: &str,
) {
    let msg = ProtocolMessage::Heartbeat {
        agent: agent.to_string(),
        status: status.to_string(),
        task: task.map(|s| s.to_string()),
        progress,
        updated: updated.to_string(),
    };
    emit_message(&msg);
}

/// Emit full prompt state for reactive terminal rendering
pub fn emit_prompt_state(state: PromptStateData) {
    let msg = ProtocolMessage::PromptState { state };
    emit_message(&msg);
}

/// Emit prompt ready signal (shell ready for input)
pub fn emit_prompt_ready(last_duration_ms: Option<u64>) {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let msg = ProtocolMessage::PromptReady {
        timestamp,
        last_duration_ms,
    };
    emit_message(&msg);
}

/// Emit prompt busy signal (shell executing a command)
pub fn emit_prompt_busy(command: &str) {
    let started_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let msg = ProtocolMessage::PromptBusy {
        command: command.to_string(),
        started_at,
    };
    emit_message(&msg);
}

/// Emit a file tree snapshot (full tree JSON)
///
/// Sent on directory change or initial load.
pub fn emit_file_tree_snapshot(tree: serde_json::Value, cwd: &str, shell_id: Option<&str>) {
    let msg = ProtocolMessage::FileTreeSnapshot {
        tree,
        cwd: cwd.to_string(),
        shell_id: shell_id.map(|s| s.to_string()),
    };
    emit_message(&msg);
}

/// Emit a file tree delta (incremental change)
///
/// Sent when a single file is created, modified, or removed.
pub fn emit_file_tree_delta(kind: FileTreeChangeKind, path: &str, shell_id: Option<&str>) {
    let msg = ProtocolMessage::FileTreeDelta {
        kind,
        path: path.to_string(),
        shell_id: shell_id.map(|s| s.to_string()),
    };
    emit_message(&msg);
}

/// Emit file tree git status overlay
///
/// Sent after git status cache refresh to overlay status indicators.
pub fn emit_file_tree_git(statuses: serde_json::Value, shell_id: Option<&str>) {
    let msg = ProtocolMessage::FileTreeGit {
        statuses,
        shell_id: shell_id.map(|s| s.to_string()),
    };
    emit_message(&msg);
}

/// Emit a pane split command
pub fn emit_pane_split(direction: PaneSplitDirection, ratio: f64) {
    let msg = ProtocolMessage::PaneSplit { direction, ratio };
    emit_message(&msg);
}

/// Emit a pane close command
pub fn emit_pane_close(pane_id: Option<u32>) {
    let msg = ProtocolMessage::PaneClose { pane_id };
    emit_message(&msg);
}

/// Emit a pane focus direction command
pub fn emit_pane_focus(direction: PaneFocusDirection) {
    let msg = ProtocolMessage::PaneFocus { direction };
    emit_message(&msg);
}

/// Emit a pane layout preset command
pub fn emit_pane_layout(preset: &str, panes: Option<serde_json::Value>) {
    let msg = ProtocolMessage::PaneLayout {
        preset: preset.to_string(),
        panes,
    };
    emit_message(&msg);
}

/// Emit a mirror suggestion to the terminal sidebar
///
/// Suggestions are contextual nudges displayed in the mirror panel.
/// Priority should be `"info"` for passive suggestions or `"attention"`
/// for urgent ones.
pub fn emit_mirror_suggestion(text: &str, priority: &str) {
    let msg = ProtocolMessage::MirrorSuggestion {
        text: text.to_string(),
        priority: priority.to_string(),
    };
    emit_message(&msg);
}

pub fn parse_hello_ack(json: &str) -> Option<u32> {
    if let Ok(ProtocolMessage::HelloAck {
        negotiated_version, ..
    }) = serde_json::from_str::<ProtocolMessage>(json)
    {
        return Some(negotiated_version);
    }
    None
}

// ============================================================================
// Tree Protocol Functions
// ============================================================================

/// Emit a tree snapshot to the terminal
pub fn emit_tree_snapshot(snapshot: &TreeSnapshot) {
    let msg = ProtocolMessage::TreeSnapshot(snapshot.clone());
    emit_message(&msg);
}

/// Emit a tree update to the terminal
pub fn emit_tree_update(update: &TreeUpdate) {
    let msg = ProtocolMessage::TreeUpdate(update.clone());
    emit_message(&msg);
}

/// Emit a tree action result to the terminal
pub fn emit_tree_action_result(result: &TreeActionResult) {
    let msg = ProtocolMessage::TreeActionResult(result.clone());
    emit_message(&msg);
}

/// Parse a tree action from JSON
pub fn parse_tree_action(json: &str) -> Option<TreeAction> {
    if let Ok(ProtocolMessage::TreeAction(action)) = serde_json::from_str::<ProtocolMessage>(json) {
        return Some(action);
    }
    None
}

/// Parse a tree snapshot from JSON
pub fn parse_tree_snapshot(json: &str) -> Option<TreeSnapshot> {
    if let Ok(ProtocolMessage::TreeSnapshot(snapshot)) =
        serde_json::from_str::<ProtocolMessage>(json)
    {
        return Some(snapshot);
    }
    None
}

/// Parse a tree update from JSON
pub fn parse_tree_update(json: &str) -> Option<TreeUpdate> {
    if let Ok(ProtocolMessage::TreeUpdate(update)) = serde_json::from_str::<ProtocolMessage>(json) {
        return Some(update);
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
            payload: None,
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"Output\""));
        assert!(json.contains("\"content_type\":\"syntax_highlight\""));
    }

    #[test]
    fn test_output_message_with_payload() {
        let msg = ProtocolMessage::Output {
            content_type: "markdown".to_string(),
            payload: Some(serde_json::json!({"markdown": "# Title"})),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"Output\""));
        assert!(json.contains("\"content_type\":\"markdown\""));
        assert!(json.contains("Title"));
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
    fn test_hello_message() {
        let msg = ProtocolMessage::Hello {
            role: ProtocolRole::Shell,
            min_version: 1,
            max_version: 1,
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"Hello\""));
        assert!(json.contains("\"role\":\"shell\""));
        assert!(json.contains("\"min_version\":1"));
        assert!(json.contains("\"max_version\":1"));
    }

    #[test]
    fn test_hello_ack_parsing() {
        let json = r#"{"type":"HelloAck","role":"terminal","negotiated_version":1}"#;
        let result = parse_hello_ack(json);
        assert_eq!(result, Some(1));
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

    #[test]
    fn test_prompt_state_serialization() {
        let msg = ProtocolMessage::PromptState {
            state: PromptStateData {
                cwd: "/home/user/src/colony".to_string(),
                display_path: "~/src/colony".to_string(),
                user: Some("user".to_string()),
                host: Some("hostname".to_string()),
                exit_status: 0,
                git: Some(GitStateData {
                    branch: "main".to_string(),
                    detached: false,
                    commit: None,
                    staged: 2,
                    modified: 1,
                    untracked: 3,
                    conflicted: 0,
                    ahead: Some(1),
                    behind: Some(0),
                    insertions: 10,
                    deletions: 5,
                }),
                agent: None,
                shlvl: 1,
                in_job: false,
                model: Some("claude-sonnet".to_string()),
            },
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"PromptState\""));
        assert!(json.contains("\"display_path\":\"~/src/colony\""));
        assert!(json.contains("\"branch\":\"main\""));
        assert!(json.contains("\"staged\":2"));
        assert!(json.contains("\"model\":\"claude-sonnet\""));
    }

    #[test]
    fn test_prompt_state_no_git() {
        let state = PromptStateData {
            cwd: "/tmp".to_string(),
            display_path: "/tmp".to_string(),
            user: None,
            host: None,
            exit_status: 1,
            git: None,
            agent: None,
            shlvl: 1,
            in_job: false,
            model: None,
        };

        let json = serde_json::to_string(&state).unwrap();
        assert!(!json.contains("\"git\""));
        assert!(!json.contains("\"agent\""));
        assert!(!json.contains("\"model\""));
    }

    #[test]
    fn test_prompt_ready_serialization() {
        let msg = ProtocolMessage::PromptReady {
            timestamp: 1702857600000,
            last_duration_ms: Some(1234),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"PromptReady\""));
        assert!(json.contains("\"timestamp\":1702857600000"));
        assert!(json.contains("\"last_duration_ms\":1234"));
    }

    #[test]
    fn test_prompt_ready_no_duration() {
        let msg = ProtocolMessage::PromptReady {
            timestamp: 1702857600000,
            last_duration_ms: None,
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"PromptReady\""));
        assert!(!json.contains("\"last_duration_ms\""));
    }

    #[test]
    fn test_prompt_busy_serialization() {
        let msg = ProtocolMessage::PromptBusy {
            command: "cargo build".to_string(),
            started_at: 1702857600000,
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"PromptBusy\""));
        assert!(json.contains("\"command\":\"cargo build\""));
        assert!(json.contains("\"started_at\":1702857600000"));
    }

    #[test]
    fn test_file_tree_snapshot() {
        let msg = ProtocolMessage::FileTreeSnapshot {
            tree: serde_json::json!({"name": "src", "is_dir": true, "children": []}),
            cwd: "/home/user/project".to_string(),
            shell_id: Some("12345".to_string()),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"FileTreeSnapshot\""));
        assert!(json.contains("\"name\":\"src\""));
        assert!(json.contains("\"cwd\":\"/home/user/project\""));
        assert!(json.contains("\"shell_id\":\"12345\""));
    }

    #[test]
    fn test_file_tree_snapshot_no_shell_id() {
        let msg = ProtocolMessage::FileTreeSnapshot {
            tree: serde_json::json!({"name": "src", "is_dir": true, "children": []}),
            cwd: "/tmp".to_string(),
            shell_id: None,
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"FileTreeSnapshot\""));
        assert!(json.contains("\"cwd\":\"/tmp\""));
        assert!(!json.contains("\"shell_id\""));
    }

    #[test]
    fn test_file_tree_delta() {
        let msg = ProtocolMessage::FileTreeDelta {
            kind: FileTreeChangeKind::Created,
            path: "/src/new_file.rs".to_string(),
            shell_id: Some("12345".to_string()),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"FileTreeDelta\""));
        assert!(json.contains("\"kind\":\"created\""));
        assert!(json.contains("\"path\":\"/src/new_file.rs\""));
        assert!(json.contains("\"shell_id\":\"12345\""));
    }

    #[test]
    fn test_file_tree_delta_no_shell_id() {
        let msg = ProtocolMessage::FileTreeDelta {
            kind: FileTreeChangeKind::Modified,
            path: "/src/lib.rs".to_string(),
            shell_id: None,
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"FileTreeDelta\""));
        assert!(!json.contains("\"shell_id\""));
    }

    #[test]
    fn test_file_tree_git() {
        let msg = ProtocolMessage::FileTreeGit {
            statuses: serde_json::json!({"src/main.rs": "modified"}),
            shell_id: Some("12345".to_string()),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"FileTreeGit\""));
        assert!(json.contains("\"src/main.rs\""));
        assert!(json.contains("\"shell_id\":\"12345\""));
    }

    #[test]
    fn test_file_tree_git_no_shell_id() {
        let msg = ProtocolMessage::FileTreeGit {
            statuses: serde_json::json!({"src/main.rs": "modified"}),
            shell_id: None,
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"FileTreeGit\""));
        assert!(!json.contains("\"shell_id\""));
    }

    #[test]
    fn test_pane_split_horizontal() {
        let msg = ProtocolMessage::PaneSplit {
            direction: PaneSplitDirection::Horizontal,
            ratio: 0.5,
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"PaneSplit\""));
        assert!(json.contains("\"direction\":\"horizontal\""));
        assert!(json.contains("\"ratio\":0.5"));
    }

    #[test]
    fn test_pane_split_vertical() {
        let msg = ProtocolMessage::PaneSplit {
            direction: PaneSplitDirection::Vertical,
            ratio: 0.3,
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"direction\":\"vertical\""));
        assert!(json.contains("\"ratio\":0.3"));
    }

    #[test]
    fn test_pane_close() {
        let msg = ProtocolMessage::PaneClose { pane_id: Some(3) };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"PaneClose\""));
        assert!(json.contains("\"pane_id\":3"));
    }

    #[test]
    fn test_pane_close_current() {
        let msg = ProtocolMessage::PaneClose { pane_id: None };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"PaneClose\""));
        assert!(!json.contains("\"pane_id\""));
    }

    #[test]
    fn test_pane_focus() {
        let msg = ProtocolMessage::PaneFocus {
            direction: PaneFocusDirection::Left,
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"PaneFocus\""));
        assert!(json.contains("\"direction\":\"left\""));
    }

    #[test]
    fn test_pane_layout() {
        let msg = ProtocolMessage::PaneLayout {
            preset: "dev".to_string(),
            panes: Some(serde_json::json!([
                {"direction": "vertical", "ratio": 0.7},
                {"direction": "horizontal", "ratio": 0.5}
            ])),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"PaneLayout\""));
        assert!(json.contains("\"preset\":\"dev\""));
        assert!(json.contains("\"panes\""));
    }

    #[test]
    fn test_file_tree_change_kind_roundtrip() {
        for kind in [
            FileTreeChangeKind::Created,
            FileTreeChangeKind::Modified,
            FileTreeChangeKind::Removed,
        ] {
            let msg = ProtocolMessage::FileTreeDelta {
                kind,
                path: "/test".to_string(),
                shell_id: Some("42".to_string()),
            };
            let json = serde_json::to_string(&msg).unwrap();
            let parsed: ProtocolMessage = serde_json::from_str(&json).unwrap();
            if let ProtocolMessage::FileTreeDelta {
                kind: parsed_kind, ..
            } = parsed
            {
                assert_eq!(kind, parsed_kind);
            } else {
                panic!("Expected FileTreeDelta");
            }
        }
    }

    #[test]
    fn test_pane_focus_direction_roundtrip() {
        for dir in [
            PaneFocusDirection::Left,
            PaneFocusDirection::Right,
            PaneFocusDirection::Up,
            PaneFocusDirection::Down,
        ] {
            let msg = ProtocolMessage::PaneFocus { direction: dir };
            let json = serde_json::to_string(&msg).unwrap();
            let parsed: ProtocolMessage = serde_json::from_str(&json).unwrap();
            if let ProtocolMessage::PaneFocus {
                direction: parsed_dir,
            } = parsed
            {
                assert_eq!(dir, parsed_dir);
            } else {
                panic!("Expected PaneFocus");
            }
        }
    }

    // Note: Testing actual escape sequence emission would require
    // capturing stdout, which is complex. These tests verify the
    // JSON serialization is correct, which is the core logic.

    // ========================================================================
    // Tree Protocol Tests
    // ========================================================================

    #[test]
    fn test_tree_snapshot_message() {
        let snapshot = TreeSnapshot {
            root: "/project".to_string(),
            nodes: vec![
                TreeNode::directory(1, "project".to_string(), "".to_string(), 0)
                    .with_expanded(true),
                TreeNode::file(2, "main.rs".to_string(), "main.rs".to_string(), 1)
                    .with_git_status(GitStatus::Modified),
            ],
            git_branch: Some("main".to_string()),
            git_dirty: true,
            git_ahead: 2,
            git_behind: 0,
        };

        let msg = ProtocolMessage::TreeSnapshot(snapshot);
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"TreeSnapshot\""));
        assert!(json.contains("\"root\":\"/project\""));
        assert!(json.contains("\"git_branch\":\"main\""));
    }

    #[test]
    fn test_tree_update_message() {
        let update = TreeUpdate::NodeAdded {
            node: TreeNode::file(42, "new.rs".to_string(), "src/new.rs".to_string(), 2),
            parent_id: Some(10),
            index: 0,
        };

        let msg = ProtocolMessage::TreeUpdate(update);
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"TreeUpdate\""));
        assert!(json.contains("\"kind\":\"node_added\""));
    }

    #[test]
    fn test_tree_action_message() {
        let action = TreeAction::Toggle { id: 42 };
        let msg = ProtocolMessage::TreeAction(action);
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"TreeAction\""));
        assert!(json.contains("\"action\":\"toggle\""));
    }

    #[test]
    fn test_parse_tree_action() {
        let json = r#"{"type":"TreeAction","action":"toggle","id":42}"#;
        let result = parse_tree_action(json);
        assert!(result.is_some());
        match result.unwrap() {
            TreeAction::Toggle { id } => assert_eq!(id, 42),
            _ => panic!("Expected Toggle action"),
        }
    }

    #[test]
    fn test_parse_tree_action_create_file() {
        let json = r#"{"type":"TreeAction","action":"create_file","parent_id":10,"name":"new.rs"}"#;
        let result = parse_tree_action(json);
        assert!(result.is_some());
        match result.unwrap() {
            TreeAction::CreateFile { parent_id, name } => {
                assert_eq!(parent_id, 10);
                assert_eq!(name, "new.rs");
            }
            _ => panic!("Expected CreateFile action"),
        }
    }

    #[test]
    fn test_tree_action_result_message() {
        let result = TreeActionResult::error("Permission denied");
        let msg = ProtocolMessage::TreeActionResult(result);
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"TreeActionResult\""));
        assert!(json.contains("\"success\":false"));
        assert!(json.contains("\"error\":\"Permission denied\""));
    }
}
