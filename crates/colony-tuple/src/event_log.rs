//! The machine's event log: append-only and totally ordered.
//!
//! Every state change in Colony is exactly one event here. State is a fold over the
//! log, so replaying it from the start rebuilds the same state after a restart.
//!
//! The log is one table in the machine's SQLite file, beside the tuple space
//! (`~/.config/colony/tuple-space.db`). SQLite triggers refuse every `UPDATE` and
//! `DELETE`, so no process can rewrite history. Sequence numbers come from
//! `AUTOINCREMENT` inside SQLite's single writer, so they are unique, never reused,
//! and a reader that has seen `seq` will never later see a smaller one commit.
//!
//! ```sql
//! CREATE TABLE events (
//!     seq     INTEGER PRIMARY KEY AUTOINCREMENT,
//!     at      INTEGER NOT NULL,  -- Unix milliseconds, for people; order by seq
//!     actor   TEXT NOT NULL,     -- who caused it: "daemon", a person, an agent name
//!     cause   INTEGER,           -- the event that led to this one
//!     kind    TEXT NOT NULL,     -- "AgentCreated", "HeartbeatFired", ...
//!     subject TEXT,              -- the colony:// address it is about
//!     data    TEXT NOT NULL      -- JSON object
//! );
//! ```

use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde_json::Value;

use crate::address::Address;
use crate::persistent::{default_db_path, StorageError};

/// One entry in the log.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    /// Position in the machine's total order. Starts at 1.
    pub seq: i64,
    /// When it was appended, in Unix milliseconds.
    pub at_ms: i64,
    /// Who caused it.
    pub actor: String,
    /// The event that led to this one.
    pub cause: Option<i64>,
    /// What happened, e.g. `"AgentCreated"`.
    pub kind: String,
    /// The `colony://` address it is about, as written.
    pub subject: Option<String>,
    /// The details, a JSON object.
    pub data: Value,
}

impl Event {
    /// This event's own address.
    pub fn address(&self) -> Address {
        Address::local(crate::address::Scope::Event(self.seq))
    }
}

/// A connection to the machine's event log.
pub struct EventLog {
    db: Connection,
}

impl EventLog {
    /// Open the log in the machine's SQLite file, creating it if needed.
    pub fn open_default() -> Result<Self, StorageError> {
        Self::open(&default_db_path())
    }

    /// Open the log in the SQLite file at `path`, creating it if needed.
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let db = Connection::open(path)?;
        // The daemon and every shell share this file.
        db.busy_timeout(Duration::from_secs(5))?;
        db.pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get::<_, String>(0))?;
        db.execute_batch(SCHEMA)?;
        Ok(Self { db })
    }

    /// Append one event and return its sequence number.
    ///
    /// `data` must be a JSON object; anything else is stored under `"value"`.
    pub fn append(
        &self,
        actor: &str,
        kind: &str,
        subject: Option<&Address>,
        cause: Option<i64>,
        data: Value,
    ) -> Result<i64, StorageError> {
        let data = match data {
            Value::Object(_) => data,
            Value::Null => Value::Object(serde_json::Map::new()),
            other => serde_json::json!({ "value": other }),
        };
        self.db.execute(
            "INSERT INTO events (at, actor, cause, kind, subject, data)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                now_ms(),
                actor,
                cause,
                kind,
                subject.map(ToString::to_string),
                data.to_string()
            ],
        )?;
        Ok(self.db.last_insert_rowid())
    }

    /// One event by sequence number.
    pub fn get(&self, seq: i64) -> Result<Option<Event>, StorageError> {
        self.db
            .query_row(&format!("{SELECT} WHERE seq = ?1"), params![seq], read_row)
            .optional()?
            .transpose()
    }

    /// Up to `limit` events after `seq`, oldest first. `after(0, ..)` starts at the top.
    ///
    /// Replay is a loop over this, carrying the last `seq` seen.
    pub fn after(&self, seq: i64, limit: usize) -> Result<Vec<Event>, StorageError> {
        self.query(
            &format!("{SELECT} WHERE seq > ?1 ORDER BY seq LIMIT ?2"),
            params![seq, limit_param(limit)],
        )
    }

    /// The last `limit` events, oldest first.
    pub fn last(&self, limit: usize) -> Result<Vec<Event>, StorageError> {
        let mut events = self.query(
            &format!("{SELECT} ORDER BY seq DESC LIMIT ?1"),
            params![limit_param(limit)],
        )?;
        events.reverse();
        Ok(events)
    }

    /// Every event about `addr` or about something under it, oldest first.
    ///
    /// For `colony://local/agent/x` that is the agent's own events and those of its
    /// runs, outputs and diffs.
    pub fn about(&self, addr: &Address) -> Result<Vec<Event>, StorageError> {
        let exact = addr.to_string();
        let under = format!("{exact}/");
        self.query(
            &format!(
                "{SELECT} WHERE subject = ?1 OR substr(subject, 1, length(?2)) = ?2 ORDER BY seq"
            ),
            params![exact, under],
        )
    }

    /// The newest event about exactly `addr`, if any.
    pub fn latest(&self, addr: &Address) -> Result<Option<Event>, StorageError> {
        self.db
            .query_row(
                &format!("{SELECT} WHERE subject = ?1 ORDER BY seq DESC LIMIT 1"),
                params![addr.to_string()],
                read_row,
            )
            .optional()?
            .transpose()
    }

    fn query(&self, sql: &str, params: impl rusqlite::Params) -> Result<Vec<Event>, StorageError> {
        let mut stmt = self.db.prepare(sql)?;
        let events = stmt.query_map(params, read_row)?.map(|row| row?).collect();
        events
    }
}

