//! mirror — long-running builtin that tails the reflection database
//!
//! Opens `~/.colony/reflection/observations.db` read-only and polls for new
//! observations every few seconds.  When new observations arrive they are
//! formatted and (in the future) sent to an LLM for analysis.  If the LLM
//! returns a suggestion it is emitted to the host terminal via
//! [`colony_protocol::emit_mirror_suggestion`].
//!
//! The command runs in the foreground until interrupted with Ctrl-C.

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OpenFlags};

use crate::builtins::BuiltinResult;
use crate::{ExecutionError, Result};

/// How often the mirror loop polls the database for new observations.
const POLL_INTERVAL: Duration = Duration::from_secs(5);

/// Maximum number of observations to send per LLM analysis batch.
const MAX_BATCH_SIZE: usize = 50;

// ---------------------------------------------------------------------------
// LLM integration (placeholder)
// ---------------------------------------------------------------------------

/// System prompt for the mirror LLM analyst.
#[allow(dead_code)] // Used once LLM integration is wired
const MIRROR_SYSTEM_PROMPT: &str = "\
You are a developer's shadow assistant observing their work session.
You receive a stream of observations: commands run, errors encountered, files changed, agent conversations.

Rules:
- Only speak when you notice something genuinely useful
- Patterns worth mentioning: repeated failures, forgotten context, time spent on one task, potential shortcuts
- Keep suggestions brief (1-2 sentences)
- Respond with JSON: {\"suggestion\": \"text or null\", \"priority\": \"info or attention\"}
- Set priority to \"attention\" only for time-sensitive or high-value insights
- If nothing noteworthy, respond with {\"suggestion\": null}";

/// Analyze observations and return a suggestion if noteworthy.
///
/// Returns `Some((text, priority))` or `None`.
#[allow(unused_variables)]
async fn analyze_observations(context: &str) -> Option<(String, String)> {
    // TODO: Wire to ModelRouter for LLM analysis
    //
    // The implementation should:
    // 1. Create a CompletionRequest with MIRROR_SYSTEM_PROMPT as system prompt
    // 2. Pass the formatted observations as the user message
    // 3. Parse the JSON response to extract suggestion and priority
    // 4. Return Some((text, priority)) if suggestion is non-null, else None
    //
    // Example wiring (once colony-agent is a dependency):
    //
    //   let mut router = ModelRouter::auto_detect().await.ok()?;
    //   let request = CompletionRequest {
    //       system_prompt: Some(MIRROR_SYSTEM_PROMPT.to_string()),
    //       messages: vec![Message { role: MessageRole::User, content: MessageContent::Text(context.to_string()) }],
    //       max_tokens: 256,
    //       tools: vec![],
    //       temperature: Some(0.3),
    //       enable_caching: false,
    //   };
    //   ... collect stream into text, parse JSON ...

    tracing::debug!(
        "mirror: would analyze {} chars of observations",
        context.len()
    );
    None
}

// ---------------------------------------------------------------------------
// Observation row (lightweight, avoids full Observation deserialization)
// ---------------------------------------------------------------------------

/// A lightweight row from the observations table, sufficient for formatting.
#[derive(Debug)]
struct ObservationRow {
    timestamp: u64,
    category: String,
    data: String,
    session_id: String,
}

impl ObservationRow {
    /// Format the observation as a concise line for the LLM context window.
    fn format_for_llm(&self) -> String {
        // Truncate data to a reasonable length to avoid blowing up context
        let data_preview = if self.data.len() > 300 {
            format!("{}...", &self.data[..300])
        } else {
            self.data.clone()
        };

        format!(
            "[t={} session={} cat={}] {}",
            self.timestamp, self.session_id, self.category, data_preview
        )
    }
}

// ---------------------------------------------------------------------------
// Database helpers
// ---------------------------------------------------------------------------

/// Open the observations database read-only.
fn open_db(path: &PathBuf) -> Result<Connection> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(
        |e| {
            ExecutionError::IoError(std::io::Error::other(
                format!("mirror: failed to open database {}: {}", path.display(), e),
            ))
        },
    )?;

    // Use WAL mode read-only so we don't block the writer
    conn.pragma_update(None, "journal_mode", "wal")
        .unwrap_or_default();

    Ok(conn)
}

/// Query observations newer than the given timestamp.
fn query_new_observations(
    conn: &Connection,
    since_timestamp: u64,
) -> Result<Vec<ObservationRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT timestamp, category, data, session_id \
             FROM observations \
             WHERE timestamp > ?1 \
             ORDER BY timestamp ASC \
             LIMIT ?2",
        )
        .map_err(|e| {
            ExecutionError::IoError(std::io::Error::other(
                format!("mirror: query prepare failed: {e}"),
            ))
        })?;

    let rows = stmt
        .query_map(
            rusqlite::params![since_timestamp, MAX_BATCH_SIZE as i64],
            |row| {
                Ok(ObservationRow {
                    timestamp: row.get(0)?,
                    category: row.get(1)?,
                    data: row.get(2)?,
                    session_id: row.get(3)?,
                })
            },
        )
        .map_err(|e| {
            ExecutionError::IoError(std::io::Error::other(
                format!("mirror: query failed: {e}"),
            ))
        })?;

    let mut observations = Vec::new();
    for row in rows {
        let obs = row.map_err(|e| {
            ExecutionError::IoError(std::io::Error::other(
                format!("mirror: row read failed: {e}"),
            ))
        })?;
        observations.push(obs);
    }

    Ok(observations)
}

