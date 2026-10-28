//! Observer trait and implementations for the reflection system.

use std::sync::Mutex;

use async_trait::async_trait;

use crate::storage::{ObservationStore, StorageError};
use crate::types::{
    AgentChatData, CommandData, ContextUsageData, ErrorData, FileActivityData, Observation,
    ObservationCategory, SessionTracker,
};

// ---------------------------------------------------------------------------
// ReflectionObserver trait
// ---------------------------------------------------------------------------

/// Async observer interface for recording shell observations.
#[allow(clippy::double_must_use)] // async_trait marks methods that already return a must-use future
#[async_trait]
pub trait ReflectionObserver: Send + Sync {
    async fn observe_command(&self, cmd: CommandData);
    async fn observe_error(&self, err: ErrorData);
    async fn observe_agent_chat(&self, chat: AgentChatData);
    async fn observe_file_activity(&self, file: FileActivityData);
    async fn observe_context_usage(&self, ctx: ContextUsageData);
}

// ---------------------------------------------------------------------------
// NoopObserver
// ---------------------------------------------------------------------------

/// Observer that discards all observations — useful as a default.
pub struct NoopObserver;

#[async_trait]
impl ReflectionObserver for NoopObserver {
    async fn observe_command(&self, _cmd: CommandData) {}
    async fn observe_error(&self, _err: ErrorData) {}
    async fn observe_agent_chat(&self, _chat: AgentChatData) {}
    async fn observe_file_activity(&self, _file: FileActivityData) {}
    async fn observe_context_usage(&self, _ctx: ContextUsageData) {}
}

// ---------------------------------------------------------------------------
// SqliteObserver
// ---------------------------------------------------------------------------

/// Observer that persists observations to a SQLite database.
pub struct SqliteObserver {
    store: ObservationStore,
    tracker: Mutex<SessionTracker>,
}

impl SqliteObserver {
    /// Create an observer that writes to the default database path.
    pub fn new(session_id: String) -> Result<Self, StorageError> {
        let store = ObservationStore::open(&ObservationStore::default_path())?;
        Ok(Self {
            store,
            tracker: Mutex::new(SessionTracker::with_session_id(session_id)),
        })
    }

    /// Create an observer with an explicit store — useful for testing.
    pub fn with_store(store: ObservationStore, session_id: String) -> Self {
        Self {
            store,
            tracker: Mutex::new(SessionTracker::with_session_id(session_id)),
        }
    }

    /// Record an observation of the given category and data.
    fn record(&self, category: ObservationCategory, data: serde_json::Value) {
        let mut tracker = self.tracker.lock().expect("tracker mutex poisoned");

        if category == ObservationCategory::Error {
            tracker.record_error();
        }

        let context = tracker.build_context();
        drop(tracker);

        let obs = Observation::new(category, data, context);
        if let Err(e) = self.store.insert(&obs) {
            tracing::warn!("failed to record observation: {e}");
        }
    }

    /// Access the underlying store for querying.
    pub fn store(&self) -> &ObservationStore {
        &self.store
    }
}

#[async_trait]
impl ReflectionObserver for SqliteObserver {
    async fn observe_command(&self, cmd: CommandData) {
        let data = serde_json::to_value(&cmd).expect("CommandData is serializable");
        self.record(ObservationCategory::Command, data);
    }

    async fn observe_error(&self, err: ErrorData) {
        let data = serde_json::to_value(&err).expect("ErrorData is serializable");
        self.record(ObservationCategory::Error, data);
    }

    async fn observe_agent_chat(&self, chat: AgentChatData) {
        let data = serde_json::to_value(&chat).expect("AgentChatData is serializable");
        self.record(ObservationCategory::AgentChat, data);
    }

    async fn observe_file_activity(&self, file: FileActivityData) {
        let data = serde_json::to_value(&file).expect("FileActivityData is serializable");
        self.record(ObservationCategory::FileActivity, data);
    }

    async fn observe_context_usage(&self, ctx: ContextUsageData) {
        let data = serde_json::to_value(&ctx).expect("ContextUsageData is serializable");
        self.record(ObservationCategory::ContextUsage, data);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_command_data() -> CommandData {
        CommandData {
            input: "ls -la".to_string(),
            exit_code: 0,
            duration_ms: 42,
            pipeline_length: 1,
        }
    }

    fn make_error_data() -> ErrorData {
        ErrorData {
            message: "not found".to_string(),
            kind: "command".to_string(),
            recoverable: true,
        }
    }

    fn make_agent_chat_data() -> AgentChatData {
        AgentChatData {
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            prompt_tokens: 100,
            completion_tokens: 50,
            duration_ms: 1200,
        }
    }

    fn make_file_activity_data() -> FileActivityData {
        FileActivityData {
            path: "/tmp/test.rs".to_string(),
            action: "edit".to_string(),
            language: Some("rust".to_string()),
        }
    }

    fn make_context_usage_data() -> ContextUsageData {
        ContextUsageData {
            channel: "fd3".to_string(),
            bytes: 1024,
            direction: "write".to_string(),
        }
    }

    #[tokio::test]
    async fn test_sqlite_observer_records_command() {
        let store = ObservationStore::open_in_memory().expect("open in-memory");
        let observer = SqliteObserver::with_store(store, "sess-test".to_string());

        observer.observe_command(make_command_data()).await;

        assert_eq!(observer.store().count(None).expect("count"), 1);
    }

    #[tokio::test]
    async fn test_sqlite_observer_records_all_types() {
        let store = ObservationStore::open_in_memory().expect("open in-memory");
        let observer = SqliteObserver::with_store(store, "sess-all".to_string());

        observer.observe_command(make_command_data()).await;
        observer.observe_error(make_error_data()).await;
        observer.observe_agent_chat(make_agent_chat_data()).await;
        observer
            .observe_file_activity(make_file_activity_data())
            .await;
        observer
            .observe_context_usage(make_context_usage_data())
            .await;

        assert_eq!(observer.store().count(None).expect("count"), 5);
    }

    #[tokio::test]
    async fn test_noop_observer_does_nothing() {
        let observer = NoopObserver;
        observer.observe_command(make_command_data()).await;
        // No panic — success.
    }

    #[tokio::test]
    async fn test_error_degrades_energy() {
        let store = ObservationStore::open_in_memory().expect("open in-memory");
        let observer = SqliteObserver::with_store(store, "sess-energy".to_string());

        // Record initial command to establish baseline energy.
        observer.observe_command(make_command_data()).await;
        let rows = observer.store().by_session("sess-energy").expect("query");
        let initial_energy = rows[0].context.energy_level;

        // Inject errors to degrade energy.
        for _ in 0..10 {
            observer.observe_error(make_error_data()).await;
        }

        // Record another command and check energy dropped.
        observer.observe_command(make_command_data()).await;
        let rows = observer.store().by_session("sess-energy").expect("query");
        let final_energy = rows.last().expect("last row").context.energy_level;

        assert!(
            final_energy < initial_energy,
            "energy should degrade after errors: {final_energy} vs {initial_energy}",
        );
    }
}
