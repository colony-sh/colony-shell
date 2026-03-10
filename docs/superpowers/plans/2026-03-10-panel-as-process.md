# Panel-as-Process Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the hardcoded sidebar component system with a composable panel architecture where panels are child processes communicating via a structured JSON protocol, configured through `.colony/init` and the `sidebar` command.

**Architecture:** A new `colony-panel` crate defines the protocol types and panel host. The panel host spawns child processes, reads their stdout for rendering frames, writes input events to their stdin, and proxies tuple space events bidirectionally. The layout engine resolves panel positions and constraints into ratatui `Rect` areas. The `sidebar` builtin command registers panels, and `.colony/init` scripts run at startup to configure the layout.

**Tech Stack:** Rust, tokio (async I/O for process pipes), serde_json (protocol serialization), ratatui (rendering), nix (process spawning)

**Spec:** `docs/superpowers/specs/2026-03-10-panel-as-process-design.md`

---

## File Structure

### New crate: `crates/colony-panel/`

| File | Responsibility |
|------|---------------|
| `Cargo.toml` | Crate manifest with serde, serde_json, tokio deps |
| `src/lib.rs` | Public API re-exports |
| `src/protocol.rs` | Protocol types: `FrameLine`, `InputEvent`, `PanelConfig`; frame parser; event serializer |
| `src/host.rs` | `PanelHost`: spawns processes, manages I/O, lifecycle |
| `src/slot.rs` | `PanelSlot`: single panel process handle, buffered I/O, current frame |
| `src/layout.rs` | `LayoutEngine`: resolves positions + constraints to `Rect` areas |

### New binary: `crates/colony-filetree-bin/`

| File | Responsibility |
|------|---------------|
| `Cargo.toml` | Standalone binary crate |
| `src/main.rs` | Entry point, event loop |
| `src/tree.rs` | File tree state, expansion, git status |
| `src/render.rs` | Emit protocol frames to stdout |

### Modified files

| File | Changes |
|------|---------|
| `Cargo.toml` (workspace) | Add `colony-panel` and `colony-filetree-bin` to workspace members |
| `crates/colony-exec/src/builtins.rs` | Register `sidebar` builtin, dispatch to new module |
| `crates/colony-exec/src/sidebar.rs` | New: `sidebar` builtin implementation |
| `crates/colony-exec/Cargo.toml` | Add `colony-panel` dependency |
| `crates/colony-shell/src/repl_tui.rs` | Replace `SidebarManager` with `PanelHost`, new rendering/input pipeline |
| `crates/colony-shell/src/init.rs` | New: `.colony/init` file discovery and execution |
| `crates/colony-shell/src/shell/orchestrator.rs` | Call init loading at startup |
| `crates/colony-shell/Cargo.toml` | Add `colony-panel` dependency |

---

## Chunk 1: Panel Protocol Crate

### Task 1: Create colony-panel crate skeleton

**Files:**
- Create: `crates/colony-panel/Cargo.toml`
- Create: `crates/colony-panel/src/lib.rs`
- Modify: `Cargo.toml` (workspace root)

- [ ] **Step 1: Create Cargo.toml**

```toml
[package]
name = "colony-panel"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
tokio = { workspace = true, features = ["process", "io-util", "sync", "macros"] }
ratatui = { workspace = true }
```

- [ ] **Step 2: Create lib.rs with module declarations**

```rust
//! Panel-as-process protocol and host for colony shell.
//!
//! Panels are child processes that render via a structured JSON protocol
//! on stdout and receive input events on stdin. The panel host manages
//! their lifecycle and bridges them to the tuple space.

pub mod protocol;
pub mod slot;
pub mod host;
pub mod layout;

pub use protocol::{FrameLine, InputEvent, PanelConfig, StyleSpec};
pub use slot::PanelSlot;
pub use host::PanelHost;
pub use layout::{LayoutEngine, PanelPosition};
```

- [ ] **Step 3: Add to workspace members**

In root `Cargo.toml`, add `"crates/colony-panel"` to the `[workspace] members` array.

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p colony-panel`
Expected: Warning about empty modules, no errors.

- [ ] **Step 5: Commit**

```bash
git add crates/colony-panel/ Cargo.toml
git commit -m "feat(panel): create colony-panel crate skeleton"
```

---

### Task 2: Protocol types

**Files:**
- Create: `crates/colony-panel/src/protocol.rs`

- [ ] **Step 1: Write tests for FrameLine deserialization**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_frame_line() {
        let json = r#"{"text":"hello"}"#;
        let line: FrameLine = serde_json::from_str(json).unwrap();
        assert_eq!(line.text, "hello");
        assert!(line.icon.is_none());
        assert!(line.style.is_none());
        assert_eq!(line.indent, 0);
        assert!(line.action.is_none());
        assert!(!line.expandable);
        assert!(!line.expanded);
    }

    #[test]
    fn test_parse_full_frame_line() {
        let json = r#"{"text":"src","icon":"","style":"bold,blue","indent":0,"expandable":true,"expanded":true,"action":"toggle:/path/src"}"#;
        let line: FrameLine = serde_json::from_str(json).unwrap();
        assert_eq!(line.text, "src");
        assert_eq!(line.icon.as_deref(), Some(""));
        assert_eq!(line.style.as_deref(), Some("bold,blue"));
        assert!(line.expandable);
        assert!(line.expanded);
        assert_eq!(line.action.as_deref(), Some("toggle:/path/src"));
    }

    #[test]
    fn test_serialize_input_event_key() {
        let event = InputEvent::Key {
            code: "j".to_string(),
            modifiers: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"event\":\"key\""));
        assert!(json.contains("\"code\":\"j\""));
    }

    #[test]
    fn test_serialize_input_event_resize() {
        let event = InputEvent::Resize { width: 25, height: 40 };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"event\":\"resize\""));
    }

    #[test]
    fn test_parse_panel_config() {
        let json = r#"{"subscriptions":[["filetree_select","*"]]}"#;
        let config: PanelConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.subscriptions.len(), 1);
        assert_eq!(config.subscriptions[0], vec!["filetree_select", "*"]);
    }

    #[test]
    fn test_parse_tuple_emit() {
        let json = r#"{"tuple":["filetree_select","/path/to/file"]}"#;
        let emit: TupleEmit = serde_json::from_str(json).unwrap();
        assert_eq!(emit.tuple, vec!["filetree_select", "/path/to/file"]);
    }

    #[test]
    fn test_parse_style_spec() {
        let spec = StyleSpec::parse("bold,blue,italic");
        assert!(spec.bold);
        assert!(spec.italic);
        assert!(!spec.dim);
        assert_eq!(spec.fg, Some(StyleColor::Named("blue".to_string())));
    }

    #[test]
    fn test_parse_style_spec_hex() {
        let spec = StyleSpec::parse("#ff0000,dim");
        assert!(spec.dim);
        assert_eq!(spec.fg, Some(StyleColor::Hex(255, 0, 0)));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p colony-panel`
Expected: FAIL with compilation errors (types not defined).

- [ ] **Step 3: Implement protocol types**