// ---------------------------------------------------------------------------
// Parse CLI flags
// ---------------------------------------------------------------------------

/// Parsed mirror command arguments.
struct MirrorArgs {
    db_path: PathBuf,
    verbose: bool,
}

fn parse_mirror_args(args: &[String]) -> std::result::Result<MirrorArgs, String> {
    let mut db_path: Option<PathBuf> = None;
    let mut verbose = false;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "-v" | "--verbose" => verbose = true,
            "--db" => {
                i += 1;
                let Some(path) = args.get(i) else {
                    return Err("mirror: --db requires a path argument".to_string());
                };
                db_path = Some(PathBuf::from(path));
            }
            "-h" | "--help" => {
                return Err(
                    "Usage: mirror [--db PATH] [-v|--verbose]\n\n\
                     Tail the reflection database and surface LLM-powered suggestions.\n\
                     Press Ctrl-C to stop."
                        .to_string(),
                );
            }
            other => {
                return Err(format!("mirror: unknown option: {other}"));
            }
        }
        i += 1;
    }

    let db_path = db_path.unwrap_or_else(|| {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".colony/reflection/observations.db")
    });

    Ok(MirrorArgs { db_path, verbose })
}

// ---------------------------------------------------------------------------
// Main loop
// ---------------------------------------------------------------------------

