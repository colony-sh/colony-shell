//! gitstage: a panel binary that shows inline diffs and supports staging/unstaging.
//!
//! Subscribes to `filetree_select` and `gitfiles_select` tuples. When a file
//! path is received, runs `git diff` and renders the output with color coding.
//! Supports `s` to stage and `u` to unstage the current file, re-emitting a
//! `git_change` tuple afterward.

use colony_panel::protocol::{
    FrameLine, InputEvent, PanelConfig, TupleEmit, SENTINEL_CONFIG, SENTINEL_EMIT, SENTINEL_FRAME,
};
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let mut state = DiffState::new();
    let stdout = io::stdout();
    let stdin = io::stdin();

    // Emit config with subscriptions
    {
        let mut out = stdout.lock();
        writeln!(out, "{SENTINEL_CONFIG}").ok();
        let config = PanelConfig {
            subscriptions: vec![
                "filetree_select".to_string(),
                "gitfiles_select".to_string(),
            ],
            help: Vec::new(),
        };
        writeln!(out, "{}", serde_json::to_string(&config).expect("config must serialize")).ok();
        out.flush().ok();
    }

    // Initial render
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
                ref modifiers,
            } => {
                let action = handle_key(&mut state, key, modifiers);
                match action {
                    KeyAction::Render => emit_frame(&stdout, &state),
                    KeyAction::StageAndRender => {
                        emit_frame(&stdout, &state);
                        emit_tuple(&stdout, "git_change", &state);
                    }
                    KeyAction::None => {}
                }
            }
            InputEvent::Tuple { ref data } => {
                let Some(key) = data.get("key").and_then(|k| k.as_str()) else {
                    continue;
                };

                let path = match key {
                    "filetree_select" | "gitfiles_select" => {
                        data.get("value").and_then(|v| v.as_str()).map(String::from)
                    }
                    _ => None,
                };

                if let Some(path) = path {
                    state.set_file(PathBuf::from(path));
                    emit_frame(&stdout, &state);
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

// ---------------------------------------------------------------------------
// Key handling
// ---------------------------------------------------------------------------

enum KeyAction {
    None,
    Render,
    StageAndRender,
}

fn handle_key(state: &mut DiffState, key: &str, _modifiers: &[String]) -> KeyAction {
    match key {
        "j" | "Down" => {
            if state.scroll_down() {
                KeyAction::Render
            } else {
                KeyAction::None
            }
        }
        "k" | "Up" => {
            if state.scroll_up() {
                KeyAction::Render
            } else {
                KeyAction::None
            }
        }
        "s" => {
            if state.stage_file() {
                KeyAction::StageAndRender
            } else {
                KeyAction::None
            }
        }
        "u" => {
            if state.unstage_file() {
                KeyAction::StageAndRender
            } else {
                KeyAction::None
            }
        }
        _ => KeyAction::None,
    }
}

// ---------------------------------------------------------------------------
// Frame emission
// ---------------------------------------------------------------------------

fn emit_frame(stdout: &io::Stdout, state: &DiffState) {
    let mut out = stdout.lock();
    writeln!(out, "{SENTINEL_FRAME}").ok();
    for line in state.render_lines() {
        writeln!(out, "{}", serde_json::to_string(&line).expect("frame line must serialize")).ok();
    }
    writeln!(out, "{SENTINEL_FRAME}").ok();
    out.flush().ok();
}

fn emit_tuple(stdout: &io::Stdout, key: &str, state: &DiffState) {
    let mut out = stdout.lock();
    writeln!(out, "{SENTINEL_EMIT}").ok();
    let path_str = state
        .file
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let emit = TupleEmit {
        key: key.to_string(),
        value: serde_json::json!({ "file": path_str }),
    };
    writeln!(out, "{}", serde_json::to_string(&emit).expect("tuple emit must serialize")).ok();
    out.flush().ok();
}

// ---------------------------------------------------------------------------
// Diff state
// ---------------------------------------------------------------------------

struct DiffState {
    file: Option<PathBuf>,
    diff_lines: Vec<DiffLine>,
    scroll_offset: usize,
    _width: u16,
    height: u16,
}

struct DiffLine {
    text: String,
    kind: LineKind,
}

enum LineKind {
    Added,
    Removed,
    Hunk,
    Context,
    Header,
}

impl DiffState {
    fn new() -> Self {
        Self {
            file: None,
            diff_lines: Vec::new(),
            scroll_offset: 0,
            _width: 80,
            height: 40,
        }
    }

    fn set_file(&mut self, path: PathBuf) {
        self.file = Some(path);
        self.scroll_offset = 0;
        self.refresh_diff();
    }

    fn set_size(&mut self, width: u16, height: u16) {
        self._width = width;
        self.height = height;
    }

    fn scroll_down(&mut self) -> bool {
        let max = self.diff_lines.len().saturating_sub(1);
        if self.scroll_offset < max {
            self.scroll_offset += 1;
            true
        } else {
            false
        }
    }

    fn scroll_up(&mut self) -> bool {
        if self.scroll_offset > 0 {
            self.scroll_offset -= 1;
            true
        } else {
            false
        }
    }

    fn stage_file(&mut self) -> bool {
        let Some(ref file) = self.file else {
            return false;
        };
        let status = Command::new("git")
            .arg("add")
            .arg(file)
            .status();
        let Ok(status) = status else { return false };
        if status.success() {
            self.refresh_diff();
            true
        } else {
            false
        }
    }

    fn unstage_file(&mut self) -> bool {
        let Some(ref file) = self.file else {
            return false;
        };
        let status = Command::new("git")
            .arg("restore")
            .arg("--staged")
            .arg(file)
            .status();
        let Ok(status) = status else { return false };
        if status.success() {
            self.refresh_diff();
            true
        } else {
            false
        }
    }

    fn refresh_diff(&mut self) {
        self.diff_lines.clear();

        let Some(ref file) = self.file else { return };

        // Try unstaged diff first, then staged diff
        let output = Command::new("git")
            .arg("diff")
            .arg("--")
            .arg(file)
            .output();

        let output = match output {
            Ok(o) if !o.stdout.is_empty() => o,
            _ => {
                // Fall back to staged diff
                let staged = Command::new("git")
                    .arg("diff")
                    .arg("--cached")
                    .arg("--")
                    .arg(file)
                    .output();
                match staged {
                    Ok(o) => o,
                    Err(_) => return,
                }
            }
        };

        let text = String::from_utf8_lossy(&output.stdout);
        if text.is_empty() {
            return;
        }

        for line in text.lines() {
            let kind = classify_line(line);
            self.diff_lines.push(DiffLine {
                text: line.to_string(),
                kind,
            });
        }
    }

    fn render_lines(&self) -> Vec<FrameLine> {
        let Some(ref file) = self.file else {
            return vec![FrameLine {
                text: "[no file selected]".to_string(),
                style: Some("dim".to_string()),
                icon: None,
                indent: None,
                action: None,
                expandable: None,
                expanded: None,
                selected: None,
            }];
        };

        if self.diff_lines.is_empty() {
            return vec![
                FrameLine {
                    text: file.display().to_string(),
                    style: Some("bold".to_string()),
                    icon: None,
                    indent: None,
                    action: None,
                    expandable: None,
                    expanded: None,
                    selected: None,
                },
                FrameLine {
                    text: "[no changes]".to_string(),
                    style: Some("dim".to_string()),
                    icon: None,
                    indent: None,
                    action: None,
                    expandable: None,
                    expanded: None,
                    selected: None,
                },
            ];
        }

        let visible_count = self.height as usize;
        self.diff_lines
            .iter()
            .skip(self.scroll_offset)
            .take(visible_count)
            .map(|dl| FrameLine {
                text: dl.text.clone(),
                style: Some(style_for_kind(&dl.kind).to_string()),
                icon: None,
                indent: None,
                action: None,
                expandable: None,
                expanded: None,
                selected: None,
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn classify_line(line: &str) -> LineKind {
    if line.starts_with("+++") || line.starts_with("---") || line.starts_with("diff ") {
        LineKind::Header
    } else if line.starts_with('+') {
        LineKind::Added
    } else if line.starts_with('-') {
        LineKind::Removed
    } else if line.starts_with("@@") {
        LineKind::Hunk
    } else {
        LineKind::Context
    }
}

fn style_for_kind(kind: &LineKind) -> &'static str {
    match kind {
        LineKind::Added => "green",
        LineKind::Removed => "red",
        LineKind::Hunk => "cyan",
        LineKind::Header => "bold",
        LineKind::Context => "white",
    }
}