```rust
//! Panel protocol types: frames, input events, configuration.
//!
//! ## Protocol Overview
//!
//! **stdout (panel -> host):**
//! - `---CONFIG---` followed by a JSON `PanelConfig` line (optional, sent once at startup)
//! - `---FRAME---` delimiters with JSON `FrameLine` objects between them
//! - `---EMIT---` followed by a JSON `TupleEmit` line (tuple space publication)
//!
//! **stdin (host -> panel):**
//! - JSON `InputEvent` objects, one per line

use serde::{Deserialize, Serialize};

/// Sentinel lines in the stdout protocol.
pub const FRAME_DELIMITER: &str = "---FRAME---";
pub const CONFIG_DELIMITER: &str = "---CONFIG---";
pub const EMIT_DELIMITER: &str = "---EMIT---";

/// A single line in a panel frame.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameLine {
    /// Display text (required).
    pub text: String,

    /// Icon prefix.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,

    /// Comma-separated style: bold, dim, italic, color names, #rrggbb.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,

    /// Indentation level (default 0).
    #[serde(default)]
    pub indent: usize,

    /// Action string for Enter/click.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,

    /// Shows expand/collapse indicator.
    #[serde(default)]
    pub expandable: bool,

    /// Current expand state.
    #[serde(default)]
    pub expanded: bool,
}

/// An input event sent from host to panel via stdin.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum InputEvent {
    Key {
        code: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        modifiers: Option<String>,
    },
    Resize {
        width: u16,
        height: u16,
    },
    Focus {
        focused: bool,
    },
    /// Tuple space event forwarded to the panel.
    Tuple {
        data: Vec<String>,
    },
    Shutdown,
}

/// Startup configuration sent by the panel.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PanelConfig {
    /// Tuple space patterns to subscribe to. Each pattern is a list of
    /// strings where "*" is a wildcard.
    #[serde(default)]
    pub subscriptions: Vec<Vec<String>>,
}

/// A tuple space publication from the panel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TupleEmit {
    pub tuple: Vec<String>,
}

/// Parsed style specification.
#[derive(Debug, Clone, Default)]
pub struct StyleSpec {
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub fg: Option<StyleColor>,
}

/// A color in a style spec.
#[derive(Debug, Clone, PartialEq)]
pub enum StyleColor {
    Named(String),
    Hex(u8, u8, u8),
}

impl StyleSpec {
    /// Parse a comma-separated style string like "bold,blue,italic" or "#ff0000,dim".
    pub fn parse(s: &str) -> Self {
        let mut spec = StyleSpec::default();
        for part in s.split(',') {
            let part = part.trim();
            match part {
                "bold" => spec.bold = true,
                "dim" => spec.dim = true,
                "italic" => spec.italic = true,
                s if s.starts_with('#') && s.len() == 7 => {
                    if let (Ok(r), Ok(g), Ok(b)) = (
                        u8::from_str_radix(&s[1..3], 16),
                        u8::from_str_radix(&s[3..5], 16),
                        u8::from_str_radix(&s[5..7], 16),
                    ) {
                        spec.fg = Some(StyleColor::Hex(r, g, b));
                    }
                }
                name if !name.is_empty() => {
                    spec.fg = Some(StyleColor::Named(name.to_string()));
                }
                _ => {}
            }
        }
        spec
    }

    /// Convert to a ratatui Style.
    pub fn to_ratatui_style(&self) -> ratatui::style::Style {
        use ratatui::style::{Color, Modifier, Style};

        let mut style = Style::default();
        if self.bold {
            style = style.add_modifier(Modifier::BOLD);
        }
        if self.dim {
            style = style.add_modifier(Modifier::DIM);
        }
        if self.italic {
            style = style.add_modifier(Modifier::ITALIC);
        }
        if let Some(ref color) = self.fg {
            style = style.fg(match color {
                StyleColor::Named(name) => match name.as_str() {
                    "black" => Color::Black,
                    "red" => Color::Red,
                    "green" => Color::Green,
                    "yellow" => Color::Yellow,
                    "blue" => Color::Blue,
                    "magenta" => Color::Magenta,
                    "cyan" => Color::Cyan,
                    "white" => Color::White,
                    "gray" | "grey" => Color::DarkGray,
                    _ => Color::White,
                },
                StyleColor::Hex(r, g, b) => Color::Rgb(*r, *g, *b),
            });
        }
        style
    }
}

impl FrameLine {
    /// Parse the style field into a StyleSpec.
    pub fn style_spec(&self) -> StyleSpec {
        self.style.as_deref().map(StyleSpec::parse).unwrap_or_default()
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p colony-panel`
Expected: All tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/colony-panel/src/protocol.rs
git commit -m "feat(panel): protocol types for panel-as-process communication"
```

---

### Task 3: Frame parser (reads panel stdout)

**Files:**
- Modify: `crates/colony-panel/src/protocol.rs` (add parser)

- [ ] **Step 1: Write tests for the frame parser**

```rust
    #[tokio::test]
    async fn test_frame_parser_single_frame() {
        let input = "---FRAME---\n{\"text\":\"hello\"}\n{\"text\":\"world\"}\n---FRAME---\n";
        let mut reader = tokio::io::BufReader::new(input.as_bytes());
        let mut parser = FrameParser::new();

        let msg = parser.read_next(&mut reader).await.unwrap().unwrap();
        match msg {
            PanelMessage::Frame(lines) => {
                assert_eq!(lines.len(), 2);
                assert_eq!(lines[0].text, "hello");
                assert_eq!(lines[1].text, "world");
            }
            _ => panic!("expected Frame"),
        }
    }

    #[tokio::test]
    async fn test_frame_parser_config() {
        let input = "---CONFIG---\n{\"subscriptions\":[[\"dir_change\",\"*\"]]}\n---FRAME---\n{\"text\":\"content\"}\n---FRAME---\n";
        let mut reader = tokio::io::BufReader::new(input.as_bytes());
        let mut parser = FrameParser::new();

        let msg = parser.read_next(&mut reader).await.unwrap().unwrap();
        match msg {
            PanelMessage::Config(config) => {
                assert_eq!(config.subscriptions.len(), 1);
            }
            _ => panic!("expected Config"),
        }

        let msg = parser.read_next(&mut reader).await.unwrap().unwrap();
        assert!(matches!(msg, PanelMessage::Frame(_)));
    }

    #[tokio::test]
    async fn test_frame_parser_emit() {
        let input = "---EMIT---\n{\"tuple\":[\"filetree_select\",\"/path\"]}\n";
        let mut reader = tokio::io::BufReader::new(input.as_bytes());
        let mut parser = FrameParser::new();

        let msg = parser.read_next(&mut reader).await.unwrap().unwrap();
        match msg {
            PanelMessage::Emit(emit) => {
                assert_eq!(emit.tuple, vec!["filetree_select", "/path"]);
            }
            _ => panic!("expected Emit"),
        }
    }

    #[tokio::test]
    async fn test_frame_parser_eof() {
        let input = "";
        let mut reader = tokio::io::BufReader::new(input.as_bytes());
        let mut parser = FrameParser::new();

        let msg = parser.read_next(&mut reader).await.unwrap();
        assert!(msg.is_none()); // EOF
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p colony-panel`
Expected: FAIL (FrameParser not defined).

- [ ] **Step 3: Implement FrameParser**

Add to `protocol.rs`:

```rust
use tokio::io::{AsyncBufRead, AsyncBufReadExt};

/// Messages that can be read from a panel's stdout.
#[derive(Debug)]
pub enum PanelMessage {
    /// Startup configuration.
    Config(PanelConfig),
    /// A complete rendered frame.
    Frame(Vec<FrameLine>),
    /// A tuple space emission.
    Emit(TupleEmit),
}

/// Reads and parses panel stdout into structured messages.
pub struct FrameParser {
    /// Lines accumulated for the current frame.
    frame_buf: Vec<FrameLine>,
    /// Whether we are inside a frame (between ---FRAME--- delimiters).
    in_frame: bool,
}

impl FrameParser {
    pub fn new() -> Self {
        Self {
            frame_buf: Vec::new(),
            in_frame: false,
        }
    }

    /// Read the next complete message from the panel's stdout.
    /// Returns `None` on EOF.
    pub async fn read_next<R: AsyncBufRead + Unpin>(
        &mut self,
        reader: &mut R,
    ) -> Result<Option<PanelMessage>, ProtocolError> {
        let mut line = String::new();
        loop {
            line.clear();
            let n = reader
                .read_line(&mut line)
                .await
                .map_err(|e| ProtocolError::Io(e.to_string()))?;
            if n == 0 {
                return Ok(None); // EOF
            }

            let trimmed = line.trim();

            match trimmed {
                FRAME_DELIMITER => {
                    if self.in_frame {
                        // End of frame
                        self.in_frame = false;
                        let frame = std::mem::take(&mut self.frame_buf);
                        return Ok(Some(PanelMessage::Frame(frame)));
                    }
                    // Start of frame
                    self.in_frame = true;
                    self.frame_buf.clear();
                }
                CONFIG_DELIMITER => {
                    // Next line is the config JSON
                    line.clear();
                    let n = reader
                        .read_line(&mut line)
                        .await
                        .map_err(|e| ProtocolError::Io(e.to_string()))?;
                    if n == 0 {
                        return Err(ProtocolError::UnexpectedEof);
                    }
                    let config: PanelConfig = serde_json::from_str(line.trim())
                        .map_err(|e| ProtocolError::Json(e.to_string()))?;
                    return Ok(Some(PanelMessage::Config(config)));
                }
                EMIT_DELIMITER => {
                    // Next line is the tuple JSON
                    line.clear();
                    let n = reader
                        .read_line(&mut line)
                        .await
                        .map_err(|e| ProtocolError::Io(e.to_string()))?;
                    if n == 0 {
                        return Err(ProtocolError::UnexpectedEof);
                    }
                    let emit: TupleEmit = serde_json::from_str(line.trim())
                        .map_err(|e| ProtocolError::Json(e.to_string()))?;
                    return Ok(Some(PanelMessage::Emit(emit)));
                }
                _ if self.in_frame => {
                    // Parse as a frame line
                    if !trimmed.is_empty() {
                        let frame_line: FrameLine = serde_json::from_str(trimmed)
                            .map_err(|e| ProtocolError::Json(e.to_string()))?;
                        self.frame_buf.push(frame_line);
                    }
                }
                _ => {
                    // Ignore lines outside frames (e.g. debug output)
                }
            }
        }
    }
}

/// Protocol errors.
#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("I/O error: {0}")]
    Io(String),
    #[error("JSON parse error: {0}")]
    Json(String),
    #[error("unexpected EOF in protocol stream")]
    UnexpectedEof,
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p colony-panel`
Expected: All tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/colony-panel/src/protocol.rs
git commit -m "feat(panel): frame parser for reading panel stdout"
```

---

### Task 4: Panel slot (single process handle)

**Files:**
- Create: `crates/colony-panel/src/slot.rs`

- [ ] **Step 1: Write tests for PanelSlot**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_slot_spawn_and_read_frame() {
        // Use `echo` to emit a simple frame
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
    async fn test_slot_send_event() {
        // Use `cat` as a panel that echoes stdin to stdout
        let mut slot = PanelSlot::spawn("cat", &[], PanelPosition::Left)
            .await
            .unwrap();

        let event = InputEvent::Key {
            code: "j".to_string(),
            modifiers: None,
        };
        slot.send_event(&event).await.unwrap();
        slot.send_event(&InputEvent::Shutdown).await.unwrap();

        // cat should exit after stdin closes
        let status = slot.wait().await;
        assert!(status.is_ok());
    }

    #[tokio::test]
    async fn test_slot_detects_exit() {
        let mut slot = PanelSlot::spawn("true", &[], PanelPosition::Left)
            .await
            .unwrap();

        // Process exits immediately; read should return None
        let msg = slot.read_message().await.unwrap();
        assert!(msg.is_none());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p colony-panel`
Expected: FAIL (PanelSlot not defined).

- [ ] **Step 3: Implement PanelSlot**

```rust
//! A single panel process: manages the child process, its I/O, and current frame.

use std::process::Stdio;
use tokio::io::{AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};

use crate::layout::PanelPosition;
use crate::protocol::{
    FrameLine, FrameParser, InputEvent, PanelConfig, PanelMessage, ProtocolError,
};

/// A running panel process with its I/O handles.
pub struct PanelSlot {
    /// The child process.
    child: Child,
    /// Frame parser for stdout.
    parser: FrameParser,
    /// Buffered stdout reader.
    stdout: BufReader<tokio::process::ChildStdout>,
    /// Stdin writer.
    stdin: tokio::process::ChildStdin,
    /// Panel position in the layout.
    pub position: PanelPosition,
    /// Current rendered frame.
    pub current_frame: Vec<FrameLine>,
    /// Panel configuration (from startup handshake).
    pub config: PanelConfig,
    /// The command that was spawned (for display/debugging).
    pub command_name: String,
    /// Panel-specific constraints.
    pub weight: u16,
    pub min_size: u16,
    pub max_size: u16,
    /// Whether this is an outer panel (top/bottom only).
    pub outer: bool,
}

impl PanelSlot {
    /// Spawn a new panel process.
    pub async fn spawn(
        program: &str,
        args: &[&str],
        position: PanelPosition,
    ) -> Result<Self, ProtocolError> {
        let mut cmd = Command::new(program);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        let mut child = cmd
            .spawn()
            .map_err(|e| ProtocolError::Io(format!("failed to spawn {}: {}", program, e)))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| ProtocolError::Io("no stdout".to_string()))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| ProtocolError::Io("no stdin".to_string()))?;

        let (min_size, max_size) = match position {
            PanelPosition::Left | PanelPosition::Right => (15, 40),
            PanelPosition::Top | PanelPosition::Bottom => (3, 15),
        };

        Ok(Self {
            child,
            parser: FrameParser::new(),
            stdout: BufReader::new(stdout),
            stdin,
            position,
            current_frame: Vec::new(),
            config: PanelConfig::default(),
            command_name: program.to_string(),
            weight: 1,
            min_size,
            max_size,
            outer: false,
        })
    }

    /// Read the next message from the panel's stdout.
    pub async fn read_message(&mut self) -> Result<Option<PanelMessage>, ProtocolError> {
        self.parser.read_next(&mut self.stdout).await
    }

    /// Send an input event to the panel via stdin.
    pub async fn send_event(&mut self, event: &InputEvent) -> Result<(), ProtocolError> {
        let json = serde_json::to_string(event)
            .map_err(|e| ProtocolError::Json(e.to_string()))?;
        self.stdin
            .write_all(format!("{}\n", json).as_bytes())
            .await
            .map_err(|e| ProtocolError::Io(e.to_string()))?;
        self.stdin
            .flush()
            .await
            .map_err(|e| ProtocolError::Io(e.to_string()))?;
        Ok(())
    }

    /// Wait for the child process to exit.
    pub async fn wait(&mut self) -> Result<std::process::ExitStatus, ProtocolError> {
        self.child
            .wait()
            .await
            .map_err(|e| ProtocolError::Io(e.to_string()))
    }

    /// Kill the child process.
    pub async fn kill(&mut self) {
        let _ = self.child.kill().await;
    }

    /// Check if the child process has exited.
    pub fn try_wait(&mut self) -> Option<std::process::ExitStatus> {
        self.child.try_wait().ok().flatten()
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p colony-panel`
Expected: All tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/colony-panel/src/slot.rs
git commit -m "feat(panel): PanelSlot for spawning and managing panel processes"
```

---

### Task 5: Layout engine

**Files:**
- Create: `crates/colony-panel/src/layout.rs`

- [ ] **Step 1: Write tests for layout resolution**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;

    #[test]
    fn test_no_panels() {
        let engine = LayoutEngine::new();
        let result = engine.resolve(Rect::new(0, 0, 80, 24));
        assert_eq!(result.main, Rect::new(0, 0, 80, 24));
        assert!(result.panels.is_empty());
    }

    #[test]
    fn test_single_left_panel() {
        let mut engine = LayoutEngine::new();
        engine.add_panel(PanelId(0), PanelPosition::Left, PanelConstraints {
            weight: 1, min_size: 15, max_size: 40, outer: false, content_size: 25,
        });
        let result = engine.resolve(Rect::new(0, 0, 80, 24));
        assert_eq!(result.panels[&PanelId(0)].width, 25);
        assert!(result.main.width >= 10);
    }

    #[test]
    fn test_two_left_panels_horizontal() {
        let mut engine = LayoutEngine::new();
        engine.add_panel(PanelId(0), PanelPosition::Left, PanelConstraints {
            weight: 2, min_size: 15, max_size: 40, outer: false, content_size: 25,
        });
        engine.add_panel(PanelId(1), PanelPosition::Left, PanelConstraints {
            weight: 1, min_size: 15, max_size: 30, outer: false, content_size: 20,
        });
        let result = engine.resolve(Rect::new(0, 0, 100, 24));
        // Both panels should be present, stacked horizontally on the left
        assert!(result.panels.contains_key(&PanelId(0)));
        assert!(result.panels.contains_key(&PanelId(1)));
        assert_eq!(result.panels[&PanelId(0)].y, 0);
        assert_eq!(result.panels[&PanelId(1)].y, 0);
        assert!(result.panels[&PanelId(0)].x < result.panels[&PanelId(1)].x);
    }

    #[test]
    fn test_outer_top_full_width() {
        let mut engine = LayoutEngine::new();
        engine.add_panel(PanelId(0), PanelPosition::Left, PanelConstraints {
            weight: 1, min_size: 15, max_size: 25, outer: false, content_size: 20,
        });
        engine.add_panel(PanelId(1), PanelPosition::Top, PanelConstraints {
            weight: 1, min_size: 3, max_size: 10, outer: true, content_size: 5,
        });
        let result = engine.resolve(Rect::new(0, 0, 80, 24));
        // Outer top should span full width
        assert_eq!(result.panels[&PanelId(1)].width, 80);
        assert_eq!(result.panels[&PanelId(1)].y, 0);
    }

    #[test]
    fn test_inner_top_between_panels() {
        let mut engine = LayoutEngine::new();
        engine.add_panel(PanelId(0), PanelPosition::Left, PanelConstraints {
            weight: 1, min_size: 15, max_size: 20, outer: false, content_size: 20,
        });
        engine.add_panel(PanelId(1), PanelPosition::Top, PanelConstraints {
            weight: 1, min_size: 3, max_size: 10, outer: false, content_size: 5,
        });
        let result = engine.resolve(Rect::new(0, 0, 80, 24));
        // Inner top should NOT span the left panel area
        assert!(result.panels[&PanelId(1)].width < 80);
        assert!(result.panels[&PanelId(1)].x >= result.panels[&PanelId(0)].width);
    }

    #[test]
    fn test_main_enforced_minimum() {
        let mut engine = LayoutEngine::new();
        // Try to squeeze main with big panels
        engine.add_panel(PanelId(0), PanelPosition::Left, PanelConstraints {
            weight: 1, min_size: 30, max_size: 40, outer: false, content_size: 35,
        });
        engine.add_panel(PanelId(1), PanelPosition::Right, PanelConstraints {
            weight: 1, min_size: 30, max_size: 40, outer: false, content_size: 35,
        });
        let result = engine.resolve(Rect::new(0, 0, 60, 24));
        // Main must be at least 10 wide
        assert!(result.main.width >= 10);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p colony-panel`
Expected: FAIL (LayoutEngine not defined).

- [ ] **Step 3: Implement LayoutEngine**

```rust
//! Layout engine: resolves panel positions and constraints to Rect areas.

use std::collections::HashMap;
use ratatui::layout::Rect;

/// Unique identifier for a panel slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PanelId(pub usize);

/// Where a panel lives in the layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelPosition {
    Left,
    Right,
    Top,
    Bottom,
}

/// Size constraints for a panel.
#[derive(Debug, Clone)]
pub struct PanelConstraints {
    pub weight: u16,
    pub min_size: u16,
    pub max_size: u16,
    pub outer: bool,
    /// Current content size (width for left/right, height for top/bottom).
    pub content_size: u16,
}

/// The resolved layout: a Rect for each panel and the main area.
#[derive(Debug)]
pub struct ResolvedLayout {
    pub main: Rect,
    pub panels: HashMap<PanelId, Rect>,
}

/// Manages panel registrations and resolves their layout.
pub struct LayoutEngine {
    panels: Vec<(PanelId, PanelPosition, PanelConstraints)>,
}

const MAIN_MIN_WIDTH: u16 = 10;
const MAIN_MIN_HEIGHT: u16 = 5;

impl LayoutEngine {
    pub fn new() -> Self {
        Self { panels: Vec::new() }
    }

    pub fn add_panel(&mut self, id: PanelId, position: PanelPosition, constraints: PanelConstraints) {
        self.panels.push((id, position, constraints));
    }

    pub fn remove_panel(&mut self, id: PanelId) {
        self.panels.retain(|(pid, _, _)| *pid != id);
    }

    pub fn clear(&mut self) {
        self.panels.clear();
    }

    pub fn clear_position(&mut self, position: PanelPosition) {
        self.panels.retain(|(_, pos, _)| *pos != position);
    }

    /// Resolve all panel positions into concrete Rect areas.
    pub fn resolve(&self, area: Rect) -> ResolvedLayout {
        let mut result = HashMap::new();
        let mut remaining = area;

        // 1. Outer top panels (full width, stacked vertically)
        let outer_tops: Vec<_> = self.panels.iter()
            .filter(|(_, pos, c)| *pos == PanelPosition::Top && c.outer)
            .collect();
        for (id, _, constraints) in &outer_tops {
            let h = constraints.content_size.clamp(constraints.min_size, constraints.max_size)
                .min(remaining.height.saturating_sub(MAIN_MIN_HEIGHT));
            if h == 0 { continue; }
            result.insert(*id, Rect::new(remaining.x, remaining.y, remaining.width, h));
            remaining = Rect::new(remaining.x, remaining.y + h, remaining.width, remaining.height - h);
        }

        // 2. Outer bottom panels (full width, stacked vertically from bottom)
        let outer_bottoms: Vec<_> = self.panels.iter()
            .filter(|(_, pos, c)| *pos == PanelPosition::Bottom && c.outer)
            .collect();
        for (id, _, constraints) in &outer_bottoms {
            let h = constraints.content_size.clamp(constraints.min_size, constraints.max_size)
                .min(remaining.height.saturating_sub(MAIN_MIN_HEIGHT));
            if h == 0 { continue; }
            let y = remaining.y + remaining.height - h;
            result.insert(*id, Rect::new(remaining.x, y, remaining.width, h));
            remaining = Rect::new(remaining.x, remaining.y, remaining.width, remaining.height - h);
        }

        // 3. Left panels (stacked horizontally)
        let lefts: Vec<_> = self.panels.iter()
            .filter(|(_, pos, _)| *pos == PanelPosition::Left)
            .collect();
        for (id, _, constraints) in &lefts {
            let w = constraints.content_size.clamp(constraints.min_size, constraints.max_size)
                .min(remaining.width.saturating_sub(MAIN_MIN_WIDTH));
            if w == 0 { continue; }
            result.insert(*id, Rect::new(remaining.x, remaining.y, w, remaining.height));
            remaining = Rect::new(remaining.x + w, remaining.y, remaining.width - w, remaining.height);
        }

        // 4. Right panels (stacked horizontally from right)
        let rights: Vec<_> = self.panels.iter()
            .filter(|(_, pos, _)| *pos == PanelPosition::Right)
            .collect();
        for (id, _, constraints) in &rights {
            let w = constraints.content_size.clamp(constraints.min_size, constraints.max_size)
                .min(remaining.width.saturating_sub(MAIN_MIN_WIDTH));
            if w == 0 { continue; }
            let x = remaining.x + remaining.width - w;
            result.insert(*id, Rect::new(x, remaining.y, w, remaining.height));
            remaining = Rect::new(remaining.x, remaining.y, remaining.width - w, remaining.height);
        }

        // 5. Inner top panels (within main column)
        let inner_tops: Vec<_> = self.panels.iter()
            .filter(|(_, pos, c)| *pos == PanelPosition::Top && !c.outer)
            .collect();
        for (id, _, constraints) in &inner_tops {
            let h = constraints.content_size.clamp(constraints.min_size, constraints.max_size)
                .min(remaining.height.saturating_sub(MAIN_MIN_HEIGHT));
            if h == 0 { continue; }
            result.insert(*id, Rect::new(remaining.x, remaining.y, remaining.width, h));
            remaining = Rect::new(remaining.x, remaining.y + h, remaining.width, remaining.height - h);
        }

        // 6. Inner bottom panels (within main column)
        let inner_bottoms: Vec<_> = self.panels.iter()
            .filter(|(_, pos, c)| *pos == PanelPosition::Bottom && !c.outer)
            .collect();
        for (id, _, constraints) in &inner_bottoms {
            let h = constraints.content_size.clamp(constraints.min_size, constraints.max_size)
                .min(remaining.height.saturating_sub(MAIN_MIN_HEIGHT));
            if h == 0 { continue; }
            let y = remaining.y + remaining.height - h;
            result.insert(*id, Rect::new(remaining.x, y, remaining.width, h));
            remaining = Rect::new(remaining.x, remaining.y, remaining.width, remaining.height - h);
        }

        // 7. Main gets everything left
        ResolvedLayout {
            main: remaining,
            panels: result,
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p colony-panel`
Expected: All tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/colony-panel/src/layout.rs
git commit -m "feat(panel): layout engine for resolving panel positions to Rects"
```

---

### Task 6: Panel host (manages all panels)

**Files:**
- Create: `crates/colony-panel/src/host.rs`

- [ ] **Step 1: Implement PanelHost**

```rust
//! Panel host: manages all panel processes and coordinates their lifecycle.

use std::collections::HashMap;
use tokio::sync::mpsc;

use crate::layout::{LayoutEngine, PanelConstraints, PanelId, PanelPosition, ResolvedLayout};
use crate::protocol::{FrameLine, InputEvent, PanelMessage, ProtocolError};
use crate::slot::PanelSlot;

/// Identifies the focused panel or main REPL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Main,
    Panel(PanelId),
}

/// The panel host manages all panel processes.
pub struct PanelHost {
    /// Active panel slots indexed by ID.
    slots: HashMap<PanelId, PanelSlot>,
    /// Layout engine for position resolution.
    layout: LayoutEngine,
    /// Next panel ID to assign.
    next_id: usize,
    /// Current focus.
    pub focus: Focus,
}

impl PanelHost {
    pub fn new() -> Self {
        Self {
            slots: HashMap::new(),
            layout: LayoutEngine::new(),
            next_id: 0,
            focus: Focus::Main,
        }
    }

    /// Spawn a panel process and add it to the layout.
    pub async fn add_panel(
        &mut self,
        program: &str,
        args: &[&str],
        position: PanelPosition,
        weight: u16,
        min_size: u16,
        max_size: u16,
        outer: bool,
    ) -> Result<PanelId, ProtocolError> {
        let id = PanelId(self.next_id);
        self.next_id += 1;

        let mut slot = PanelSlot::spawn(program, args, position).await?;
        slot.weight = weight;
        slot.min_size = min_size;
        slot.max_size = max_size;
        slot.outer = outer;

        self.layout.add_panel(id, position, PanelConstraints {
            weight,
            min_size,
            max_size,
            outer,
            content_size: min_size, // Start at min, updated on first frame
        });

        self.slots.insert(id, slot);
        Ok(id)
    }

    /// Remove a panel, killing its process.
    pub async fn remove_panel(&mut self, id: PanelId) {
        if let Some(mut slot) = self.slots.remove(&id) {
            let _ = slot.send_event(&InputEvent::Shutdown).await;
            slot.kill().await;
        }
        self.layout.remove_panel(id);
        if self.focus == Focus::Panel(id) {
            self.focus = Focus::Main;
        }
    }

    /// Remove all panels.
    pub async fn reset(&mut self) {
        let ids: Vec<PanelId> = self.slots.keys().copied().collect();
        for id in ids {
            self.remove_panel(id).await;
        }
    }

    /// Remove all panels at a given position.
    pub async fn reset_position(&mut self, position: PanelPosition) {
        let ids: Vec<PanelId> = self.slots.iter()
            .filter(|(_, slot)| slot.position == position)
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            self.remove_panel(id).await;
        }
        self.layout.clear_position(position);
    }

    /// Poll all panels for new messages (non-blocking).
    /// Returns list of panels that produced new frames or emitted tuples.
    pub async fn poll_panels(&mut self) -> Vec<(PanelId, PanelMessage)> {
        let mut messages = Vec::new();
        let mut dead = Vec::new();

        for (id, slot) in &mut self.slots {
            // Check if process has exited
            if slot.try_wait().is_some() {
                dead.push(*id);
                continue;
            }

            // Try to read a message with a very short timeout
            match tokio::time::timeout(
                std::time::Duration::from_millis(1),
                slot.read_message(),
            ).await {
                Ok(Ok(Some(msg))) => {
                    // Update current frame if it's a Frame message
                    if let PanelMessage::Frame(ref lines) = msg {
                        slot.current_frame = lines.clone();
                        // Update content size in layout
                        let content_size = match slot.position {
                            PanelPosition::Left | PanelPosition::Right => {
                                lines.iter().map(|l| l.text.len() + l.indent * 2 + 4).max().unwrap_or(0) as u16
                            }
                            PanelPosition::Top | PanelPosition::Bottom => {
                                lines.len() as u16
                            }
                        };
                        self.layout.remove_panel(*id);
                        self.layout.add_panel(*id, slot.position, PanelConstraints {
                            weight: slot.weight,
                            min_size: slot.min_size,
                            max_size: slot.max_size,
                            outer: slot.outer,
                            content_size,
                        });
                    }
                    if let PanelMessage::Config(ref config) = msg {
                        slot.config = config.clone();
                    }
                    messages.push((*id, msg));
                }
                Ok(Ok(None)) => {
                    // EOF
                    dead.push(*id);
                }
                Ok(Err(_)) => {
                    dead.push(*id);
                }
                Err(_) => {
                    // Timeout - no message ready, that's fine
                }
            }
        }

        // Clean up dead panels
        for id in dead {
            self.slots.remove(&id);
            self.layout.remove_panel(id);
            if self.focus == Focus::Panel(id) {
                self.focus = Focus::Main;
            }
        }

        messages
    }

    /// Send an input event to the focused panel.
    pub async fn send_to_focused(&mut self, event: &InputEvent) -> Result<(), ProtocolError> {
        let Focus::Panel(id) = self.focus else {
            return Ok(());
        };
        let Some(slot) = self.slots.get_mut(&id) else {
            return Ok(());
        };
        slot.send_event(event).await
    }

    /// Send a tuple event to all panels that subscribe to it.
    pub async fn broadcast_tuple(&mut self, tuple: &[String]) {
        for slot in self.slots.values_mut() {
            if matches_any_subscription(&slot.config.subscriptions, tuple) {
                let _ = slot.send_event(&InputEvent::Tuple {
                    data: tuple.to_vec(),
                }).await;
            }
        }
    }

    /// Resolve the current layout.
    pub fn resolve_layout(&self, area: ratatui::layout::Rect) -> ResolvedLayout {
        self.layout.resolve(area)
    }

    /// Get the current frame for a panel.
    pub fn current_frame(&self, id: PanelId) -> &[FrameLine] {
        self.slots.get(&id)
            .map(|s| s.current_frame.as_slice())
            .unwrap_or(&[])
    }

    /// Get all panel IDs.
    pub fn panel_ids(&self) -> Vec<PanelId> {
        self.slots.keys().copied().collect()
    }

    /// Check if there are any panels.
    pub fn has_panels(&self) -> bool {
        !self.slots.is_empty()
    }

    /// Cycle focus to the next panel (or back to main).
    pub fn cycle_focus(&mut self) {
        let ids = self.panel_ids();
        if ids.is_empty() {
            self.focus = Focus::Main;
            return;
        }
        self.focus = match self.focus {
            Focus::Main => Focus::Panel(ids[0]),
            Focus::Panel(current) => {
                let pos = ids.iter().position(|id| *id == current);
                match pos {
                    Some(i) if i + 1 < ids.len() => Focus::Panel(ids[i + 1]),
                    _ => Focus::Main,
                }
            }
        };
    }

    /// Send resize events to all panels with their current dimensions.
    pub async fn notify_resize(&mut self, layout: &ResolvedLayout) {
        for (id, rect) in &layout.panels {
            if let Some(slot) = self.slots.get_mut(id) {
                let _ = slot.send_event(&InputEvent::Resize {
                    width: rect.width,
                    height: rect.height,
                }).await;
            }
        }
    }

    /// Shutdown all panels gracefully.
    pub async fn shutdown(&mut self) {
        for (_, slot) in &mut self.slots {
            let _ = slot.send_event(&InputEvent::Shutdown).await;
        }
        // Give panels a moment to exit, then kill
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        for (_, slot) in &mut self.slots {
            slot.kill().await;
        }
        self.slots.clear();
        self.layout.clear();
    }
}

/// Check if a tuple matches any of the subscription patterns.
fn matches_any_subscription(subscriptions: &[Vec<String>], tuple: &[String]) -> bool {
    subscriptions.iter().any(|pattern| {
        if pattern.len() != tuple.len() {
            return false;
        }
        pattern.iter().zip(tuple.iter()).all(|(p, t)| p == "*" || p == t)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_subscription() {
        let subs = vec![vec!["filetree_select".to_string(), "*".to_string()]];
        assert!(matches_any_subscription(&subs, &["filetree_select".to_string(), "/path".to_string()]));
        assert!(!matches_any_subscription(&subs, &["other".to_string(), "/path".to_string()]));
        assert!(!matches_any_subscription(&subs, &["filetree_select".to_string()])); // wrong length
    }

    #[test]
    fn test_focus_cycle() {
        let mut host = PanelHost::new();
        assert_eq!(host.focus, Focus::Main);
        host.cycle_focus();
        assert_eq!(host.focus, Focus::Main); // no panels
    }
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test -p colony-panel`
Expected: All tests PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/colony-panel/src/host.rs
git commit -m "feat(panel): PanelHost for managing panel processes and focus"
```

---

## Chunk 2: Shell Integration

### Task 7: `sidebar` builtin command

**Files:**
- Create: `crates/colony-exec/src/sidebar.rs`
- Modify: `crates/colony-exec/src/builtins.rs`
- Modify: `crates/colony-exec/Cargo.toml`

- [ ] **Step 1: Add colony-panel dependency to colony-exec**

In `crates/colony-exec/Cargo.toml`, add:
```toml
colony-panel = { path = "../colony-panel" }
```

- [ ] **Step 2: Create sidebar.rs with the builtin**

```rust
//! The `sidebar` builtin command.
//!
//! Usage:
//!   sidebar <position> [options] <command>   # add a panel
//!   sidebar reset                             # clear all panels
//!   sidebar reset <position>                  # clear panels at position

use colony_panel::PanelPosition;

/// Result of parsing a sidebar command.
#[derive(Debug)]
pub enum SidebarCommand {
    /// Add a panel at the given position.
    Add {
        position: PanelPosition,
        command: String,
        args: Vec<String>,
        weight: u16,
        min_size: Option<u16>,
        max_size: Option<u16>,
        outer: bool,
    },
    /// Reset all panels.
    Reset,
    /// Reset panels at a specific position.
    ResetPosition(PanelPosition),
}

/// Parse sidebar command arguments.
pub fn parse_sidebar_args(args: &[String]) -> Result<SidebarCommand, String> {
    if args.is_empty() {
        return Err("usage: sidebar <position> [options] <command> | sidebar reset [position]".to_string());
    }

    if args[0] == "reset" {
        return if args.len() > 1 {
            let pos = parse_position(&args[1])?;
            Ok(SidebarCommand::ResetPosition(pos))
        } else {
            Ok(SidebarCommand::Reset)
        };
    }

    let position = parse_position(&args[0])?;
    let mut weight = 1u16;
    let mut min_size = None;
    let mut max_size = None;
    let mut outer = false;
    let mut command_start = 1;

    // Parse options
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--weight" => {
                i += 1;
                weight = args.get(i)
                    .ok_or("--weight requires a value")?
                    .parse()
                    .map_err(|_| "--weight must be a number")?;
            }
            "--min-width" | "--min-height" => {
                i += 1;
                min_size = Some(args.get(i)
                    .ok_or("--min-width/--min-height requires a value")?
                    .parse()
                    .map_err(|_| "size must be a number")?);
            }
            "--max-width" | "--max-height" => {
                i += 1;
                max_size = Some(args.get(i)
                    .ok_or("--max-width/--max-height requires a value")?
                    .parse()
                    .map_err(|_| "size must be a number")?);
            }
            "--outer" => {
                if position != PanelPosition::Top && position != PanelPosition::Bottom {
                    return Err("--outer only applies to top/bottom panels".to_string());
                }
                outer = true;
            }
            _ => {
                command_start = i;
                break;
            }
        }
        i += 1;
        command_start = i;
    }

    if command_start >= args.len() {
        return Err("sidebar requires a command".to_string());
    }

    let command = args[command_start].clone();
    let cmd_args = args[command_start + 1..].to_vec();

    Ok(SidebarCommand::Add {
        position,
        command,
        args: cmd_args,
        weight,
        min_size,
        max_size,
        outer,
    })
}

fn parse_position(s: &str) -> Result<PanelPosition, String> {
    match s {
        "left" => Ok(PanelPosition::Left),
        "right" => Ok(PanelPosition::Right),
        "top" => Ok(PanelPosition::Top),
        "bottom" => Ok(PanelPosition::Bottom),
        _ => Err(format!("unknown position '{}': expected left, right, top, or bottom", s)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple() {
        let args: Vec<String> = vec!["left", "filetree"].iter().map(|s| s.to_string()).collect();
        let cmd = parse_sidebar_args(&args).unwrap();
        match cmd {
            SidebarCommand::Add { position, command, .. } => {
                assert_eq!(position, PanelPosition::Left);
                assert_eq!(command, "filetree");
            }
            _ => panic!("expected Add"),
        }
    }

    #[test]
    fn test_parse_with_options() {
        let args: Vec<String> = vec!["left", "--weight", "2", "--max-width", "30", "filetree"]
            .iter().map(|s| s.to_string()).collect();
        let cmd = parse_sidebar_args(&args).unwrap();
        match cmd {
            SidebarCommand::Add { weight, max_size, .. } => {
                assert_eq!(weight, 2);
                assert_eq!(max_size, Some(30));
            }
            _ => panic!("expected Add"),
        }
    }

    #[test]
    fn test_parse_reset() {
        let args: Vec<String> = vec!["reset"].iter().map(|s| s.to_string()).collect();
        assert!(matches!(parse_sidebar_args(&args).unwrap(), SidebarCommand::Reset));
    }

    #[test]
    fn test_parse_reset_position() {
        let args: Vec<String> = vec!["reset", "left"].iter().map(|s| s.to_string()).collect();
        assert!(matches!(parse_sidebar_args(&args).unwrap(), SidebarCommand::ResetPosition(PanelPosition::Left)));
    }

    #[test]
    fn test_parse_outer() {
        let args: Vec<String> = vec!["bottom", "--outer", "logs"].iter().map(|s| s.to_string()).collect();
        match parse_sidebar_args(&args).unwrap() {
            SidebarCommand::Add { outer, .. } => assert!(outer),
            _ => panic!("expected Add"),
        }
    }

    #[test]
    fn test_outer_on_left_fails() {
        let args: Vec<String> = vec!["left", "--outer", "filetree"].iter().map(|s| s.to_string()).collect();
        assert!(parse_sidebar_args(&args).is_err());
    }
}
```

- [ ] **Step 3: Register sidebar in builtins.rs**

In `is_builtin_base()`, add `"sidebar"` to the match list.

In `execute_builtin()`, add:
```rust
"sidebar" => {
    let result = crate::sidebar::parse_sidebar_args(&args);
    match result {
        Ok(cmd) => {
            // Store the parsed command in the tuple space for the TUI to pick up
            let json = serde_json::to_string(&cmd).unwrap_or_default();
            tuple_space.out(colony_tuple::Tuple::from_values(vec![
                colony_tuple::Value::Str("sidebar_command".to_string()),
                colony_tuple::Value::Str(json),
            ]));
            Some(Ok(BuiltinResult::Success))
        }
        Err(e) => {
            eprintln!("sidebar: {}", e);
            Some(Ok(BuiltinResult::Failure(1)))
        }
    }
}
```

Note: The `sidebar` builtin emits a tuple rather than directly manipulating the panel host. The TUI event loop picks up `sidebar_command` tuples and applies them to the `PanelHost`. This decouples the builtin from the TUI.

- [ ] **Step 4: Add `mod sidebar;` to lib.rs or builtins module**

Add `pub mod sidebar;` to `crates/colony-exec/src/lib.rs`.

- [ ] **Step 5: Run tests**

Run: `cargo test -p colony-exec -- sidebar`
Expected: All sidebar tests PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/colony-exec/src/sidebar.rs crates/colony-exec/src/builtins.rs crates/colony-exec/src/lib.rs crates/colony-exec/Cargo.toml
git commit -m "feat(exec): sidebar builtin command with argument parsing"
```

---

### Task 8: `.colony/init` loading

**Files:**
- Create: `crates/colony-shell/src/init.rs`
- Modify: `crates/colony-shell/src/shell/orchestrator.rs`

- [ ] **Step 1: Create init.rs**

```rust
//! Load and execute `.colony/init` startup scripts.
//!
//! Layered: `~/.colony/init` runs first (global), then `.colony/init`
//! in the current directory runs second (project-specific).

use std::path::{Path, PathBuf};

/// Discover init scripts to run, in order.
pub fn discover_init_scripts() -> Vec<PathBuf> {
    let mut scripts = Vec::new();

    // 1. Global: ~/.colony/init
    if let Some(home) = dirs::home_dir() {
        let global = home.join(".colony").join("init");
        if global.is_file() {
            scripts.push(global);
        }
    }

    // 2. Project: .colony/init in current directory
    if let Ok(cwd) = std::env::current_dir() {
        let project = cwd.join(".colony").join("init");
        if project.is_file() {
            // Don't run the same file twice (if cwd is home)
            if scripts.last().map_or(true, |last| last != &project) {
                scripts.push(project);
            }
        }
    }

    scripts
}

/// Read an init script and return its lines as commands to execute.
pub fn read_init_script(path: &Path) -> Vec<String> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };

    content
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_read_init_script() {
        let dir = TempDir::new().unwrap();
        let init = dir.path().join("init");
        fs::write(&init, "# comment\nsidebar left filetree\n\nalias ll=\"ls -la\"\n").unwrap();

        let lines = read_init_script(&init);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "sidebar left filetree");
        assert_eq!(lines[1], "alias ll=\"ls -la\"");
    }

    #[test]
    fn test_read_missing_script() {
        let lines = read_init_script(Path::new("/nonexistent/init"));
        assert!(lines.is_empty());
    }
}
```

- [ ] **Step 2: Wire init loading into orchestrator**

In `crates/colony-shell/src/shell/orchestrator.rs`, after loading `.colonyrc`, add:

```rust
// Load and execute .colony/init scripts
let init_scripts = crate::init::discover_init_scripts();
for script_path in &init_scripts {
    let commands = crate::init::read_init_script(script_path);
    for line in commands {
        if let Err(e) = execute_shell_command(&line, &mut executor).await {
            eprintln!("init: {}: {}", script_path.display(), e);
        }
    }
}
```

- [ ] **Step 3: Add `mod init;` to shell module**

- [ ] **Step 4: Add `dirs` dependency if not present**

Check if `dirs` crate is in workspace dependencies. If not, add `dirs = "5"` to workspace deps and colony-shell's Cargo.toml.

- [ ] **Step 5: Run tests**

Run: `cargo test -p colony-shell -- init`
Expected: All tests PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/colony-shell/src/init.rs crates/colony-shell/src/shell/orchestrator.rs crates/colony-shell/src/shell/mod.rs
git commit -m "feat(shell): load .colony/init startup scripts"
```

---

### Task 9: Integrate PanelHost into repl_tui.rs

This is the largest single task. Replace `SidebarManager` with `PanelHost` for panel rendering and input routing.

**Files:**
- Modify: `crates/colony-shell/src/repl_tui.rs`
- Modify: `crates/colony-shell/Cargo.toml`

- [ ] **Step 1: Add colony-panel dependency to colony-shell**

In `crates/colony-shell/Cargo.toml`, add:
```toml
colony-panel = { path = "../colony-panel" }
```

- [ ] **Step 2: Replace SidebarManager with PanelHost in run_tui()**

Replace:
```rust
let mut sidebars = SidebarManager::new();
sidebars.register(Arc::new(FileTreePanel::new()));
let mut sidebar_state = SidebarState::new();
```

With:
```rust
let mut panel_host = PanelHost::new();
```

Remove `SidebarState`, `Focus` enum (use `colony_panel::host::Focus`), and all `SidebarManager` imports.

- [ ] **Step 3: Update run_repl_loop signature**

Replace `sidebars: &mut SidebarManager, sidebar_state: &mut SidebarState, focus: &mut Focus` with `panel_host: &mut PanelHost`.

- [ ] **Step 4: Update draw_tui to use PanelHost**

Replace the current layout/rendering logic with:

```rust
fn draw_tui(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    prompt_manager: &PromptManager,
    ctx: &ComponentContext,
    editor: &InputEditor,
    panel_host: &PanelHost,
    scrollback: &Scrollback,
    status_message: &str,
) -> Result<()> {
    terminal
        .draw(|frame| {
            let area = frame.area();
            let layout = panel_host.resolve_layout(area);

            // Render panels
            for (id, panel_area) in &layout.panels {
                let lines = panel_host.current_frame(*id);
                let focused = panel_host.focus == Focus::Panel(*id);
                render_panel_frame(frame, *panel_area, lines, focused);
            }

            // Render main area (scrollback + prompt + status)
            render_main_area(frame, layout.main, prompt_manager, ctx, editor, scrollback, status_message, panel_host.focus == Focus::Main);
        })
        .map_err(|e| miette::miette!("TUI render error: {}", e))?;
    Ok(())
}
```

- [ ] **Step 5: Add render_panel_frame function**

```rust
fn render_panel_frame(
    frame: &mut ratatui::Frame,
    area: Rect,
    lines: &[FrameLine],
    focused: bool,
) {
    use colony_panel::protocol::FrameLine;

    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .style(Style::default().bg(Color::Rgb(20, 20, 35)));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let ratatui_lines: Vec<Line> = lines.iter().map(|fl| {
        let style = fl.style_spec().to_ratatui_style();
        let mut spans = Vec::new();
        if fl.indent > 0 {
            spans.push(Span::raw("  ".repeat(fl.indent)));
        }
        if let Some(ref icon) = fl.icon {
            spans.push(Span::styled(format!("{} ", icon), Style::default().fg(Color::DarkGray)));
        }
        if fl.expandable {
            let indicator = if fl.expanded { " " } else { " " };
            spans.push(Span::styled(indicator, Style::default().fg(Color::DarkGray)));
        }
        spans.push(Span::styled(fl.text.clone(), style));
        Line::from(spans)
    }).collect();

    let paragraph = Paragraph::new(ratatui_lines);
    frame.render_widget(paragraph, inner);
}
```

- [ ] **Step 6: Update input handling for panel focus**

Replace the sidebar key handling section with panel host routing:

```rust
// In the event loop:
if key.code == KeyCode::Tab {
    let prompt_empty = editor.text().is_empty();
    if prompt_empty || panel_host.focus != Focus::Main {
        panel_host.cycle_focus();
        // Send focus events
        // ...redraw...
        continue;
    }
}

// Route keys to focused panel
if panel_host.focus != Focus::Main {
    let event = key_to_input_event(key);
    panel_host.send_to_focused(&event).await.ok();
    // ...redraw...
    continue;
}
```

- [ ] **Step 7: Add sidebar_command tuple handling in the REPL loop**

After each command execution, check for `sidebar_command` tuples:

```rust
// Check for sidebar commands from the tuple space
while let Some(tuple) = executor.tuple_space().inp(colony_tuple::Tuple::from_values(vec![
    colony_tuple::Value::Str("sidebar_command".to_string()),
    colony_tuple::Value::Wildcard,
])) {
    if let Some(colony_tuple::Value::Str(json)) = tuple.fields.get(1) {
        if let Ok(cmd) = serde_json::from_str::<SidebarCommand>(json) {
            match cmd {
                SidebarCommand::Reset => panel_host.reset().await,
                SidebarCommand::ResetPosition(pos) => panel_host.reset_position(pos).await,
                SidebarCommand::Add { position, command, args, weight, min_size, max_size, outer } => {
                    let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
                    let min = min_size.unwrap_or(match position {
                        PanelPosition::Left | PanelPosition::Right => 15,
                        PanelPosition::Top | PanelPosition::Bottom => 3,
                    });
                    let max = max_size.unwrap_or(match position {
                        PanelPosition::Left | PanelPosition::Right => 40,
                        PanelPosition::Top | PanelPosition::Bottom => 15,
                    });
                    panel_host.add_panel(&command, &args_refs, position, weight, min, max, outer).await.ok();
                }
            }
        }
    }
}
```

- [ ] **Step 8: Remove old sidebar code**

Remove from repl_tui.rs:
- `SidebarState` struct and impl
- `Focus` enum (old version)
- `SidebarAction` enum
- `ContextMenu` struct
- `render_sidebar_data()`, `get_node_path_at_cursor()`, `is_node_expanded_at_cursor()`, `is_cd_parent_at_cursor()`, `handle_sidebar_key()`, `render_context_menu()`, `shell_escape()`
- All `SidebarManager` and `FileTreePanel` imports

- [ ] **Step 9: Poll panels in the event loop**

In the main event loop, alongside `event::poll`, poll the panel host:

```rust
// Poll panel processes for new frames
let messages = panel_host.poll_panels().await;
for (id, msg) in messages {
    match msg {
        PanelMessage::Frame(_) => needs_redraw = true,
        PanelMessage::Emit(emit) => {
            // Forward to tuple space
            let tuple = colony_tuple::Tuple::from_values(
                emit.tuple.into_iter().map(colony_tuple::Value::Str).collect()
            );
            executor.tuple_space().out(tuple);
            // Broadcast to other panels
            panel_host.broadcast_tuple(&emit.tuple).await;
        }
        PanelMessage::Config(_) => {}
    }
}
```

- [ ] **Step 10: Build and fix compilation errors**

Run: `cargo build --bin colony`
Fix any remaining compilation issues iteratively.

- [ ] **Step 11: Commit**

```bash
git add crates/colony-shell/src/repl_tui.rs crates/colony-shell/Cargo.toml
git commit -m "feat(tui): replace SidebarManager with PanelHost for panel-as-process rendering"
```

---

## Chunk 3: filetree Panel Binary

### Task 10: Create colony-filetree-bin crate

**Files:**
- Create: `crates/colony-filetree-bin/Cargo.toml`
- Create: `crates/colony-filetree-bin/src/main.rs`
- Modify: `Cargo.toml` (workspace root)

- [ ] **Step 1: Create Cargo.toml**

```toml
[package]
name = "colony-filetree-bin"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "filetree"
path = "src/main.rs"

[dependencies]
colony-panel = { path = "../colony-panel" }
colony-filetree = { path = "../colony-filetree" }
serde_json = { workspace = true }
tokio = { workspace = true, features = ["io-util", "io-std", "macros", "rt-multi-thread"] }
```

- [ ] **Step 2: Add to workspace members**

- [ ] **Step 3: Create main.rs**

```rust
//! filetree: a panel binary that displays a navigable file tree.
//!
//! Emits frames to stdout following the panel protocol.
//! Publishes ("filetree_select", path) via ---EMIT--- when selection changes.
//! Reads key events from stdin for navigation.

use colony_panel::protocol::*;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

mod tree;

fn main() {
    let mut state = tree::TreeState::new();
    let stdout = io::stdout();
    let stdin = io::stdin();

    // Emit config with subscriptions
    println!("{}", CONFIG_DELIMITER);
    println!("{}", serde_json::to_string(&PanelConfig {
        subscriptions: vec![
            vec!["dir_change".to_string(), "*".to_string(), "*".to_string(), "*".to_string()],
        ],
    }).unwrap());

    // Initial render
    emit_frame(&stdout, &state);

    // Event loop: read stdin for events
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let Ok(event) = serde_json::from_str::<InputEvent>(&line) else { continue };

        match event {
            InputEvent::Key { ref code, ref modifiers } => {
                let changed = handle_key(&mut state, code, modifiers.as_deref());
                if changed {
                    emit_frame(&stdout, &state);
                    // Emit selection change
                    if let Some(path) = state.selected_path() {
                        emit_tuple(&stdout, &["filetree_select", &path.display().to_string()]);
                    }
                }
            }
            InputEvent::Tuple { ref data } => {
                // React to dir_change
                if data.first().map(|s| s.as_str()) == Some("dir_change") {
                    if let Some(new_dir) = data.get(2) {
                        state.set_root(PathBuf::from(new_dir));
                        emit_frame(&stdout, &state);
                    }
                }
            }
            InputEvent::Resize { width, height } => {
                state.set_size(width, height);
                emit_frame(&stdout, &state);
            }
            InputEvent::Shutdown => break,
            _ => {}
        }
    }
}

fn handle_key(state: &mut tree::TreeState, code: &str, _modifiers: Option<&str>) -> bool {
    match code {
        "j" | "Down" => state.cursor_down(),
        "k" | "Up" => state.cursor_up(),
        "Enter" | "l" | "Right" => state.toggle_or_enter(),
        "h" | "Left" => state.collapse_or_up(),
        "." => { state.toggle_hidden(); true }
        _ => false,
    }
}

fn emit_frame(stdout: &io::Stdout, state: &tree::TreeState) {
    let mut out = stdout.lock();
    writeln!(out, "{}", FRAME_DELIMITER).ok();
    for line in state.render_lines() {
        writeln!(out, "{}", serde_json::to_string(&line).unwrap()).ok();
    }
    writeln!(out, "{}", FRAME_DELIMITER).ok();
    out.flush().ok();
}

fn emit_tuple(stdout: &io::Stdout, values: &[&str]) {
    let mut out = stdout.lock();
    writeln!(out, "{}", EMIT_DELIMITER).ok();
    let emit = TupleEmit {
        tuple: values.iter().map(|s| s.to_string()).collect(),
    };
    writeln!(out, "{}", serde_json::to_string(&emit).unwrap()).ok();
    out.flush().ok();
}
```

- [ ] **Step 4: Create tree.rs (file tree state and rendering)**

```rust
//! File tree state management and rendering to FrameLine format.

use colony_panel::protocol::FrameLine;
use std::path::{Path, PathBuf};

pub struct TreeState {
    root: PathBuf,
    entries: Vec<TreeEntry>,
    cursor: usize,
    show_hidden: bool,
    width: u16,
    height: u16,
}

struct TreeEntry {
    name: String,
    path: PathBuf,
    is_dir: bool,
    depth: usize,
    expanded: bool,
}

impl TreeState {
    pub fn new() -> Self {
        let root = std::env::current_dir().unwrap_or_default();
        let mut state = Self {
            root: root.clone(),
            entries: Vec::new(),
            cursor: 0,
            show_hidden: false,
            width: 25,
            height: 40,
        };
        state.rebuild();
        state
    }

    pub fn set_root(&mut self, root: PathBuf) {
        self.root = root;
        self.cursor = 0;
        self.rebuild();
    }

    pub fn set_size(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
    }

    pub fn selected_path(&self) -> Option<&Path> {
        // Account for ".." entry at index 0
        if self.cursor == 0 {
            self.root.parent().map(|p| p as &Path)
        } else {
            self.entries.get(self.cursor - 1).map(|e| e.path.as_path())
        }
    }

    pub fn cursor_down(&mut self) -> bool {
        if self.cursor < self.entries.len() {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    pub fn cursor_up(&mut self) -> bool {
        if self.cursor > 0 {
            self.cursor -= 1;
            true
        } else {
            false
        }
    }

    pub fn toggle_or_enter(&mut self) -> bool {
        if self.cursor == 0 {
            // ".." entry — cd to parent
            if let Some(parent) = self.root.parent().map(|p| p.to_path_buf()) {
                self.set_root(parent);
            }
            return true;
        }
        let idx = self.cursor - 1;
        let Some(entry) = self.entries.get_mut(idx) else { return false };
        if entry.is_dir {
            entry.expanded = !entry.expanded;
            self.rebuild_preserving_expansion();
            true
        } else {
            false
        }
    }

    pub fn collapse_or_up(&mut self) -> bool {
        if self.cursor == 0 { return false; }
        let idx = self.cursor - 1;
        let Some(entry) = self.entries.get(idx) else { return false };
        if entry.is_dir && entry.expanded {
            self.entries[idx].expanded = false;
            self.rebuild_preserving_expansion();
            true
        } else {
            self.cursor_up()
        }
    }

    pub fn toggle_hidden(&mut self) {
        self.show_hidden = !self.show_hidden;
        self.rebuild();
    }

    pub fn render_lines(&self) -> Vec<FrameLine> {
        let mut lines = Vec::new();

        // ".." parent entry
        if self.root.parent().is_some() {
            lines.push(FrameLine {
                text: "..".to_string(),
                icon: Some("\u{f0162}".to_string()), // nerd font up arrow
                style: Some("dim".to_string()),
                indent: 0,
                action: Some("cd_parent".to_string()),
                expandable: false,
                expanded: false,
            });
        }

        for entry in &self.entries {
            let (icon, style) = if entry.is_dir {
                let icon = if entry.expanded { "\u{f024b}" } else { "\u{f024b}" };
                (icon, "bold,blue")
            } else {
                (file_icon(&entry.name), "white")
            };

            lines.push(FrameLine {
                text: entry.name.clone(),
                icon: Some(icon.to_string()),
                style: Some(style.to_string()),
                indent: entry.depth,
                action: Some(format!("open:{}", entry.path.display())),
                expandable: entry.is_dir,
                expanded: entry.expanded,
            });
        }

        lines
    }

    fn rebuild(&mut self) {
        self.entries.clear();
        self.scan_dir(&self.root.clone(), 0);
    }

    fn rebuild_preserving_expansion(&mut self) {
        let expanded: std::collections::HashSet<PathBuf> = self.entries.iter()
            .filter(|e| e.expanded)
            .map(|e| e.path.clone())
            .collect();
        self.entries.clear();
        self.scan_dir_with_expanded(&self.root.clone(), 0, &expanded);
    }

    fn scan_dir(&mut self, dir: &Path, depth: usize) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        let mut items: Vec<_> = entries.filter_map(|e| e.ok()).collect();
        items.sort_by(|a, b| {
            let a_dir = a.path().is_dir();
            let b_dir = b.path().is_dir();
            b_dir.cmp(&a_dir).then(a.file_name().cmp(&b.file_name()))
        });

        for item in items {
            let name = item.file_name().to_string_lossy().to_string();
            if !self.show_hidden && name.starts_with('.') { continue; }
            self.entries.push(TreeEntry {
                name,
                path: item.path(),
                is_dir: item.path().is_dir(),
                depth,
                expanded: false,
            });
        }
    }

    fn scan_dir_with_expanded(
        &mut self,
        dir: &Path,
        depth: usize,
        expanded: &std::collections::HashSet<PathBuf>,
    ) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        let mut items: Vec<_> = entries.filter_map(|e| e.ok()).collect();
        items.sort_by(|a, b| {
            let a_dir = a.path().is_dir();
            let b_dir = b.path().is_dir();
            b_dir.cmp(&a_dir).then(a.file_name().cmp(&b.file_name()))
        });

        for item in items {
            let name = item.file_name().to_string_lossy().to_string();
            if !self.show_hidden && name.starts_with('.') { continue; }
            let path = item.path();
            let is_dir = path.is_dir();
            let is_expanded = is_dir && expanded.contains(&path);

            self.entries.push(TreeEntry {
                name,
                path: path.clone(),
                is_dir,
                depth,
                expanded: is_expanded,
            });

            if is_expanded {
                self.scan_dir_with_expanded(&path, depth + 1, expanded);
            }
        }
    }
}

fn file_icon(name: &str) -> &'static str {
    let ext = name.rsplit('.').next().unwrap_or("");
    match ext {
        "rs" => "\u{e7a8}",
        "toml" | "json" | "yaml" | "yml" => "\u{e60b}",
        "md" | "txt" => "\u{f0219}",
        "sh" | "bash" | "zsh" => "\u{e795}",
        "py" => "\u{e606}",
        "js" | "ts" => "\u{e781}",
        "go" => "\u{e627}",
        "c" | "cpp" | "h" => "\u{e61e}",
        _ => "\u{f0214}",
    }
}
```

- [ ] **Step 5: Build the filetree binary**

Run: `cargo build --bin filetree`
Expected: Compiles successfully.

- [ ] **Step 6: Smoke test**

Run: `echo '{"event":"shutdown"}' | ./target/debug/filetree`
Expected: Outputs a `---CONFIG---` block, then a `---FRAME---` with file listings, then exits.

- [ ] **Step 7: Commit**

```bash
git add crates/colony-filetree-bin/ Cargo.toml
git commit -m "feat(filetree): standalone panel binary for file tree browsing"
```

---

### Task 11: End-to-end test

- [ ] **Step 1: Create .colony/init with sidebar command**

```bash
mkdir -p ~/.colony
echo 'sidebar left filetree' > ~/.colony/init
```

- [ ] **Step 2: Build and run colony**

```bash
cargo build --bin colony --bin filetree
./target/debug/colony
```

Expected: Colony launches with a file tree panel on the left. Tab switches focus. j/k navigates. Enter expands directories.

- [ ] **Step 3: Test sidebar reset**

In the colony shell, run: `sidebar reset`
Expected: File tree panel disappears.

Run: `sidebar left filetree`
Expected: File tree panel reappears.

- [ ] **Step 4: Commit any fixes**

```bash
git commit -am "fix: end-to-end panel integration fixes"
```

---

## Future Work (Separate Plans)

The following are not covered by this plan and should be separate implementation plans:

1. **filedetails panel binary** — subscribes to `filetree_select`, shows file metadata
2. **preview panel binary** — syntax-highlighted file preview
3. **gitstatus panel binary** — modified/staged/untracked files
4. **gitstage panel binary** — inline diff with stage/unstage
5. **jobs, history, env, logs, session panel binaries**
6. **Panel selection highlighting and horizontal scroll** — port the existing scroll animation to the new panel renderer
7. **Panel-specific actions** — `i` to insert path, `o` to open editor, `p` to preview (currently in repl_tui.rs sidebar handling, needs to be protocol-driven)
8. **Context menu support** — protocol extension for menus
