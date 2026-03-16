//! Background writer for non-blocking history persistence.
//!
//! Uses a bounded channel (capacity 256) to decouple command execution
//! from SQLite I/O. The `Drop` impl closes the channel and joins the
//! writer thread, ensuring deterministic flush on shutdown.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use tracing::warn;

use crate::record::HistoryRecord;
use crate::store::HistoryStore;

const CHANNEL_CAPACITY: usize = 256;

/// Combined handle for the background writer system.
///
/// Owns the sender channel and background thread. On drop, closes the
/// channel and joins the thread to ensure all queued records are flushed.
pub struct HistoryWriter {
    sender: Option<std::sync::mpsc::SyncSender<HistoryRecord>>,
    drops: Arc<AtomicU64>,
    handle: Option<JoinHandle<()>>,
}

impl HistoryWriter {
    /// Start the background writer thread.
    ///
    /// Records sent via [`send()`](Self::send) are inserted into the
    /// store on a dedicated thread.
    pub fn start(store: Arc<HistoryStore>) -> Self {
        let (sender, receiver) = std::sync::mpsc::sync_channel(CHANNEL_CAPACITY);
        let drops = Arc::new(AtomicU64::new(0));
        let drops_clone = drops.clone();

        let handle = thread::Builder::new()
            .name("colony-history-writer".to_string())
            .spawn(move || {
                for record in receiver {
                    if let Err(e) = store.insert(&record) {
                        warn!("failed to write history record: {e}");
                        drops_clone.fetch_add(1, Ordering::Relaxed);
                    }
                }
            })
            .expect("failed to spawn history writer thread");

        Self {
            sender: Some(sender),
            drops,
            handle: Some(handle),
        }
    }

    /// Send a record to the background writer.
    ///
    /// Uses `try_send` to avoid blocking the command execution path.
    /// If the channel is full, the record is dropped and the drop
    /// counter is incremented.
    pub fn send(&self, record: HistoryRecord) {
        let Some(ref sender) = self.sender else {
            return;
        };
        if sender.try_send(record).is_err() {
            self.drops.fetch_add(1, Ordering::Relaxed);
            warn!("history write channel full -- record dropped");
        }
    }

    /// Number of records dropped due to channel backpressure or write
    /// errors.
    pub fn drop_count(&self) -> u64 {
        self.drops.load(Ordering::Relaxed)
    }
}

impl Drop for HistoryWriter {
    fn drop(&mut self) {
        // Drop sender first to close the channel and signal shutdown
        self.sender.take();
        // Then join the thread to ensure all records are flushed
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::HistoryRecord;
    use crate::store::HistoryStore;
    use tempfile::TempDir;

    fn sample_record(cmd: &str) -> HistoryRecord {
        HistoryRecord::builder()
            .command(cmd)
            .cwd("/tmp")
            .exit_code(0)
            .duration_ms(5)
            .session_id("test")
            .build()
            .unwrap()
    }

    #[test]
    fn test_writer_processes_records() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let store = Arc::new(HistoryStore::open(&db_path).unwrap());

        let writer = HistoryWriter::start(store.clone());
        writer.send(sample_record("echo hello"));
        writer.send(sample_record("ls -la"));

        // Drop joins the thread -- all records flushed deterministically
        drop(writer);

        assert_eq!(store.count().unwrap(), 2);
    }

    #[test]
    fn test_writer_drop_count_starts_zero() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let store = Arc::new(HistoryStore::open(&db_path).unwrap());

        let writer = HistoryWriter::start(store);
        assert_eq!(writer.drop_count(), 0);
    }

    #[test]
    fn test_writer_multiple_records() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let store = Arc::new(HistoryStore::open(&db_path).unwrap());

        let writer = HistoryWriter::start(store.clone());
        for i in 0..10 {
            writer.send(sample_record(&format!("command-{i}")));
        }
        drop(writer);

        assert_eq!(store.count().unwrap(), 10);
    }
}
