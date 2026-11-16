//! The ADE socket: the channel the Colony ADE application uses to talk to this machine's
//! Colony, served by the `colony-events` daemon.
//!
//! It is a Unix socket at `~/.colony/events/ade.sock`, mode 0600 in a 0700 directory, and
//! it refuses any peer whose uid is not the daemon's. Requests and replies are one JSON
//! object per line:
//!
//! ```text
//! → {"id": 1, "method": "feed"}
//! ← {"id": 1, "result": {"needs_you": [...], "feed": [...]}}
//! ← {"id": 2, "error": "no agent named x"}
//! ```
//!
//! | Method | Params | Result |
//! |---|---|---|
//! | `hello` | | `{protocol, pid, viewport}` |
//! | `agents` | | every agent: `{name, state, every_secs, next_beat, runs, image, grants, open_asks, notes, proposed_notes}` |
//! | `feed` | | what the viewport shows: `{needs_you, feed}` |
//! | `open` | `{address}` | `{text}` for an output or diff, else `{json}` |
//! | `answer` | `{ask, answer}` | `{seq}`; the log names the answer's actor `ade`. A proposed note's address takes `keep` or `drop` |
//! | `subscribe` | `{after?}` | `{subscribed: true}`, then `{"id", "event": {...}}` for every event after `after` (default: the newest), as it is appended, until the connection closes |
//!
//! A connection that subscribes sends nothing more; open another for requests.

use std::path::{Path, PathBuf};
use std::time::Duration;

use colony_tuple::{Address, EventLog};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

use crate::agents::{answer_ask, Agents};
use crate::viewport::{event_json, read_feed, resolve, Resolved};

/// The version of the protocol above.
pub const PROTOCOL: u32 = 1;

/// Longest request line read.
const LINE_MAX: usize = 64 * 1024;

/// How often a subscription looks for new events.
const SUBSCRIBE_EVERY: Duration = Duration::from_millis(200);

/// The socket's path.
pub fn socket_path() -> PathBuf {
    crate::events::daemon_pid_path().with_file_name("ade.sock")
}

/// Serve the socket at `path` until the task is dropped. `log_path` is the machine's log.
/// The caller holds the daemon lock, so a socket file already there is stale.
pub async fn serve(path: PathBuf, log_path: PathBuf) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    tracing::info!("ADE socket at {}", path.display());
    // SAFETY: geteuid has no preconditions and cannot fail.
    let uid = unsafe { libc::geteuid() };
    loop {
        let (stream, _) = listener.accept().await?;
        if stream.peer_cred().map(|c| c.uid()).ok() != Some(uid) {
            tracing::warn!("ADE socket: refused a peer that is not uid {uid}");
            continue;
        }
        let log_path = log_path.clone();
        tokio::spawn(async move {
            if let Err(e) = connection(stream, &log_path).await {
                tracing::debug!("ADE socket: connection ended: {e}");
            }
        });
    }
}

async fn connection(stream: UnixStream, log_path: &Path) -> std::io::Result<()> {
    let (read, mut write) = stream.into_split();
    let mut lines = BufReader::new(read);
    let mut line = String::new();
    loop {
        line.clear();
        let n = tokio::io::AsyncReadExt::take(&mut lines, LINE_MAX as u64 + 1)
            .read_line(&mut line)
            .await?;
        if n == 0 {
            return Ok(());
        }
        if line.len() > LINE_MAX {
            send(
                &mut write,
                &json!({ "id": null, "error": "request line too long" }),
            )
            .await?;
            return Ok(());
        }
        let request: Value = match serde_json::from_str(line.trim()) {
            Ok(v) => v,
            Err(e) => {
                send(
                    &mut write,
                    &json!({ "id": null, "error": format!("not JSON: {e}") }),
                )
                .await?;
                continue;
            }
        };
        let id = request["id"].clone();
        let method = request["method"].as_str().unwrap_or_default();
        if method == "subscribe" {
            return subscribe(
                &mut write,
                log_path,
                id,
                request["params"]["after"].as_i64(),
            )
            .await;
        }
        // SQLite calls block: off the async workers.
        let (path, method, params) = (
            log_path.to_path_buf(),
            method.to_string(),
            request["params"].clone(),
        );
        let outcome = tokio::task::spawn_blocking(move || {
            EventLog::open(&path)
                .map_err(|e| e.to_string())
                .and_then(|log| handle(&log, &method, &params))
        })
        .await
        .unwrap_or_else(|e| Err(e.to_string()));
        let reply = match outcome {
            Ok(result) => json!({ "id": id, "result": result }),
            Err(e) => json!({ "id": id, "error": e }),
        };
        send(&mut write, &reply).await?;
    }
}

