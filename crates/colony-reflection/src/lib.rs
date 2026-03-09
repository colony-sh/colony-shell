//! Reflection data collection for Colony Shell

mod observer;
mod storage;
mod types;

pub use observer::{NoopObserver, ReflectionObserver, SqliteObserver};
pub use storage::{ObservationStore, StorageError, StorageResult};
pub use types::{
    AgentChatData, CommandData, ContextUsageData, ErrorData, FileActivityData,
    Observation, ObservationCategory, SessionContext, SessionTracker,
};
