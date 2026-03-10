//! Observer that emits observations as OSC 2337 escape sequences.
//!
//! When colony-shell runs inside colony-nerve's terminal widget, this observer
//! forwards observations via the terminal protocol instead of writing to SQLite.
//! The host terminal (colony-nerve) receives and stores them.
//!
//! OSC 2337 format: `\x1b]2337;{json}\x07`

use std::io::Write;

use async_trait::async_trait;
use serde_json::json;

use crate::types::{
    AgentChatData, CommandData, ContextUsageData, ErrorData, FileActivityData, ObservationCategory,
};
use crate::ReflectionObserver;

/// Observer that emits observations as OSC 2337 escape sequences to stdout.
///
/// Used when colony-shell is embedded in colony-nerve's terminal widget.
/// Each observation is serialized as JSON and wrapped in the OSC 2337 protocol:
///
/// ```text
/// \x1b]2337;{"type":"observation","category":"command","data":{...}}\x07
/// ```
pub struct OscObserver;

impl OscObserver {
    /// Create a new OSC observer.
    pub fn new() -> Self {
        Self
    }

    /// Emit an observation as an OSC 2337 escape sequence to stdout.
    fn emit(&self, category: ObservationCategory, data: serde_json::Value) {
        let payload = json!({
            "type": "observation",
            "category": category.as_str(),
            "data": data,
        });

        let Some(encoded) = serde_json::to_string(&payload).ok() else {
            tracing::warn!("failed to serialize OSC observation payload");
            return;
        };

        // Write the full escape sequence atomically to avoid interleaving.
        let sequence = format!("\x1b]2337;{encoded}\x07");
        let mut stdout = std::io::stdout().lock();
        if let Err(e) = stdout.write_all(sequence.as_bytes()) {
            tracing::warn!("failed to write OSC observation: {e}");
        }
        // Flush to ensure the escape sequence is delivered immediately.
        let _ = stdout.flush();
    }
}

impl Default for OscObserver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ReflectionObserver for OscObserver {
    async fn observe_command(&self, cmd: CommandData) {
        let data = serde_json::to_value(&cmd).expect("CommandData is serializable");
        self.emit(ObservationCategory::Command, data);
    }

    async fn observe_error(&self, err: ErrorData) {
        let data = serde_json::to_value(&err).expect("ErrorData is serializable");
        self.emit(ObservationCategory::Error, data);
    }

    async fn observe_agent_chat(&self, chat: AgentChatData) {
        let data = serde_json::to_value(&chat).expect("AgentChatData is serializable");
        self.emit(ObservationCategory::AgentChat, data);
    }

    async fn observe_file_activity(&self, file: FileActivityData) {
        let data = serde_json::to_value(&file).expect("FileActivityData is serializable");
        self.emit(ObservationCategory::FileActivity, data);
    }

    async fn observe_context_usage(&self, ctx: ContextUsageData) {
        let data = serde_json::to_value(&ctx).expect("ContextUsageData is serializable");
        self.emit(ObservationCategory::ContextUsage, data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_osc_observer_does_not_panic() {
        let observer = OscObserver::new();

        observer
            .observe_command(CommandData {
                input: "ls -la".to_string(),
                exit_code: 0,
                duration_ms: 42,
                pipeline_length: 1,
            })
            .await;

        observer
            .observe_error(ErrorData {
                message: "not found".to_string(),
                kind: "command".to_string(),
                recoverable: true,
            })
            .await;

        observer
            .observe_agent_chat(AgentChatData {
                provider: "openai".to_string(),
                model: "gpt-4".to_string(),
                prompt_tokens: 100,
                completion_tokens: 50,
                duration_ms: 1200,
            })
            .await;

        observer
            .observe_file_activity(FileActivityData {
                path: "/tmp/test.rs".to_string(),
                action: "edit".to_string(),
                language: Some("rust".to_string()),
            })
            .await;

        observer
            .observe_context_usage(ContextUsageData {
                channel: "fd3".to_string(),
                bytes: 1024,
                direction: "write".to_string(),
            })
            .await;
    }

    #[test]
    fn test_osc_observer_default() {
        let _observer = OscObserver::default();
    }
}
