//! filetree: a panel binary that displays a navigable file tree.
//!
//! Emits frames to stdout following the panel protocol.
//! Publishes `filetree_select` via `---EMIT---` when selection changes.
//! Reads key events from stdin for navigation.

use colony_panel::protocol::{
    FrameLine, InputEvent, PanelConfig, TupleEmit, SENTINEL_CONFIG, SENTINEL_EMIT, SENTINEL_FRAME,
};
use std::collections::HashSet;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

fn main() {
    let mut state = TreeState::new();
    let stdout = io::stdout();
    let stdin = io::stdin();

    // Emit config with subscriptions
    {
        let mut out = stdout.lock();
        writeln!(out, "{SENTINEL_CONFIG}").ok();
        let config = PanelConfig {
            subscriptions: vec!["dir_change".to_string()],
            help: vec![
                "j/↓  Move down".to_string(),
                "k/↑  Move up".to_string(),
                "Enter/l/→  Expand / enter directory".to_string(),
                "h/←  Collapse / go up".to_string(),
                ".    Toggle hidden files".to_string(),
                "Tab  Switch focus".to_string(),
                "Esc  Return to prompt".to_string(),
            ],
        };
        writeln!(out, "{}", serde_json::to_string(&config).expect("config must serialize")).ok();
        out.flush().ok();
    }

    // Initial render
    emit_frame(&stdout, &state);

    // Event loop: read stdin for events
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
                let changed = handle_key(&mut state, key, modifiers);
                if changed {
                    emit_frame(&stdout, &state);
                    if let Some(path) = state.selected_path() {
                        emit_tuple(&stdout, "filetree_select", path);
                    }
                }
            }
            InputEvent::Tuple { ref data } => {
                // React to dir_change: {"key": "dir_change", "value": {"cwd": "/path"}}
                if let Some(key) = data.get("key").and_then(|k| k.as_str()) {
                    if key == "dir_change" {
                        if let Some(new_dir) = data
                            .get("value")
                            .and_then(|v| v.get("cwd"))
                            .and_then(|n| n.as_str())
                        {
                            state.set_root(PathBuf::from(new_dir));
                            emit_frame(&stdout, &state);
                        }
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

fn handle_key(state: &mut TreeState, key: &str, _modifiers: &[String]) -> bool {
    match key {
        "j" | "Down" => state.cursor_down(),
        "k" | "Up" => state.cursor_up(),
        "Enter" | "l" | "Right" => state.toggle_or_enter(),
        "h" | "Left" => state.collapse_or_up(),
        "." => {
            state.toggle_hidden();
            true
        }
        _ => false,
    }
}

fn emit_frame(stdout: &io::Stdout, state: &TreeState) {
    let mut out = stdout.lock();
    writeln!(out, "{SENTINEL_FRAME}").ok();
    for line in state.render_lines() {
        writeln!(out, "{}", serde_json::to_string(&line).expect("frame line must serialize")).ok();
    }
    writeln!(out, "{SENTINEL_FRAME}").ok();
    out.flush().ok();
}

fn emit_tuple(stdout: &io::Stdout, key: &str, path: &Path) {
    let mut out = stdout.lock();
    writeln!(out, "{SENTINEL_EMIT}").ok();
    let emit = TupleEmit {
        key: key.to_string(),
        value: serde_json::json!(path.display().to_string()),
    };
    writeln!(out, "{}", serde_json::to_string(&emit).expect("tuple emit must serialize")).ok();
    out.flush().ok();
}

// ---------------------------------------------------------------------------
// Tree state
// ---------------------------------------------------------------------------

struct TreeState {
    root: PathBuf,
    entries: Vec<TreeEntry>,
    cursor: usize,
    show_hidden: bool,
    _width: u16,
    _height: u16,
}

struct TreeEntry {
    name: String,
    path: PathBuf,
    is_dir: bool,
    depth: u16,
    expanded: bool,
}

impl TreeState {
    fn new() -> Self {
        let root = std::env::current_dir().unwrap_or_default();
        let mut state = Self {
            root: root.clone(),
            entries: Vec::new(),
            cursor: 0,
            show_hidden: false,
            _width: 25,
            _height: 40,
        };
        state.rebuild();
        state
    }

    fn set_root(&mut self, root: PathBuf) {
        self.root = root;
        self.cursor = 0;
        self.rebuild();
    }

    fn set_size(&mut self, width: u16, height: u16) {
        self._width = width;
        self._height = height;
    }

    fn selected_path(&self) -> Option<&Path> {
        // Account for ".." entry at index 0
        if self.cursor == 0 {
            self.root.parent().map(|p| p as &Path)
        } else {
            self.entries.get(self.cursor - 1).map(|e| e.path.as_path())
        }
    }

    fn cursor_down(&mut self) -> bool {
        if self.cursor < self.entries.len() {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    fn cursor_up(&mut self) -> bool {
        if self.cursor > 0 {
            self.cursor -= 1;
            true
        } else {
            false
        }
    }

    fn toggle_or_enter(&mut self) -> bool {
        if self.cursor == 0 {
            let Some(parent) = self.root.parent().map(|p| p.to_path_buf()) else {
                return false;
            };
            self.set_root(parent);
            return true;
        }
        let idx = self.cursor - 1;
        let Some(entry) = self.entries.get_mut(idx) else {
            return false;
        };
        if entry.is_dir {
            entry.expanded = !entry.expanded;
            self.rebuild_preserving_expansion();
            true
        } else {
            false
        }
    }

    fn collapse_or_up(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        let idx = self.cursor - 1;
        let Some(entry) = self.entries.get(idx) else {
            return false;
        };
        if entry.is_dir && entry.expanded {
            self.entries[idx].expanded = false;
            self.rebuild_preserving_expansion();
            true
        } else {
            self.cursor_up()
        }
    }

    fn toggle_hidden(&mut self) {
        self.show_hidden = !self.show_hidden;
        self.rebuild();
    }

    fn render_lines(&self) -> Vec<FrameLine> {
        let mut lines = Vec::new();
        let mut line_idx = 0usize;

        // ".." parent entry
        if self.root.parent().is_some() {
            lines.push(FrameLine {
                text: "..".to_string(),
                icon: Some("\u{f0162}".to_string()),
                style: Some("dim".to_string()),
                indent: None,
                action: Some("cd_parent".to_string()),
                expandable: None,
                expanded: None,
                selected: if self.cursor == line_idx {
                    Some(true)
                } else {
                    None
                },
            });
            line_idx += 1;
        }

        for entry in &self.entries {
            let (icon, style) = if entry.is_dir {
                let icon = if entry.expanded {
                    "\u{f07c}"
                } else {
                    "\u{f07b}"
                };
                (icon, "bold,blue")
            } else {
                (file_icon(&entry.name), "white")
            };

            lines.push(FrameLine {
                text: entry.name.clone(),
                icon: Some(icon.to_string()),
                style: Some(style.to_string()),
                indent: if entry.depth > 0 {
                    Some(entry.depth)
                } else {
                    None
                },
                action: Some(format!("open:{}", entry.path.display())),
                expandable: if entry.is_dir { Some(true) } else { None },
                expanded: if entry.is_dir {
                    Some(entry.expanded)
                } else {
                    None
                },
                selected: if self.cursor == line_idx {
                    Some(true)
                } else {
                    None
                },
            });
            line_idx += 1;
        }

        lines
    }

    fn rebuild(&mut self) {
        self.entries.clear();
        let root = self.root.clone();
        self.scan_dir(&root, 0);
    }

    fn rebuild_preserving_expansion(&mut self) {
        let expanded: HashSet<PathBuf> = self
            .entries
            .iter()
            .filter(|e| e.expanded)
            .map(|e| e.path.clone())
            .collect();
        self.entries.clear();
        let root = self.root.clone();
        self.scan_dir_with_expanded(&root, 0, &expanded);
    }

    fn scan_dir(&mut self, dir: &Path, depth: u16) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut items: Vec<_> = entries.filter_map(|e| e.ok()).collect();
        items.sort_by(|a, b| {
            let a_dir = a.path().is_dir();
            let b_dir = b.path().is_dir();
            b_dir
                .cmp(&a_dir)
                .then(a.file_name().cmp(&b.file_name()))
        });

        for item in items {
            let name = item.file_name().to_string_lossy().to_string();
            if !self.show_hidden && name.starts_with('.') {
                continue;
            }
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
        depth: u16,
        expanded: &HashSet<PathBuf>,
    ) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut items: Vec<_> = entries.filter_map(|e| e.ok()).collect();
        items.sort_by(|a, b| {
            let a_dir = a.path().is_dir();
            let b_dir = b.path().is_dir();
            b_dir
                .cmp(&a_dir)
                .then(a.file_name().cmp(&b.file_name()))
        });

        for item in items {
            let name = item.file_name().to_string_lossy().to_string();
            if !self.show_hidden && name.starts_with('.') {
                continue;
            }
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
    colony_filetree::file_icon_for_ext(ext)
}
