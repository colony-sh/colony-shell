//! OSC 2337 protocol for colony-terminal communication

use serde::{Deserialize, Serialize};

/// OSC 2337 protocol messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ProtocolMessage {
    Agent { status: String },
    Widget { kind: String, data: serde_json::Value },
    Memory { operation: String },
    Output { content_type: String },
    Session { action: String },
}
