//! SQLite-backed shell command history with structured metadata.
//!
//! Provides FTS5 full-text search, agent tracking, privacy-aware
//! filtering, risk scoring, and aggregated statistics.

mod executor;
mod privacy;
mod record;
mod risk;
mod search;
mod stats;
mod store;
mod writer;

pub use executor::{AgentPattern, ExecutorDetector};
pub use privacy::{PrivacyAction, PrivacyFilter};
pub use record::{Executor, HistoryRecord, HistoryRecordBuilder, RecordError, RiskLevel};
pub use risk::RiskScorer;
pub use search::SearchQuery;
pub use stats::HistoryStats;
pub use store::{
    AgentSummary, ExportFormat, ExportRecord, HistoryStore, SearchResult, StoreError, StoreResult,
};
pub use writer::HistoryWriter;
