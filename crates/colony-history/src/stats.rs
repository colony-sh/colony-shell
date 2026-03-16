//! Aggregation queries for the `history stats` subcommand.

use crate::store::{HistoryStore, StoreResult};

/// Aggregated history statistics.
#[derive(Debug)]
pub struct HistoryStats {
    /// Total number of commands recorded.
    pub total_commands: u64,
    /// Timestamp of the earliest recorded command.
    pub earliest_timestamp: Option<i64>,
    /// Commands recorded in the last 24 hours.
    pub today_count: u64,
    /// Commands recorded in the last 7 days.
    pub week_count: u64,
    /// Commands recorded in the last 30 days.
    pub month_count: u64,
    /// Command counts grouped by executor.
    pub executor_counts: Vec<(String, u64)>,
    /// Top directories by command count.
    pub top_directories: Vec<(String, u64)>,
    /// Command counts grouped by hour of day (local time).
    pub peak_hours: Vec<(u32, u64)>,
    /// Number of records dropped by the background writer.
    pub drop_count: u64,
}

impl HistoryStats {
    /// Compute aggregate statistics from the history store.
    pub fn compute(store: &HistoryStore) -> StoreResult<Self> {
        let total_commands = store.count()?;

        let now = chrono::Utc::now().timestamp_millis();
        let day_ago = now - 86_400_000;
        let week_ago = now - 604_800_000;
        let month_ago = now - 2_592_000_000;

        let today_count = store.count_since(day_ago)?;
        let week_count = store.count_since(week_ago)?;
        let month_count = store.count_since(month_ago)?;

        let earliest_timestamp = store.earliest_timestamp()?;
        let executor_counts = store.executor_counts()?;
        let top_directories = store.top_directories(10)?;
        let peak_hours = store.hourly_distribution()?;

        Ok(Self {
            total_commands,
            earliest_timestamp,
            today_count,
            week_count,
            month_count,
            executor_counts,
            top_directories,
            peak_hours,
            drop_count: 0,
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::{Executor, HistoryRecord};
    use tempfile::TempDir;

    fn sample_record(cmd: &str, cwd: &str, executor: &str) -> HistoryRecord {
        let mut builder = HistoryRecord::builder()
            .command(cmd)
            .cwd(cwd)
            .exit_code(0)
            .duration_ms(10)
            .session_id("test");

        if executor != "user" {
            builder = builder.executor(Executor::External(executor.to_string()));
        }

        builder.build().unwrap()
    }

    #[test]
    fn test_stats_total_count() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let store = HistoryStore::open(&db_path).unwrap();
        store.insert(&sample_record("ls", "/tmp", "user")).unwrap();
        store
            .insert(&sample_record("pwd", "/home", "user"))
            .unwrap();

        let stats = HistoryStats::compute(&store).unwrap();
        assert_eq!(stats.total_commands, 2);
    }

    #[test]
    fn test_stats_empty_store() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let store = HistoryStore::open(&db_path).unwrap();

        let stats = HistoryStats::compute(&store).unwrap();
        assert_eq!(stats.total_commands, 0);
        assert!(stats.earliest_timestamp.is_none());
        assert!(stats.executor_counts.is_empty());
    }

    #[test]
    fn test_stats_executor_breakdown() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let store = HistoryStore::open(&db_path).unwrap();
        store.insert(&sample_record("ls", "/tmp", "user")).unwrap();
        store.insert(&sample_record("pwd", "/tmp", "user")).unwrap();
        store
            .insert(&sample_record("cargo build", "/tmp", "claude-code"))
            .unwrap();

        let stats = HistoryStats::compute(&store).unwrap();
        assert_eq!(stats.executor_counts.len(), 2);
    }

    #[test]
    fn test_stats_top_directories() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let store = HistoryStore::open(&db_path).unwrap();
        store
            .insert(&sample_record("ls", "/project", "user"))
            .unwrap();
        store
            .insert(&sample_record("pwd", "/project", "user"))
            .unwrap();
        store.insert(&sample_record("ls", "/home", "user")).unwrap();

        let stats = HistoryStats::compute(&store).unwrap();
        assert_eq!(stats.top_directories.len(), 2);
        // /project should be first (2 commands)
        assert_eq!(stats.top_directories[0].0, "/project");
        assert_eq!(stats.top_directories[0].1, 2);
    }

    #[test]
    fn test_stats_recent_counts() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let store = HistoryStore::open(&db_path).unwrap();
        store.insert(&sample_record("ls", "/tmp", "user")).unwrap();

        let stats = HistoryStats::compute(&store).unwrap();
        // The record was just inserted, so it's within today/week/month
        assert!(stats.today_count >= 1);
        assert!(stats.week_count >= 1);
        assert!(stats.month_count >= 1);
    }
}