const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS events (
        seq     INTEGER PRIMARY KEY AUTOINCREMENT,
        at      INTEGER NOT NULL,
        actor   TEXT NOT NULL,
        cause   INTEGER REFERENCES events(seq),
        kind    TEXT NOT NULL,
        subject TEXT,
        data    TEXT NOT NULL DEFAULT '{}'
    );
    CREATE INDEX IF NOT EXISTS idx_events_subject ON events(subject) WHERE subject IS NOT NULL;
    CREATE TRIGGER IF NOT EXISTS events_no_update BEFORE UPDATE ON events
        BEGIN SELECT RAISE(ABORT, 'the event log is append-only'); END;
    CREATE TRIGGER IF NOT EXISTS events_no_delete BEFORE DELETE ON events
        BEGIN SELECT RAISE(ABORT, 'the event log is append-only'); END;
";

const SELECT: &str = "SELECT seq, at, actor, cause, kind, subject, data FROM events";

/// Read one row. The outer `Result` is SQLite's, the inner one the JSON's.
fn read_row(row: &Row<'_>) -> rusqlite::Result<Result<Event, StorageError>> {
    let data: String = row.get(6)?;
    Ok(serde_json::from_str(&data)
        .map_err(StorageError::from)
        .and_then(|data| {
            Ok(Event {
                seq: row.get(0)?,
                at_ms: row.get(1)?,
                actor: row.get(2)?,
                cause: row.get(3)?,
                kind: row.get(4)?,
                subject: row.get(5)?,
                data,
            })
        }))
}

