//! Unix socket server for widget IPC with colony-terminal
//!
//! ## Protocol
//!
//! Messages are length-prefixed JSON:
//! - 4 bytes: message length (big-endian u32)
//! - N bytes: JSON payload (ProtocolMessage)
//!
//! ## Socket Location
//!
//! `$XDG_RUNTIME_DIR/colony-$PID.sock` or `/tmp/colony-$PID.sock`.
//! Exported as `COLONY_WIDGET_SOCKET` for terminal discovery.

use colony_protocol::ProtocolMessage;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, RwLock};

use crate::widget_handler::{handle_widget_action, handle_widget_request};

pub struct WidgetServer {
    socket_path: PathBuf,
    cwd: Arc<RwLock<PathBuf>>,
    shutdown_tx: broadcast::Sender<()>,
}

impl WidgetServer {
    pub fn new(cwd: PathBuf) -> std::io::Result<Self> {
        let socket_path = Self::socket_path_for_pid(std::process::id());

        if socket_path.exists() {
            std::fs::remove_file(&socket_path)?;
        }

        if let Some(parent) = socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let (shutdown_tx, _) = broadcast::channel(1);

        Ok(Self {
            socket_path,
            cwd: Arc::new(RwLock::new(cwd)),
            shutdown_tx,
        })
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    pub async fn set_cwd(&self, cwd: PathBuf) {
        *self.cwd.write().await = cwd;
    }

    pub fn cwd_handle(&self) -> Arc<RwLock<PathBuf>> {
        self.cwd.clone()
    }

    pub fn shutdown_handle(&self) -> broadcast::Sender<()> {
        self.shutdown_tx.clone()
    }

    pub async fn run(&self) -> std::io::Result<()> {
        let listener = UnixListener::bind(&self.socket_path)?;
        tracing::info!("Widget server listening on {:?}", self.socket_path);

        let mut shutdown_rx = self.shutdown_tx.subscribe();

        loop {
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok((stream, _addr)) => {
                            let cwd = self.cwd.clone();
                            tokio::spawn(async move {
                                if let Err(e) = handle_connection(stream, cwd).await {
                                    tracing::warn!("Widget connection error: {}", e);
                                }
                            });
                        }
                        Err(e) => {
                            tracing::error!("Accept error: {}", e);
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    tracing::info!("Widget server shutting down");
                    break;
                }
            }
        }

        let _ = std::fs::remove_file(&self.socket_path);
        Ok(())
    }

    fn socket_path_for_pid(pid: u32) -> PathBuf {
        if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
            return PathBuf::from(runtime_dir).join(format!("colony-{}.sock", pid));
        }
        PathBuf::from(format!("/tmp/colony-{}.sock", pid))
    }
}

impl Drop for WidgetServer {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

async fn handle_connection(
    mut stream: UnixStream,
    cwd: Arc<RwLock<PathBuf>>,
) -> std::io::Result<()> {
    tracing::debug!("New widget connection");

    loop {
        let mut len_buf = [0u8; 4];
        match stream.read_exact(&mut len_buf).await {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e),
        }

        let msg_len = u32::from_be_bytes(len_buf) as usize;

        if msg_len > 1024 * 1024 {
            tracing::warn!("Message too large: {} bytes", msg_len);
            break;
        }

        let mut msg_buf = vec![0u8; msg_len];
        stream.read_exact(&mut msg_buf).await?;

        let json = match std::str::from_utf8(&msg_buf) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("Invalid UTF-8 in message: {}", e);
                continue;
            }
        };

        tracing::debug!("Received widget message: {}", json);

        let cwd_path = cwd.read().await.clone();
        let response = process_widget_message(json, &cwd_path);

        if let Some(response_json) = response {
            let response_bytes = response_json.as_bytes();
            let len_bytes = (response_bytes.len() as u32).to_be_bytes();

            stream.write_all(&len_bytes).await?;
            stream.write_all(response_bytes).await?;
            stream.flush().await?;
        }
    }

    tracing::debug!("Widget connection closed");
    Ok(())
}

fn process_widget_message(json: &str, cwd: &Path) -> Option<String> {
    let msg: ProtocolMessage = match serde_json::from_str(json) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!("Failed to parse widget message: {}", e);
            return None;
        }
    };

    match msg {
        ProtocolMessage::WidgetRequest {
            widget,
            component_id,
            context,
        } => {
            let req_json = serde_json::json!({
                "type": "WidgetRequest",
                "widget": widget,
                "component_id": component_id,
                "context": context
            });
            handle_widget_request(&req_json.to_string(), cwd);
            None
        }
        ProtocolMessage::WidgetAction {
            widget,
            action,
            payload,
        } => {
            let action_json = serde_json::json!({
                "type": "WidgetAction",
                "widget": widget,
                "action": action,
                "payload": payload
            });
            handle_widget_action(&action_json.to_string(), cwd);
            None
        }
        _ => {
            tracing::debug!("Ignoring non-widget message: {:?}", msg);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_socket_path_generation() {
        let path = WidgetServer::socket_path_for_pid(12345);
        assert!(path.to_string_lossy().contains("colony-12345.sock"));
    }

    #[tokio::test]
    async fn test_server_lifecycle() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cwd = temp_dir.path().to_path_buf();
        std::env::set_var("XDG_RUNTIME_DIR", temp_dir.path());

        let server = WidgetServer::new(cwd).unwrap();
        let socket_path = server.socket_path().to_path_buf();
        let shutdown = server.shutdown_handle();

        let handle = tokio::spawn(async move { server.run().await });

        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(socket_path.exists());

        let _ = shutdown.send(());
        let _ = tokio::time::timeout(Duration::from_secs(1), handle).await;
        assert!(!socket_path.exists());
    }

    #[tokio::test]
    async fn test_message_framing() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cwd = temp_dir.path().to_path_buf();
        std::env::set_var("XDG_RUNTIME_DIR", temp_dir.path());

        let server = WidgetServer::new(cwd).unwrap();
        let socket_path = server.socket_path().to_path_buf();
        let shutdown = server.shutdown_handle();

        let handle = tokio::spawn(async move { server.run().await });

        tokio::time::sleep(Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&socket_path).await.unwrap();

        let msg = serde_json::json!({
            "type": "WidgetRequest",
            "widget": "branch_picker",
            "component_id": "git_branch",
            "context": {}
        });
        let msg_bytes = msg.to_string().into_bytes();
        let len_bytes = (msg_bytes.len() as u32).to_be_bytes();

        client.write_all(&len_bytes).await.unwrap();
        client.write_all(&msg_bytes).await.unwrap();
        client.flush().await.unwrap();

        drop(client);

        let _ = shutdown.send(());
        let _ = tokio::time::timeout(Duration::from_secs(1), handle).await;
    }
}