/// Run the mirror observation loop.
///
/// This function blocks until the process receives SIGINT (Ctrl-C).
/// It is called from the builtin dispatcher via [`builtin_mirror`].
fn run_mirror_loop(ma: &MirrorArgs) -> Result<BuiltinResult> {
    // Open the database
    let conn = open_db(&ma.db_path)?;

    // Start from "now" so we only see new observations
    let mut last_seen_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX epoch")
        .as_millis() as u64;

    if ma.verbose {
        eprintln!(
            "mirror: tailing {} (since ts={})",
            ma.db_path.display(),
            last_seen_ts
        );
    }

    // Install a simple Ctrl-C flag via std atomic
    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let r = running.clone();

    // We use signal_hook for a non-panicking SIGINT handler
    // SAFETY: signal_hook::flag::register is safe to call with valid signal numbers
    // and an AtomicBool. SIGINT is always a valid signal.
    if let Err(e) =
        signal_hook::flag::register(signal_hook::consts::SIGINT, std::sync::Arc::clone(&r))
    {
        tracing::warn!("mirror: could not register SIGINT handler: {e}");
    }

    eprintln!("mirror: watching for observations... (Ctrl-C to stop)");

    while running.load(std::sync::atomic::Ordering::Relaxed) {
        let observations = query_new_observations(&conn, last_seen_ts)?;

        if !observations.is_empty() {
            // Update watermark
            if let Some(last) = observations.last() {
                last_seen_ts = last.timestamp;
            }

            if ma.verbose {
                eprintln!(
                    "mirror: {} new observation(s), watermark={}",
                    observations.len(),
                    last_seen_ts
                );
            }

            // Format observations for LLM context
            let context: String = observations
                .iter()
                .map(|o| o.format_for_llm())
                .collect::<Vec<_>>()
                .join("\n");

            // Attempt LLM analysis (currently a placeholder)
            let suggestion = tokio::runtime::Handle::current().block_on(async {
                analyze_observations(&context).await
            });

            if let Some((text, priority)) = suggestion {
                colony_protocol::emit_mirror_suggestion(&text, &priority);
                if ma.verbose {
                    eprintln!("mirror: emitted suggestion (priority={priority}): {text}");
                }
            }
        }

        // Sleep in short increments so Ctrl-C is responsive
        let deadline = std::time::Instant::now() + POLL_INTERVAL;
        while std::time::Instant::now() < deadline
            && running.load(std::sync::atomic::Ordering::Relaxed)
        {
            std::thread::sleep(Duration::from_millis(250));
        }
    }

    eprintln!("\nmirror: stopped");
    Ok(BuiltinResult::Success(0))
}

// ---------------------------------------------------------------------------
// Builtin entry point
// ---------------------------------------------------------------------------