fn limit_param(limit: usize) -> i64 {
    i64::try_from(limit).unwrap_or(i64::MAX)
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::address::Scope;
    use serde_json::json;

    fn temp_log() -> (EventLog, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let log = EventLog::open(&dir.path().join("machine.db")).unwrap();
        (log, dir)
    }

    fn agent(name: &str) -> Address {
        Address::local(Scope::Agent(name.into()))
    }

    fn run(name: &str, run: u64) -> Address {
        Address::local(Scope::Run {
            agent: name.into(),
            run,
        })
    }

    #[test]
    fn appends_are_totally_ordered_and_read_back() {
        let (log, _dir) = temp_log();
        let a = log
            .append(
                "jesse",
                "AgentCreated",
                Some(&agent("lead-miner")),
                None,
                json!({"charter": "find threads"}),
            )
            .unwrap();
        let b = log
            .append(
                "daemon",
                "HeartbeatFired",
                Some(&run("lead-miner", 1)),
                Some(a),
                json!({"beat": 1}),
            )
            .unwrap();
        assert_eq!((a, b), (1, 2));

        let ev = log.get(b).unwrap().unwrap();
        assert_eq!(ev.kind, "HeartbeatFired");
        assert_eq!(ev.cause, Some(a));
        assert_eq!(
            ev.subject.as_deref(),
            Some("colony://local/agent/lead-miner/run/1")
        );
        assert_eq!(ev.data, json!({"beat": 1}));
        assert_eq!(ev.address().to_string(), "colony://local/event/2");
        assert!(log.get(99).unwrap().is_none());
    }

    #[test]
    fn the_log_refuses_update_and_delete() {
        let (log, _dir) = temp_log();
        log.append("daemon", "Started", None, None, Value::Null)
            .unwrap();
        for sql in ["UPDATE events SET kind = 'X'", "DELETE FROM events"] {
            let err = log.db.execute(sql, []).unwrap_err().to_string();
            assert!(err.contains("append-only"), "{sql}: {err}");
        }
        assert_eq!(log.last(10).unwrap().len(), 1);
    }

    #[test]
    fn replay_after_reopen_sees_the_same_events() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("machine.db");
        let before = {
            let log = EventLog::open(&path).unwrap();
            for i in 0..5 {
                log.append("daemon", "Tick", None, None, json!({ "i": i }))
                    .unwrap();
            }
            log.after(0, 100).unwrap()
        };

        let log = EventLog::open(&path).unwrap();
        let mut replayed = Vec::new();
        let mut last = 0;
        loop {
            let page = log.after(last, 2).unwrap();
            let Some(tail) = page.last() else { break };
            last = tail.seq;
            replayed.extend(page);
        }
        assert_eq!(replayed, before);
        assert_eq!(
            log.append("daemon", "Tick", None, None, Value::Null)
                .unwrap(),
            6
        );
    }

    #[test]
    fn about_covers_an_address_and_everything_under_it() {
        let (log, _dir) = temp_log();
        log.append(
            "jesse",
            "AgentCreated",
            Some(&agent("lead-miner")),
            None,
            Value::Null,
        )
        .unwrap();
        log.append(
            "jesse",
            "AgentCreated",
            Some(&agent("lead")),
            None,
            Value::Null,
        )
        .unwrap();
        log.append(
            "daemon",
            "RunStarted",
            Some(&run("lead-miner", 1)),
            None,
            Value::Null,
        )
        .unwrap();
        log.append("daemon", "Tick", None, None, Value::Null)
            .unwrap();

        let seqs: Vec<i64> = log
            .about(&agent("lead-miner"))
            .unwrap()
            .iter()
            .map(|e| e.seq)
            .collect();
        assert_eq!(
            seqs,
            [1, 3],
            "a sibling whose name is a prefix must not match"
        );
        assert_eq!(log.about(&run("lead-miner", 1)).unwrap().len(), 1);
        assert!(log.about(&run("lead-miner", 2)).unwrap().is_empty());
    }

    #[test]
    fn latest_is_the_newest_event_about_exactly_that_address() {
        let (log, _dir) = temp_log();
        let out = Address::local(Scope::Output {
            agent: "x".into(),
            run: 1,
            n: 1,
        });
        log.append(
            "x",
            "OutputRecorded",
            Some(&out),
            None,
            json!({"content": "old"}),
        )
        .unwrap();
        log.append(
            "x",
            "OutputRecorded",
            Some(&out),
            None,
            json!({"content": "new"}),
        )
        .unwrap();
        let ev = log.latest(&out).unwrap().unwrap();
        assert_eq!(ev.data["content"], "new");
        assert!(log.latest(&run("x", 1)).unwrap().is_none());
    }

    #[test]
    fn non_object_data_is_wrapped() {
        let (log, _dir) = temp_log();
        let seq = log.append("x", "Note", None, None, json!("hello")).unwrap();
        assert_eq!(
            log.get(seq).unwrap().unwrap().data,
            json!({"value": "hello"})
        );
    }
}