async fn send(write: &mut (impl AsyncWriteExt + Unpin), value: &Value) -> std::io::Result<()> {
    let mut line = value.to_string();
    line.push('\n');
    write.write_all(line.as_bytes()).await
}

/// Answer one request.
fn handle(log: &EventLog, method: &str, params: &Value) -> Result<Value, String> {
    let text = |key: &str| {
        params[key]
            .as_str()
            .ok_or_else(|| format!("{method} needs a string '{key}'"))
    };
    match method {
        "hello" => Ok(json!({
            "protocol": PROTOCOL,
            "pid": std::process::id(),
            "viewport": std::fs::read_to_string(crate::viewport::url_file()).ok().map(|u| u.trim().to_string()),
        })),
        "agents" => {
            let agents = Agents::from_log(log)?;
            Ok(Value::Array(
                agents
                    .iter()
                    .map(|a| {
                        json!({
                            "name": a.name,
                            "address": a.address().to_string(),
                            "state": a.state,
                            "every_secs": a.spec.every_secs,
                            "next_beat": a.next_beat_ms(),
                            "runs": a.runs,
                            "image": a.spec.image,
                            "grants": a.spec.grants,
                            "open_asks": a.open_asks().map(|q| q.address.clone()).collect::<Vec<_>>(),
                            "notes": a.notes,
                            "proposed_notes": a.proposed.iter().map(|p| json!({"address": p.address, "note": p.question})).collect::<Vec<_>>(),
                        })
                    })
                    .collect(),
            ))
        }
        "feed" => serde_json::to_value(read_feed(log)?).map_err(|e| e.to_string()),
        "open" => {
            let addr: Address = text("address")?
                .parse()
                .map_err(|e: colony_tuple::AddressError| e.to_string())?;
            if !addr.is_local() {
                return Err("only local addresses resolve".to_string());
            }
            match resolve(log, &addr).map_err(|e| e.to_string())? {
                Some(Resolved::Text(t)) => Ok(json!({ "text": t })),
                Some(Resolved::Json(v)) => Ok(json!({ "json": v })),
                None => Err(format!("nothing at {addr}")),
            }
        }
        "answer" => {
            let seq = answer_ask(log, "ade", text("ask")?, text("answer")?)?;
            Ok(json!({ "seq": seq }))
        }
        "" => Err("missing method".to_string()),
        other => Err(format!("unknown method '{other}'")),
    }
}

/// Stream every event after `after` (default: the newest now) until the peer goes.
async fn subscribe(
    write: &mut (impl AsyncWriteExt + Unpin),
    log_path: &Path,
    id: Value,
    after: Option<i64>,
) -> std::io::Result<()> {
    let mut log = EventLog::open(log_path).map_err(std::io::Error::other)?;
    let mut seq = match after {
        Some(seq) => seq,
        None => log
            .last(1)
            .map_err(std::io::Error::other)?
            .first()
            .map_or(0, |e| e.seq),
    };
    send(
        write,
        &json!({ "id": id, "result": { "subscribed": true, "after": seq } }),
    )
    .await?;
    loop {
        // SQLite calls block: the log goes to a blocking thread and back each poll.
        let (back, page) = tokio::task::spawn_blocking(move || {
            let page = log.after(seq, 500);
            (log, page)
        })
        .await
        .map_err(std::io::Error::other)?;
        log = back;
        let page = page.map_err(std::io::Error::other)?;
        for ev in &page {
            send(write, &json!({ "id": id, "event": event_json(ev) })).await?;
            seq = ev.seq;
        }
        if page.is_empty() {
            tokio::time::sleep(SUBSCRIBE_EVERY).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_name_a_known_method_and_their_params() {
        let dir = tempfile::tempdir().unwrap();
        let log = EventLog::open(&dir.path().join("m.db")).unwrap();
        assert_eq!(handle(&log, "agents", &Value::Null).unwrap(), json!([]));
        assert_eq!(
            handle(&log, "hello", &Value::Null).unwrap()["protocol"],
            PROTOCOL
        );
        assert!(handle(&log, "nope", &Value::Null)
            .unwrap_err()
            .contains("unknown method"));
        assert!(handle(&log, "open", &json!({}))
            .unwrap_err()
            .contains("'address'"));
        assert!(
            handle(&log, "open", &json!({"address": "colony://box/event/1"}))
                .unwrap_err()
                .contains("only local")
        );
        assert!(handle(
            &log,
            "answer",
            &json!({"ask": "colony://local/agent/a/run/1/ask/1", "answer": "y"})
        )
        .unwrap_err()
        .contains("no agent named a"));
    }
}
