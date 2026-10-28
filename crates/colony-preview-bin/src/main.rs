//! preview: a panel binary that displays a read-only preview of file content.
//!
//! Subscribes to `filetree_select` tuples and renders the first N lines
//! of the selected file (fitting within the panel height). Supports j/k
//! scrolling when focused.

use colony_panel::protocol::{FrameLine, InputEvent, PanelConfig, SENTINEL_CONFIG, SENTINEL_FRAME};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

fn main() {
    let mut state = PreviewState::new();
    let stdout = io::stdout();
    let stdin = io::stdin();

    // Emit config with subscriptions
    {
        let mut out = stdout.lock();
        writeln!(out, "{SENTINEL_CONFIG}").ok();
        let config = PanelConfig {
            subscriptions: vec!["filetree_select".to_string()],
            help: Vec::new(),
        };
        writeln!(
            out,
            "{}",
            serde_json::to_string(&config).expect("config must serialize")
        )
        .ok();
        out.flush().ok();
    }

    // Initial render (empty)
    emit_frame(&stdout, &state);

    // Event loop
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let Ok(event) = serde_json::from_str::<InputEvent>(&line) else {
            continue;
        };

        match event {
            InputEvent::Key {
                ref key,
                modifiers: _,
            } => {
                let changed = handle_key(&mut state, key);
                if changed {
                    emit_frame(&stdout, &state);
                }
            }
            InputEvent::Tuple { ref data } => {
                let Some(key) = data.get("key").and_then(|k| k.as_str()) else {
                    continue;
                };
                if key == "filetree_select" {
                    if let Some(path_str) = data.get("value").and_then(|v| v.as_str()) {
                        state.load_file(PathBuf::from(path_str));
                        emit_frame(&stdout, &state);
                    }
                }
            }
            InputEvent::Resize { width, height } => {
                state.set_size(width, height);
                emit_frame(&stdout, &state);
            }
            InputEvent::Shutdown => break,
            InputEvent::Focus { .. } => {}
        }
    }
}

fn handle_key(state: &mut PreviewState, key: &str) -> bool {
    match key {
        "j" | "Down" => state.scroll_down(),
        "k" | "Up" => state.scroll_up(),
        _ => false,
    }
}

fn emit_frame(stdout: &io::Stdout, state: &PreviewState) {
    let mut out = stdout.lock();
    writeln!(out, "{SENTINEL_FRAME}").ok();
    for line in state.render_lines() {
        writeln!(
            out,
            "{}",
            serde_json::to_string(&line).expect("frame line must serialize")
        )
        .ok();
    }
    writeln!(out, "{SENTINEL_FRAME}").ok();
    out.flush().ok();
}

// ---------------------------------------------------------------------------
// Preview state
// ---------------------------------------------------------------------------

/// Content loaded from a file (or a placeholder for special cases).
enum PreviewContent {
    /// No file selected yet.
    Empty,
    /// Plain text lines from a readable file.
    Lines(Vec<String>),
    /// Directory listing entries.
    Directory(Vec<String>),
    /// Binary file placeholder.
    Binary,
    /// Error reading the file.
    Error(String),
}

struct PreviewState {
    /// Path currently being previewed.
    current_path: Option<PathBuf>,
    /// Loaded content.
    content: PreviewContent,
    /// Scroll offset (first visible line after the header).
    scroll: usize,
    /// Panel dimensions.
    width: u16,
    height: u16,
}

impl PreviewState {
    fn new() -> Self {
        Self {
            current_path: None,
            content: PreviewContent::Empty,
            scroll: 0,
            width: 60,
            height: 30,
        }
    }

    fn set_size(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
    }

    fn load_file(&mut self, path: PathBuf) {
        self.scroll = 0;
        self.content = load_content(&path);
        self.current_path = Some(path);
    }

