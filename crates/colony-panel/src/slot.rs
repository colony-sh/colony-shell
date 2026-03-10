//! A single panel process: manages the child process, its I/O, and current frame.
//!
//! [`PanelSlot`] spawns a child process with piped stdin/stdout, wraps stdout
//! in a [`FrameParser`], and provides methods to read messages, send input
//! events, and manage the process lifecycle.

use std::process::ExitStatus;

use tokio::io::{AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};

use crate::layout::PanelPosition;
use crate::protocol::{
    FrameLine, FrameParser, InputEvent, PanelConfig, PanelMessage, ProtocolError,
};

/// Manages a single panel child process and its protocol I/O.
pub struct PanelSlot {
    child: Child,
    parser: FrameParser<BufReader<ChildStdout>>,
    stdin: ChildStdin,
    position: PanelPosition,
    current_frame: Vec<FrameLine>,
    config: PanelConfig,
    command_name: String,
    weight: u16,
    min_size: u16,
    max_size: u16,
    outer: bool,
}

impl PanelSlot {
    /// Spawn a panel child process with the given program and arguments.
    ///
    /// The child's stdout and stdin are piped for protocol communication;
    /// stderr is sent to `/dev/null`. Default size constraints are chosen
    /// based on the panel position.
    pub async fn spawn(
        program: &str,
        args: &[&str],
        position: PanelPosition,
    ) -> Result<Self, ProtocolError> {
        let mut child = tokio::process::Command::new(program)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()?;

        let stdout = child
            .stdout
            .take()
            .expect("stdout must be piped because we configured it above");
        let stdin = child
            .stdin
            .take()
            .expect("stdin must be piped because we configured it above");

        let parser = FrameParser::new(BufReader::new(stdout));

        let (min_size, max_size) = match position {
            PanelPosition::Left | PanelPosition::Right => (15, 40),
            PanelPosition::Top | PanelPosition::Bottom => (3, 15),
        };

        Ok(Self {
            child,
            parser,
            stdin,
            position,
            current_frame: Vec::new(),
            config: PanelConfig {
                subscriptions: Vec::new(),
            },
            command_name: program.to_owned(),
            weight: 1,
            min_size,
            max_size,
            outer: false,
        })
    }

    /// Read the next protocol message from the panel's stdout.
    ///
    /// Returns `None` when the child has closed its stdout (typically on exit).
    pub async fn read_message(&mut self) -> Result<Option<PanelMessage>, ProtocolError> {
        self.parser.next_message().await
    }

    /// Send an input event to the panel via its stdin.
    ///
    /// The event is JSON-serialized and written as a single line followed by
    /// a newline, then the stream is flushed.
    pub async fn send_event(&mut self, event: &InputEvent) -> Result<(), ProtocolError> {
        let json = serde_json::to_string(event)?;
        self.stdin.write_all(json.as_bytes()).await?;
        self.stdin.write_all(b"\n").await?;
        self.stdin.flush().await?;
        Ok(())
    }

    /// Wait for the child process to exit.
    pub async fn wait(&mut self) -> Result<ExitStatus, ProtocolError> {
        self.child.wait().await.map_err(ProtocolError::from)
    }

    /// Kill the child process, ignoring any errors.
    pub async fn kill(&mut self) {
        let _ = self.child.kill().await;
    }

    /// Check if the child process has exited without blocking.
    ///
    /// Returns `Some(status)` if the child has exited, `None` if still running.
    pub fn try_wait(&mut self) -> Option<ExitStatus> {
        self.child.try_wait().ok().flatten()
    }

    /// The current panel position.
    pub fn position(&self) -> PanelPosition {
        self.position
    }

    /// The most recently received frame lines.
    pub fn current_frame(&self) -> &[FrameLine] {
        &self.current_frame
    }

    /// Update the stored current frame.
    pub fn set_current_frame(&mut self, frame: Vec<FrameLine>) {
        self.current_frame = frame;
    }

    /// The panel's startup configuration.
    pub fn config(&self) -> &PanelConfig {
        &self.config
    }

    /// Update the stored panel configuration.
    pub fn set_config(&mut self, config: PanelConfig) {
        self.config = config;
    }

    /// The command name used to spawn this panel.
    pub fn command_name(&self) -> &str {
        &self.command_name
    }

    /// The layout weight for this panel.
    pub fn weight(&self) -> u16 {
        self.weight
    }

    /// The minimum size constraint (width for left/right, height for top/bottom).
    pub fn min_size(&self) -> u16 {
        self.min_size
    }

    /// The maximum size constraint (width for left/right, height for top/bottom).
    pub fn max_size(&self) -> u16 {
        self.max_size
    }

    /// Whether this panel renders outside the main content area.
    pub fn outer(&self) -> bool {
        self.outer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_slot_spawn_and_read_frame() {
        let script = r#"echo "---FRAME---"; echo '{"text":"hello"}'; echo "---FRAME---""#;
        let mut slot = PanelSlot::spawn("sh", &["-c", script], PanelPosition::Left)
            .await
            .unwrap();

        let msg = slot.read_message().await.unwrap().unwrap();
        match msg {
            PanelMessage::Frame(lines) => {
                assert_eq!(lines.len(), 1);
                assert_eq!(lines[0].text, "hello");
            }
            _ => panic!("expected Frame"),
        }
    }

    #[tokio::test]
    async fn test_slot_detects_exit() {
        let mut slot = PanelSlot::spawn("true", &[], PanelPosition::Left)
            .await
            .unwrap();

        let msg = slot.read_message().await.unwrap();
        assert!(msg.is_none());
    }
}
