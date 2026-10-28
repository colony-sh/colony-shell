//! Handler process spawning for the event router daemon.
//!
//! Each handler spawns as an independent process with tuple values injected
//! as environment variables and positional parameters. Handlers have a 30s
//! timeout and stdout/stderr is captured (first 500 chars each).

use colony_exec::events::{EventListener, HandlerRun, HandlerType};
use std::time::{Duration, Instant};
use tokio::process::Command;

/// Maximum handler execution time.
const HANDLER_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum captured output length per stream.
const MAX_OUTPUT_LEN: usize = 500;

/// Result of a handler execution.
#[derive(Debug)]
pub struct HandlerResult {
    pub exit_code: Option<i32>,
    pub duration_ms: i64,
    pub stdout_snippet: Option<String>,
    pub stderr_snippet: Option<String>,
}

/// Spawn a handler process for a matched event.
///
/// Injects tuple values as:
/// - Environment: `TUPLE_KEY`, `TUPLE_0..N`, `TUPLE_COUNT`
/// - Positional params (for scripts): all tuple values as args
///
/// Returns the handler result after completion or timeout.
pub async fn spawn_handler(listener: &EventListener, tuple_values: &[String]) -> HandlerResult {
    let start = Instant::now();

    let result =
        tokio::time::timeout(HANDLER_TIMEOUT, execute_handler(listener, tuple_values)).await;

    let duration_ms = start.elapsed().as_millis() as i64;

    match result {
        Ok(Ok((exit_code, stdout, stderr))) => HandlerResult {
            exit_code: Some(exit_code),
            duration_ms,
            stdout_snippet: truncate_output(&stdout),
            stderr_snippet: truncate_output(&stderr),
        },
        Ok(Err(e)) => HandlerResult {
            exit_code: Some(127),
            duration_ms,
            stdout_snippet: None,
            stderr_snippet: Some(format!("spawn error: {e}")),
        },
        Err(_timeout) => HandlerResult {
            exit_code: Some(124), // timeout exit code convention
            duration_ms,
            stdout_snippet: None,
            stderr_snippet: Some("handler timed out (30s)".to_string()),
        },
    }
}

/// Build and execute the handler command.
async fn execute_handler(
    listener: &EventListener,
    tuple_values: &[String],
) -> Result<(i32, String, String), std::io::Error> {
    let mut cmd = match listener.handler_type {
        HandlerType::CommandEval => {
            let mut c = Command::new("colony-shell");
            c.arg("-c").arg(&listener.handler);
            c
        }
        HandlerType::Script => {
            let mut c = Command::new(&listener.handler);
            // Pass all tuple values as positional arguments
            for val in tuple_values {
                c.arg(val);
            }
            c
        }
        HandlerType::Function => {
            let mut c = Command::new("colony-shell");
            c.arg("-c").arg(format!("{} \"$@\"", listener.handler));
            // Pass tuple values as positional args after "--"
            c.arg("--");
            for val in tuple_values {
                c.arg(val);
            }
            c
        }
    };

    // Inject tuple environment variables
    if let Some(key) = tuple_values.first() {
        cmd.env("TUPLE_KEY", key);
    }
    for (i, val) in tuple_values.iter().enumerate() {
        cmd.env(format!("TUPLE_{i}"), val);
    }
    cmd.env("TUPLE_COUNT", tuple_values.len().to_string());
    // Timeout drops this future; kill the child with it so it can't outlive the 30s cap.
    cmd.kill_on_drop(true);

    let output = cmd.output().await?;

    let exit_code = output.status.code().unwrap_or(1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    Ok((exit_code, stdout, stderr))
}

/// Convert tuple values to a `HandlerRun` record.
pub fn make_handler_run(
    listener: &EventListener,
    tuple_values: &[String],
    result: &HandlerResult,
) -> HandlerRun {
    HandlerRun {
        id: uuid::Uuid::new_v4().to_string(),
        listener_id: listener.id.clone(),
        pattern: listener.pattern.clone(),
        tuple_values: tuple_values.to_vec(),
        handler: listener.handler.clone(),
        exit_code: result.exit_code,
        duration_ms: result.duration_ms,
        stdout_snippet: result.stdout_snippet.clone(),
        stderr_snippet: result.stderr_snippet.clone(),
        timestamp: chrono::Utc::now().timestamp(),
    }
}

/// Truncate output to first `MAX_OUTPUT_LEN` chars, returning `None` for empty.
fn truncate_output(s: &str) -> Option<String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }
    match trimmed.char_indices().nth(MAX_OUTPUT_LEN) {
        Some((end, _)) => Some(format!("{}...", &trimmed[..end])),
        None => Some(trimmed.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_output_multibyte_does_not_panic() {
        let s = "é".repeat(MAX_OUTPUT_LEN + 10);
        let out = truncate_output(&s).unwrap();
        assert_eq!(out.chars().count(), MAX_OUTPUT_LEN + 3);
    }

    #[test]
    fn test_truncate_output_short_and_empty() {
        assert_eq!(truncate_output("  hi \n").as_deref(), Some("hi"));
        assert_eq!(truncate_output("   "), None);
    }
}