    fn content_len(&self) -> usize {
        match &self.content {
            PreviewContent::Empty => 0,
            PreviewContent::Lines(lines) => lines.len(),
            PreviewContent::Directory(entries) => entries.len(),
            PreviewContent::Binary | PreviewContent::Error(_) => 1,
        }
    }

    /// Number of visible content lines (height minus the header line).
    fn visible_lines(&self) -> usize {
        self.height.saturating_sub(1) as usize
    }

    fn scroll_down(&mut self) -> bool {
        let max_scroll = self.content_len().saturating_sub(self.visible_lines());
        if self.scroll < max_scroll {
            self.scroll += 1;
            true
        } else {
            false
        }
    }

    fn scroll_up(&mut self) -> bool {
        if self.scroll > 0 {
            self.scroll -= 1;
            true
        } else {
            false
        }
    }

    fn render_lines(&self) -> Vec<FrameLine> {
        let mut lines = Vec::new();

        // Header: file name (bold)
        let header_text = self
            .current_path
            .as_deref()
            .map_or_else(|| "(no file selected)".to_string(), display_filename);

        lines.push(FrameLine {
            text: header_text,
            icon: None,
            style: Some("bold".to_string()),
            indent: None,
            action: None,
            expandable: None,
            expanded: None,
            selected: None,
        });

        let visible = self.visible_lines();

        match &self.content {
            PreviewContent::Empty => {}
            PreviewContent::Binary => {
                lines.push(make_plain_line("[binary file]".to_string()));
            }
            PreviewContent::Error(msg) => {
                lines.push(FrameLine {
                    text: msg.clone(),
                    icon: None,
                    style: Some("red".to_string()),
                    indent: None,
                    action: None,
                    expandable: None,
                    expanded: None,
                    selected: None,
                });
            }
            PreviewContent::Lines(content_lines) => {
                for (i, line) in content_lines
                    .iter()
                    .skip(self.scroll)
                    .take(visible)
                    .enumerate()
                {
                    let line_num = self.scroll + i + 1;
                    lines.push(FrameLine {
                        text: format!("{line_num:4} {line}"),
                        icon: None,
                        style: Some("white".to_string()),
                        indent: None,
                        action: None,
                        expandable: None,
                        expanded: None,
                        selected: None,
                    });
                }
            }
            PreviewContent::Directory(entries) => {
                for entry in entries.iter().skip(self.scroll).take(visible) {
                    lines.push(make_plain_line(entry.clone()));
                }
            }
        }

        lines
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Load file content, detecting binary files and directories.
fn load_content(path: &Path) -> PreviewContent {
    if path.is_dir() {
        return load_directory(path);
    }

    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => return PreviewContent::Error(format!("error: {e}")),
    };

    // Heuristic: if the first 8192 bytes contain a NUL, treat as binary.
    let check_len = bytes.len().min(8192);
    if bytes[..check_len].contains(&0) {
        return PreviewContent::Binary;
    }

    let text = String::from_utf8_lossy(&bytes);
    let lines: Vec<String> = text.lines().map(String::from).collect();
    PreviewContent::Lines(lines)
}

/// Load a directory listing.
fn load_directory(path: &Path) -> PreviewContent {
    let entries = match std::fs::read_dir(path) {
        Ok(rd) => rd,
        Err(e) => return PreviewContent::Error(format!("error: {e}")),
    };

    let mut names: Vec<String> = entries
        .filter_map(|e| e.ok())
        .map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if e.path().is_dir() {
                format!("{name}/")
            } else {
                name
            }
        })
        .collect();
    names.sort();
    PreviewContent::Directory(names)
}

/// Extract just the filename (or full path if no filename component).
fn display_filename(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().to_string(),
    )
}

/// Create a plain unstyled `FrameLine`.
fn make_plain_line(text: String) -> FrameLine {
    FrameLine {
        text,
        icon: None,
        style: None,
        indent: None,
        action: None,
        expandable: None,
        expanded: None,
        selected: None,
    }
}