/// Execute the `mirror` builtin command.
///
/// Opens the reflection database and enters a polling loop that watches for
/// new observations, formats them, and (in the future) sends them to an LLM
/// for analysis.  Suggestions are emitted via OSC 2337 MirrorSuggestion
/// messages.
///
/// The command blocks until interrupted with Ctrl-C.
pub fn builtin_mirror(args: &[String]) -> Result<BuiltinResult> {
    let ma = match parse_mirror_args(args) {
        Ok(ma) => ma,
        Err(e) => {
            eprintln!("{e}");
            return Ok(BuiltinResult::Failure(1));
        }
    };

    // Check that the database exists before entering the loop
    if !ma.db_path.exists() {
        eprintln!(
            "mirror: database not found at {}\n\
             The reflection service must be running to generate observations.\n\
             Hint: the shell creates this database automatically when started with an observer.",
            ma.db_path.display()
        );
        return Ok(BuiltinResult::Failure(1));
    }

    run_mirror_loop(&ma)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    /// Create a temporary SQLite database with the observations schema.
    fn create_test_db() -> (NamedTempFile, PathBuf) {
        let file = NamedTempFile::new().expect("create temp file");
        let path = file.path().to_path_buf();

        let conn = Connection::open(&path).expect("open db");
        conn.execute_batch(
            "CREATE TABLE observations (
                id TEXT PRIMARY KEY,
                timestamp INTEGER NOT NULL,
                category TEXT NOT NULL,
                data TEXT NOT NULL,
                session_id TEXT NOT NULL,
                cwd TEXT,
                git_branch TEXT,
                command_count INTEGER,
                time_since_last_ms INTEGER,
                time_of_day TEXT,
                day_of_week TEXT,
                project TEXT,
                focus_duration_ms INTEGER,
                energy_level REAL
            );
            CREATE INDEX idx_obs_timestamp ON observations(timestamp);",
        )
        .expect("create schema");

        (file, path)
    }

    /// Insert a test observation row.
    fn insert_observation(conn: &Connection, ts: u64, category: &str, data: &str, session: &str) {
        conn.execute(
            "INSERT INTO observations (id, timestamp, category, data, session_id) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                uuid::Uuid::new_v4().to_string(),
                ts,
                category,
                data,
                session
            ],
        )
        .expect("insert observation");
    }

    #[test]
    fn test_parse_args_defaults() {
        let args: Vec<String> = vec![];
        let ma = parse_mirror_args(&args).expect("parse empty args");
        assert!(!ma.verbose);
        assert!(ma.db_path.to_str().unwrap().contains("observations.db"));
    }

    #[test]
    fn test_parse_args_verbose() {
        let args = vec!["-v".to_string()];
        let ma = parse_mirror_args(&args).expect("parse -v");
        assert!(ma.verbose);
    }

    #[test]
    fn test_parse_args_custom_db() {
        let args = vec!["--db".to_string(), "/tmp/test.db".to_string()];
        let ma = parse_mirror_args(&args).expect("parse --db");
        assert_eq!(ma.db_path, PathBuf::from("/tmp/test.db"));
    }

    #[test]
    fn test_parse_args_help_returns_error() {
        let args = vec!["--help".to_string()];
        let result = parse_mirror_args(&args);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_args_unknown_option() {
        let args = vec!["--bogus".to_string()];
        let result = parse_mirror_args(&args);
        assert!(result.is_err());
    }

    #[test]
    fn test_open_db_read_only() {
        let (_file, path) = create_test_db();
        let conn = open_db(&path).expect("open read-only");
        // Verify we can query
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM observations", [], |row| row.get(0))
            .expect("count");
        assert_eq!(count, 0);
    }

    #[test]
    fn test_query_new_observations_empty() {
        let (_file, path) = create_test_db();
        let conn = open_db(&path).expect("open db");
        let obs = query_new_observations(&conn, 0).expect("query");
        assert!(obs.is_empty());
    }

    #[test]
    fn test_query_new_observations_returns_newer() {
        let (_file, path) = create_test_db();

        // Insert via a writable connection
        {
            let write_conn = Connection::open(&path).expect("open writable");
            insert_observation(&write_conn, 1000, "command", r#"{"input":"ls"}"#, "s1");
            insert_observation(&write_conn, 2000, "error", r#"{"message":"oops"}"#, "s1");
            insert_observation(&write_conn, 3000, "command", r#"{"input":"pwd"}"#, "s1");
        }

        let conn = open_db(&path).expect("open read-only");

        // Query since ts=1500 should return only the 2000 and 3000 rows
        let obs = query_new_observations(&conn, 1500).expect("query");
        assert_eq!(obs.len(), 2);
        assert_eq!(obs[0].timestamp, 2000);
        assert_eq!(obs[1].timestamp, 3000);
    }

    #[test]
    fn test_query_respects_max_batch_size() {
        let (_file, path) = create_test_db();

        {
            let write_conn = Connection::open(&path).expect("open writable");
            for i in 0..(MAX_BATCH_SIZE + 10) {
                insert_observation(
                    &write_conn,
                    (i + 1) as u64,
                    "command",
                    r#"{"input":"echo hi"}"#,
                    "s1",
                );
            }
        }

        let conn = open_db(&path).expect("open read-only");
        let obs = query_new_observations(&conn, 0).expect("query");
        assert_eq!(obs.len(), MAX_BATCH_SIZE);
    }

    #[test]
    fn test_observation_row_format_for_llm() {
        let row = ObservationRow {
            timestamp: 1700000000,
            category: "command".to_string(),
            data: r#"{"input":"ls -la"}"#.to_string(),
            session_id: "sess-123".to_string(),
        };
        let formatted = row.format_for_llm();
        assert!(formatted.contains("t=1700000000"));
        assert!(formatted.contains("cat=command"));
        assert!(formatted.contains("ls -la"));
    }

    #[test]
    fn test_observation_row_format_truncates_long_data() {
        let long_data = "x".repeat(500);
        let row = ObservationRow {
            timestamp: 100,
            category: "error".to_string(),
            data: long_data,
            session_id: "s".to_string(),
        };
        let formatted = row.format_for_llm();
        assert!(formatted.len() < 400);
        assert!(formatted.ends_with("..."));
    }

    #[test]
    fn test_builtin_mirror_missing_db() {
        let result = builtin_mirror(&["--db".to_string(), "/nonexistent/path.db".to_string()]);
        let br = result.expect("should not error at Result level");
        assert_eq!(br, BuiltinResult::Failure(1));
    }
}
